pub mod logs;
pub mod metrics;
pub mod trace;

use crate::proto::resource::Resource;

/// Extract the peer ID from OTLP resource attributes.
///
/// Prefers the explicit `hopr.peer_id` attribute and falls back to the
/// standard `service.instance.id` when the dedicated key is absent.
///
/// The peer ID label can be optionally set via the `HOSE_PEER_ID_LABEL` environment variable.
pub fn extract_peer_id(resource: Option<&Resource>) -> Option<String> {
    let attrs = &resource?.attributes;

    let string_val = |key: &str| {
        attrs
            .iter()
            .find(|a| a.key == key)
            .and_then(|a| match a.value.as_ref()?.value.as_ref()? {
                crate::proto::common::any_value::Value::StringValue(s) if !s.is_empty() => Some(s.clone()),
                _ => None,
            })
    };

    let expected_peer_id_label = std::env::var("HOSE_PEER_ID_LABEL").ok().unwrap_or("hopr.peer_id".into());

    string_val(&expected_peer_id_label).or_else(|| string_val("service.instance.id"))
}
