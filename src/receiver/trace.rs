use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tonic::{Request, Response, Status};

use crate::peer_router::PeerRouter;
use crate::peer_tracker::PeerTracker;
use crate::proto::trace_service::trace_service_server::TraceService;
use crate::proto::trace_service::{ExportTraceServiceRequest, ExportTraceServiceResponse};
use crate::server::Event;
use crate::session_tracker::SessionTracker;
use crate::types::{RoutingDecision, SessionParticipant, SessionRole};
use crate::write_buffer::{RecordType, WriteBufferSender, WriteRecord};
use tokio::sync::broadcast;

/// Rate limiter cooldown for trace sampling (1 event per second).
const TRACE_SAMPLE_COOLDOWN: Duration = Duration::from_secs(1);

/// gRPC service implementing the OTLP TraceService collector.
#[derive(Debug, Clone)]
pub struct TraceReceiver {
    pub peer_tracker: PeerTracker,
    pub session_tracker: SessionTracker,
    pub peer_router: PeerRouter,
    pub write_buffer: WriteBufferSender,
    pub event_tx: broadcast::Sender<Event>,
    /// Timestamp of the last emitted `TraceSampled` event for rate limiting.
    pub last_trace_sample: Arc<Mutex<Option<Instant>>>,
}

impl TraceReceiver {
    /// Returns `true` if enough time has elapsed since the last sample to emit another.
    pub fn should_sample_trace(&self) -> bool {
        let mut last = self.last_trace_sample.lock().unwrap();
        match *last {
            None => {
                *last = Some(Instant::now());
                true
            }
            Some(prev) if prev.elapsed() >= TRACE_SAMPLE_COOLDOWN => {
                *last = Some(Instant::now());
                true
            }
            Some(_) => false,
        }
    }
}

#[tonic::async_trait]
impl TraceService for TraceReceiver {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
        let req = request.into_inner();

