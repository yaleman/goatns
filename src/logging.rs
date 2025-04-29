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
use tracing::Subscriber;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::{layer::SubscriberExt, registry::LookupSpan};

#[allow(dead_code)]
pub(crate) fn build_loglevel_filter_layer(log_level: &str) -> EnvFilter {
    // filter what is output on log (fmt)
    // std::env::set_var("RUST_LOG", "warn,otel::tracing=info,otel=debug");
    std::env::set_var(
        "RUST_LOG",
        format!(
            // `otel::tracing` should be a level info to emit opentelemetry trace & span
            // `otel::setup` set to debug to log detected resources, configuration read and inferred
            "{},otel::tracing=debug,otel=debug,h2=error,hyper=warn,hyper_util=warn,tower=error,tonic=error",
            log_level
        ),
    );
    EnvFilter::from_default_env()
}

/// Tweaked version of what ships with tracing_subscriber
fn build_logger_text<S>() -> Box<dyn tracing_subscriber::Layer<S> + Send + Sync + 'static>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    use tracing_subscriber::fmt::format::FmtSpan;
    if cfg!(debug_assertions) {
        Box::new(
            tracing_subscriber::fmt::layer()
                // .pretty()
                .compact()
                .with_line_number(false)
                .with_thread_names(false)
                .with_span_events(FmtSpan::CLOSE)
                // .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
                .with_timer(tracing_subscriber::fmt::time::uptime()),
        )
    } else {
        Box::new(
            tracing_subscriber::fmt::layer()
                .json()
                //.with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
                .with_timer(tracing_subscriber::fmt::time::uptime()),
        )
    }
}

#[allow(dead_code)]
pub(crate) fn init_otel_subscribers(
    otel_endpoint: Option<String>,
    config_log_level: &str,
    cli_debug: bool,
) -> Result<(), String> {
    let log_level = match (cli_debug, config_log_level) {
        (true, _) => "debug",
        (_, log_level) => log_level,
    };

    //setup a temporary subscriber to log output during setup
    let subscriber = tracing_subscriber::registry()
        .with(build_loglevel_filter_layer(log_level))
        .with(tracing_subscriber_ext::build_logger_text());
    let _guard = tracing::subscriber::set_default(subscriber);

    let resource = Resource::from_schema_url(
        [
            KeyValue::new(SERVICE_NAME, "goatns"),
            KeyValue::new(SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
        ],
        SCHEMA_URL,
    );

    let endpoint = match (otel_endpoint, env::var("OTEL_EXPORTER_OTLP_ENDPOINT")) {
        (Some(endpoint), _) => Some(endpoint),
        (_, Ok(endpoint)) => Some(endpoint),
        _ => None,
    };

    let subscriber = tracing_subscriber::registry()
        .with(build_loglevel_filter_layer(log_level))
        .with(build_logger_text());

    match endpoint {
        Some(endpoint) => {
            let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint)
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
            provider.tracer("goatns");
            let subscriber = subscriber.with(OpenTelemetryLayer::new(provider.tracer("goatns")));
            tracing::subscriber::set_global_default(subscriber).map_err(|err| err.to_string())?
        }
        None => {
            tracing::subscriber::set_global_default(subscriber).map_err(|err| err.to_string())?
        }
    };
    Ok(())
}
