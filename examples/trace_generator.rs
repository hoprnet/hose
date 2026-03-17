//! Synthetic OTLP trace generator for testing HOSE locally.
//!
//! Sends one batch per second to the gRPC OTLP endpoint with randomized
//! peer IDs, span names, and optional HOPR session attributes.
//!
//! Usage:
//!   cargo run --example trace_generator
//!   cargo run --example trace_generator -- http://localhost:4317

use hose::proto::common::{AnyValue, KeyValue, any_value};
use hose::proto::resource::Resource;
use hose::proto::trace::{ResourceSpans, ScopeSpans, Span};
use hose::proto::trace_service::ExportTraceServiceRequest;
use hose::proto::trace_service::trace_service_client::TraceServiceClient;

use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};

const PEER_IDS: &[&str] = &[
    "16Uiu2HAmSynth001",
    "16Uiu2HAmSynth002",
    "16Uiu2HAmSynth003",
    "16Uiu2HAmSynth004",
    "16Uiu2HAmSynth005",
    "16Uiu2HAmSynth006",
    "16Uiu2HAmSynth007",
    "16Uiu2HAmSynth008",
    "16Uiu2HAmSynth009",
    "16Uiu2HAmSynth010",
];

const SPAN_NAMES: &[&str] = &[
    "hopr.relay.forward",
    "hopr.session.open",
    "hopr.packet.send",
    "hopr.packet.receive",
    "hopr.channel.open",
    "hopr.channel.close",
    "hopr.ticket.redeem",
    "hopr.heartbeat.ping",
    "hopr.strategy.evaluate",
    "hopr.mixer.process",
];

const PROTOCOLS: &[&str] = &["tcp", "udp", "quic"];
const ROLES: &[&str] = &["entry", "relay", "exit"];

fn string_kv(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::StringValue(value.to_string())),
        }),
    }
}

fn int_kv(key: &str, value: i64) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(any_value::Value::IntValue(value)),
        }),
    }
}

fn random_bytes(rng: &mut impl Rng, len: usize) -> Vec<u8> {
    (0..len).map(|_| rng.r#gen()).collect()
}

fn now_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn generate_batch(rng: &mut impl Rng) -> ExportTraceServiceRequest {
    let num_resources = rng.r#gen_range(1..=5);
    let mut resource_spans = Vec::with_capacity(num_resources);

    for _ in 0..num_resources {
        let peer_id = PEER_IDS[rng.r#gen_range(0..PEER_IDS.len())];
        let num_spans = rng.r#gen_range(1..=3);
        let mut spans = Vec::with_capacity(num_spans);

        for _ in 0..num_spans {
            let span_name = SPAN_NAMES[rng.r#gen_range(0..SPAN_NAMES.len())];
            let start = now_nanos() - rng.r#gen_range(1_000_000..100_000_000);
            let end = now_nanos();

            // ~30% of spans carry session attributes
            let mut attributes = Vec::new();
            if rng.r#gen_ratio(3, 10) {
                let session_id = format!("sess-{:08x}", rng.r#gen::<u32>());
                let protocol = PROTOCOLS[rng.r#gen_range(0..PROTOCOLS.len())];
                let hops = rng.r#gen_range(1..=4) as i64;
                let role = ROLES[rng.r#gen_range(0..ROLES.len())];

                attributes.push(string_kv("hopr.session.id", &session_id));
                attributes.push(string_kv("hopr.session.protocol", protocol));
                attributes.push(int_kv("hopr.session.hops", hops));
                attributes.push(string_kv("hopr.session.role", role));
            }

            spans.push(Span {
                trace_id: random_bytes(rng, 16),
                span_id: random_bytes(rng, 8),
                parent_span_id: vec![],
                name: span_name.to_string(),
                kind: rng.r#gen_range(0..=5) as i32,
                start_time_unix_nano: start,
                end_time_unix_nano: end,
                attributes,
                dropped_attributes_count: 0,
                events: vec![],
                dropped_events_count: 0,
                links: vec![],
                dropped_links_count: 0,
                status: None,
                flags: 0,
                trace_state: String::new(),
            });
        }

        resource_spans.push(ResourceSpans {
            resource: Some(Resource {
                attributes: vec![string_kv("service.instance.id", peer_id)],
                dropped_attributes_count: 0,
            }),
            scope_spans: vec![ScopeSpans {
                scope: None,
                spans,
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        });
    }

    ExportTraceServiceRequest { resource_spans }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("HOSE_GRPC_TARGET").ok())
        .unwrap_or_else(|| "http://localhost:4317".to_string());

    println!("Trace generator → {target}");
    println!("Press Ctrl+C to stop\n");

    let mut client = TraceServiceClient::connect(target).await?;
    let mut rng = rand::thread_rng();
    let mut batch_num: u64 = 0;

    loop {
        batch_num += 1;
        let request = generate_batch(&mut rng);
        let span_count: usize = request
            .resource_spans
            .iter()
            .flat_map(|rs| &rs.scope_spans)
            .map(|ss| ss.spans.len())
            .sum();

        match client.export(tonic::Request::new(request)).await {
            Ok(_) => {
                println!(
                    "batch #{batch_num}: sent {span_count} spans across {} resources",
                    batch_num
                );
            }
            Err(e) => {
                eprintln!("batch #{batch_num}: error — {e}");
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
