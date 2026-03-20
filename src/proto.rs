// Re-export OpenTelemetry proto types.
//
// We use the opentelemetry-proto crate for pre-generated OTLP types
// rather than vendoring and compiling the proto files ourselves.
// The gRPC service implementations in the receiver module will use
// tonic to implement the OTLP collector services.

pub use opentelemetry_proto::tonic::{
    collector::{logs::v1 as logs_service, metrics::v1 as metrics_service, trace::v1 as trace_service},
    common::v1 as common,
    logs::v1 as logs,
    metrics::v1 as metrics,
    resource::v1 as resource,
    trace::v1 as trace,
};
