// Re-export OpenTelemetry proto types.
//
// We use the opentelemetry-proto crate for pre-generated OTLP types
// rather than vendoring and compiling the proto files ourselves.
// The gRPC service implementations in the receiver module will use
// tonic to implement the OTLP collector services.

pub use opentelemetry_proto::tonic::collector::logs::v1 as logs_service;
pub use opentelemetry_proto::tonic::collector::metrics::v1 as metrics_service;
pub use opentelemetry_proto::tonic::collector::trace::v1 as trace_service;
pub use opentelemetry_proto::tonic::common::v1 as common;
pub use opentelemetry_proto::tonic::logs::v1 as logs;
pub use opentelemetry_proto::tonic::metrics::v1 as metrics;
pub use opentelemetry_proto::tonic::resource::v1 as resource;
pub use opentelemetry_proto::tonic::trace::v1 as trace;
