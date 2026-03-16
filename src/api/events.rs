use axum::extract::State;
use axum::response::sse::{Event as SseEvent, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::server::{AppState, Event};

/// GET /api/events - SSE stream of live events.
pub async fn event_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).ok()?;
                let event_type = match &event {
                    Event::PeerSeen { .. } => "peer_seen",
                    Event::SessionObserved { .. } => "session_observed",
                    Event::DebugSessionUpdated { .. } => "debug_session_updated",
                    Event::TelemetryRate { .. } => "telemetry_rate",
                };
                Some(Ok(SseEvent::default().event(event_type).data(json)))
            }
            Err(_) => None, // Lagging receiver — drop event
        }
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)),
    )
}
