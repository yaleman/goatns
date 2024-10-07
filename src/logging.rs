//! Logging and OTEL related thingies

use init_tracing_opentelemetry::tracing_subscriber_ext;

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
            "{},otel::tracing=debug,otel=debug,h2=error,hyper_util=error,tower=error,tonic=error",
            std::env::var("RUST_LOG")
                .or_else(|_| std::env::var("OTEL_LOG_LEVEL"))
                .unwrap_or_else(|_| "info".to_string())
        ),
    );
    EnvFilter::from_default_env()
}

#[allow(dead_code)]
pub(crate) fn init_otel_subscribers() -> Result<(), String> {
    //setup a temporary subscriber to log output during setup
    let subscriber = tracing_subscriber::registry()
        .with(build_loglevel_filter_layer())
        .with(tracing_subscriber_ext::build_logger_text());
    let _guard = tracing::subscriber::set_default(subscriber);

    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber_ext::build_otel_layer().map_err(|err| err.to_string())?)
        .with(build_loglevel_filter_layer())
        .with(tracing_subscriber_ext::build_logger_text());
    tracing::subscriber::set_global_default(subscriber).map_err(|err| err.to_string())?;
    Ok(())
}
