use tonic::{Request, Response, Status};

use crate::peer_router::PeerRouter;
use crate::peer_tracker::PeerTracker;
use crate::proto::logs_service::logs_service_server::LogsService;
use crate::proto::logs_service::{ExportLogsServiceRequest, ExportLogsServiceResponse};
use crate::server::Event;
use crate::types::RoutingDecision;
use crate::write_buffer::{RecordType, WriteBufferSender, WriteRecord};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct LogsReceiver {
    pub peer_tracker: PeerTracker,
    pub peer_router: PeerRouter,
    pub write_buffer: WriteBufferSender,
    pub event_tx: broadcast::Sender<Event>,
}

#[tonic::async_trait]
impl LogsService for LogsReceiver {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let req = request.into_inner();

        for resource_logs in &req.resource_logs {
            let peer_id = resource_logs
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
                tracing::warn!("logs received with empty peer_id, skipping resource_logs");
                continue;
            }

            tracing::debug!(peer_id = %peer_id, "logs export received");

            self.peer_tracker.record_seen(&peer_id).await;
            let _ = self.event_tx.send(Event::PeerSeen {
                peer_id: peer_id.clone(),
            });

            match self.peer_router.route(&peer_id).await {
                RoutingDecision::Discard => {
                    tracing::debug!(peer_id = %peer_id, "logs discarded by routing");
                }
                RoutingDecision::Retain { session_ids } => {
                    tracing::debug!(
                        peer_id = %peer_id,
                        session_count = session_ids.len(),
                        "logs retained for debug sessions"
                    );
                    for scope_logs in &resource_logs.scope_logs {
                        for log_record in &scope_logs.log_records {
                            let body = log_record
                                .body
                                .as_ref()
                                .and_then(|v| {
                                    v.value.as_ref().map(|val| match val {
                                        crate::proto::common::any_value::Value::StringValue(s) => {
                                            s.clone()
                                        }
                                        _ => format!("{:?}", val),
                                    })
                                })
                                .unwrap_or_default();

                            let payload = serde_json::json!({
                                "severityNumber": log_record.severity_number,
                                "severityText": log_record.severity_text,
                                "body": body,
                                "timeUnixNano": log_record.time_unix_nano,
                            });

                            for session_id in &session_ids {
                                self.write_buffer.try_send(WriteRecord {
                                    debug_session_id: *session_id,
                                    peer_id: peer_id.clone(),
                                    record_type: RecordType::Log,
                                    payload: payload.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(Response::new(ExportLogsServiceResponse {
            partial_success: None,
        }))
    }
}
