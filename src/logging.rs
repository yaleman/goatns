//! Logging and OTEL related thingies

use std::{env, time::Duration};

use init_tracing_opentelemetry::tracing_subscriber_ext;
use opentelemetry::{global, trace::TracerProvider as _, KeyValue};

use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::{
    trace::{Sampler, TracerProvider},
    Resource,
};

use opentelemetry_semantic_conventions::{
    attribute::{SERVICE_NAME, SERVICE_VERSION},
    SCHEMA_URL,
};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::EnvFilter;

#[allow(dead_code)]
pub(crate) fn build_loglevel_filter_layer() -> EnvFilter {
    // filter what is output on log (fmt)
    // std::env::set_var("RUST_LOG", "warn,otel::tracing=info,otel=debug");
    std::env::set_var(
        "RUST_LOG",
        format!(
            // `otel::tracing` should be a level info to emit opentelemetry trace & span
            // `otel::setup` set to debug to log detected resources, configuration read and inferred
            "{},otel::tracing=debug,otel=debug,h2=error,hyper=warn,hyper_util=warn,tower=error,tonic=error",
            std::env::var("RUST_LOG")
                .or_else(|_| std::env::var("OTEL_LOG_LEVEL"))
                .unwrap_or_else(|_| "info".to_string())
        ),
    );
    EnvFilter::from_default_env()
}

#[allow(dead_code)]
pub(crate) fn init_otel_subscribers(otel_endpoint: Option<String>) -> Result<(), String> {
    //setup a temporary subscriber to log output during setup
    let subscriber = tracing_subscriber::registry()
        .with(build_loglevel_filter_layer())
        .with(tracing_subscriber_ext::build_logger_text());
    let _guard = tracing::subscriber::set_default(subscriber);

    let resource = Resource::from_schema_url(
        [
            KeyValue::new(SERVICE_NAME, "goatns"),
            KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
        ],
        SCHEMA_URL,
    );

    let otlp_exporter = opentelemetry_otlp::SpanExporter::builder().with_tonic();

    let otlp_exporter = match otel_endpoint {
        Some(endpoint) => otlp_exporter.with_endpoint(endpoint),
        None => otlp_exporter.with_endpoint(
            env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:4317".to_string()),
        ),
    };
    let otlp_exporter = otlp_exporter
        .with_protocol(Protocol::HttpBinary)
        .with_timeout(Duration::from_secs(5))
        .build()
        .map_err(|err| err.to_string())?;

    let provider = TracerProvider::builder()
        .with_batch_exporter(otlp_exporter, opentelemetry_sdk::runtime::Tokio)
        // we want *everything!*
        .with_sampler(Sampler::AlwaysOn)
        .with_max_events_per_span(1000)
        .with_max_attributes_per_span(1000)
        .with_resource(resource)
        .build();

    global::set_tracer_provider(provider.clone());
    provider.tracer("tracing-otel-subscriber");

    let subscriber = tracing_subscriber::registry()
        .with(OpenTelemetryLayer::new(
            provider.tracer("tracing-otel-subscriber"),
        ))
        .with(build_loglevel_filter_layer())
        .with(tracing_subscriber_ext::build_logger_text());
    tracing::subscriber::set_global_default(subscriber).map_err(|err| err.to_string())?;
    Ok(())
}
