use tonic::{Request, Response, Status};

use crate::peer_router::PeerRouter;
use crate::peer_tracker::PeerTracker;
use crate::proto::metrics_service::metrics_service_server::MetricsService;
use crate::proto::metrics_service::{ExportMetricsServiceRequest, ExportMetricsServiceResponse};
use crate::server::Event;
use crate::types::RoutingDecision;
use crate::write_buffer::{RecordType, WriteBufferSender, WriteRecord};
use tokio::sync::broadcast;

#[derive(Debug, Clone)]
pub struct MetricsReceiver {
    pub peer_tracker: PeerTracker,
    pub peer_router: PeerRouter,
    pub write_buffer: WriteBufferSender,
    pub event_tx: broadcast::Sender<Event>,
}

#[tonic::async_trait]
impl MetricsService for MetricsReceiver {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        let req = request.into_inner();

        for resource_metrics in &req.resource_metrics {
            let peer_id = resource_metrics
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
                tracing::warn!("metrics received with empty peer_id, skipping resource_metrics");
                continue;
            }

            tracing::debug!(peer_id = %peer_id, "metrics export received");

            self.peer_tracker.record_seen(&peer_id).await;
            let _ = self
                .event_tx
                .send(Event::PeerSeen {
                    peer_id: peer_id.clone(),
                });

            match self.peer_router.route(&peer_id).await {
                RoutingDecision::Discard => {
                    tracing::debug!(peer_id = %peer_id, "metrics discarded by routing");
                }
                RoutingDecision::Retain { session_ids } => {
                    tracing::debug!(
                        peer_id = %peer_id,
                        session_count = session_ids.len(),
                        "metrics retained for debug sessions"
                    );
                    for scope_metrics in &resource_metrics.scope_metrics {
                        for metric in &scope_metrics.metrics {
                            let payload = serde_json::json!({
                                "name": metric.name,
                                "description": metric.description,
                                "unit": metric.unit,
                            });

                            for session_id in &session_ids {
                                self.write_buffer.try_send(WriteRecord {
                                    debug_session_id: *session_id,
                                    peer_id: peer_id.clone(),
                                    record_type: RecordType::Metric,
                                    payload: payload.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(Response::new(ExportMetricsServiceResponse {
            partial_success: None,
        }))
    }
}