        for resource_spans in &req.resource_spans {
            // Extract peer ID from resource attributes
            let peer_id = resource_spans
                .resource
                .as_ref()
                .and_then(|r| {
                    r.attributes.iter().find_map(|attr| {
                        if attr.key == "service.instance.id" || attr.key == "hopr.peer_id" {
                            attr.value.as_ref().and_then(|v| {
                                v.value.as_ref().map(|val| match val {
                                    crate::proto::common::any_value::Value::StringValue(s) => {
                                        s.clone()
                                    }
                                    _ => String::new(),
                                })
                            })
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default();

            if peer_id.is_empty() {
                tracing::warn!("trace received with empty peer_id, skipping resource_spans");
                continue;
            }

            tracing::debug!(peer_id = %peer_id, "trace export received");

            // Update peer presence
            self.peer_tracker.record_seen(&peer_id).await;
            let _ = self.event_tx.send(Event::PeerSeen {
                peer_id: peer_id.clone(),
            });

            // Extract HOPR session attributes from spans
            for scope_spans in &resource_spans.scope_spans {
                for span in &scope_spans.spans {
                    // Check for session attributes
                    let session_id = span.attributes.iter().find_map(|a| {
                        if a.key == "hopr.session.id" {
                            a.value.as_ref().and_then(|v| {
                                v.value.as_ref().map(|val| match val {
                                    crate::proto::common::any_value::Value::StringValue(s) => {
                                        s.clone()
                                    }
                                    _ => String::new(),
                                })
                            })
                        } else {
                            None
                        }
                    });

                    if let Some(sid) = &session_id {
                        if !sid.is_empty() {
                            let protocol =
                                extract_string_attr(&span.attributes, "hopr.session.protocol")
                                    .unwrap_or_default();
                            let hop_count = extract_int_attr(&span.attributes, "hopr.session.hops")
                                .unwrap_or(0) as u32;
                            let role_str =
                                extract_string_attr(&span.attributes, "hopr.session.role")
                                    .unwrap_or_default();
                            let role = match role_str.as_str() {
                                "entry" => SessionRole::Entry,
                                "exit" => SessionRole::Exit,
                                _ => SessionRole::Relay,
                            };

                            let participant = SessionParticipant {
                                peer_id: peer_id.clone(),
                                role,
                            };
                            self.session_tracker
                                .update_session(sid, &protocol, hop_count, participant)
                                .await;
                            let _ = self.event_tx.send(Event::SessionObserved {
                                session_id: sid.clone(),
                            });
                        }
                    }

                    // Check routing decision
                    let decision = self.peer_router.route(&peer_id).await;
                    let decision_label = match &decision {
                        RoutingDecision::Discard => "discard",
                        RoutingDecision::Retain { .. } => "retain",
                    };

                    // Emit a sampled trace event for the live inspector (rate-limited).
                    if self.should_sample_trace() {
                        let attributes = serde_json::json!(
                            span.attributes
                                .iter()
                                .filter_map(|a| {
                                    a.value.as_ref().and_then(|v| v.value.as_ref()).map(|val| {
                                        let v = match val {
                                            crate::proto::common::any_value::Value::StringValue(
                                                s,
                                            ) => serde_json::Value::String(s.clone()),
                                            crate::proto::common::any_value::Value::IntValue(i) => {
                                                serde_json::json!(i)
                                            }
                                            _ => serde_json::Value::Null,
                                        };
                                        (a.key.clone(), v)
                                    })
                                })
                                .collect::<serde_json::Map<String, serde_json::Value>>()
                        );

                        let _ = self.event_tx.send(Event::TraceSampled {
                            timestamp: chrono::Utc::now().to_rfc3339(),
                            peer_id: peer_id.clone(),
                            span_name: span.name.clone(),
                            trace_id: hex::encode(&span.trace_id),
                            span_id: hex::encode(&span.span_id),
                            routing_decision: decision_label.to_string(),
                            attributes,
                        });
                    }

                    match decision {
                        RoutingDecision::Discard => {
                            tracing::debug!(peer_id = %peer_id, "trace span discarded by routing");
                        }
                        RoutingDecision::Retain { session_ids } => {
                            tracing::debug!(
                                peer_id = %peer_id,
                                session_count = session_ids.len(),
                                "trace span retained for debug sessions"
                            );
                            let payload = serde_json::json!({
                                "name": span.name,
                                "traceId": hex::encode(&span.trace_id),
                                "spanId": hex::encode(&span.span_id),
                                "parentSpanId": hex::encode(&span.parent_span_id),
                                "startTimeUnixNano": span.start_time_unix_nano,
                                "endTimeUnixNano": span.end_time_unix_nano,
                            });

                            for session_id in session_ids {
                                self.write_buffer.try_send(WriteRecord {
                                    debug_session_id: session_id,
                                    peer_id: peer_id.clone(),
                                    record_type: RecordType::Span,
                                    payload: payload.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}

fn extract_string_attr(attrs: &[crate::proto::common::KeyValue], key: &str) -> Option<String> {
    attrs.iter().find_map(|a| {
        if a.key == key {
            a.value.as_ref().and_then(|v| {
                v.value.as_ref().map(|val| match val {
                    crate::proto::common::any_value::Value::StringValue(s) => s.clone(),
                    _ => String::new(),
                })
            })
        } else {
            None
        }
    })
}

fn extract_int_attr(attrs: &[crate::proto::common::KeyValue], key: &str) -> Option<i64> {
    attrs.iter().find_map(|a| {
        if a.key == key {
            a.value.as_ref().and_then(|v| {
                v.value.as_ref().and_then(|val| match val {
                    crate::proto::common::any_value::Value::IntValue(i) => Some(*i),
                    _ => None,
                })
            })
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test the rate limiter directly without constructing a full TraceReceiver.
    /// The rate limiter logic only depends on `Arc<Mutex<Option<Instant>>>`.
    fn make_rate_limiter() -> Arc<Mutex<Option<Instant>>> {
        Arc::new(Mutex::new(None))
    }

    /// Replicate the rate limiter check logic for isolated unit testing.
    fn should_sample(last: &Arc<Mutex<Option<Instant>>>) -> bool {
        let mut guard = last.lock().unwrap();
        match *guard {
            None => {
                *guard = Some(Instant::now());
                true
            }
            Some(prev) if prev.elapsed() >= TRACE_SAMPLE_COOLDOWN => {
                *guard = Some(Instant::now());
                true
            }
            Some(_) => false,
        }
    }

    #[test]
    fn rate_limiter_returns_true_on_first_call() {
        let limiter = make_rate_limiter();
        assert!(should_sample(&limiter), "first call should return true");
    }

    #[test]
    fn rate_limiter_returns_false_within_cooldown() {
        let limiter = make_rate_limiter();
        assert!(should_sample(&limiter));
        assert!(
            !should_sample(&limiter),
            "immediate second call should return false"
        );
    }

    #[test]
    fn rate_limiter_returns_true_after_cooldown() {
        let limiter = make_rate_limiter();
        assert!(should_sample(&limiter));

        // Manually set the last sample to >1s ago
        {
            let mut guard = limiter.lock().unwrap();
            *guard = Some(Instant::now() - Duration::from_secs(2));
        }

        assert!(
            should_sample(&limiter),
            "should return true after cooldown has elapsed"
        );
    }
}
