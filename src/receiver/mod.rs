pub mod logs;
pub mod metrics;
pub mod trace;

use crate::proto::resource::Resource;

/// Extract the peer ID from OTLP resource attributes.
///
/// Prefers the explicit `hopr.peer_id` attribute and falls back to the
/// standard `service.instance.id` when the dedicated key is absent.
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

    string_val("hopr.peer_id").or_else(|| string_val("service.instance.id"))
}
