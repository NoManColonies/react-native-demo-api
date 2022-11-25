use app::{
    config::database::init_redis,
    middleware::{
        config::layer::ConfigSessionLayer, cookie::layer::CookieSessionLayer,
        sentry::layer::SentrySessionLayer, tracing::layer::TracingLayer,
    },
    service::test_message::{
        test_message::test_message_service_server::TestMessageServiceServer, TestMessageGreeter,
    },
};
use sentry_tracing::EventFilter;
use std::{env::var, sync::Arc};
use tokio::{signal, sync::Notify, time::Duration};
use tonic::transport::Server;
use tracing::{info, info_span, log::debug};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_futures::Instrument;
use tracing_log::LogTracer;
use tracing_subscriber::{
    layer::SubscriberExt,
    {EnvFilter, Registry},
};

#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

use crate::app::config::task::spawn_with_name;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(60);

lazy_static::lazy_static! {
    static ref APP_NAME: &'static str = env!("CARGO_PKG_NAME");
    static ref APP_VERSION: &'static str = env!("CARGO_PKG_VERSION");
    static ref APP_URL: String = var("APP_URL").expect("expect an APP_URL to be set. app url define internal IP address for application to bind and is usually 0.0.0.0");
    static ref APP_PORT: String = var("APP_PORT").expect("expect an APP_PORT to be set. app port define virtual port for app to bind to");
    static ref AMQP_ADDRESS: String = var("AMQP_ADDRESS").expect("expect an AMQP_ADDRESS to be setup");
    static ref AMQP_ADMIN_USERNAME: String = var("AMQP_ADMIN_USERNAME").expect("expect an AMQP admin username to be set. admin username is used to authenticate into RabbitMQ to perform administration task");
    static ref AMQP_ADMIN_PASSWORD: String = var("AMQP_ADMIN_PASSWORD").expect("expect an AMQP admin password to be set. admin password is used to authenticate into RabbitMQ to perform administration task");
    static ref REDIS_URL: String = var("REDIS_URL").expect("expect a valid redis server url. redis url define address for redis client to connect to");
    static ref SENTRY_URL: String = var("SENTRY_URL").expect("expect SENTRY_URL to be set");
}

mod app;

#[tokio::main]
async fn main() {
    // install `log -> tracing` converter
    LogTracer::init().expect("expect log tracer to complete the setup process");
    // setup .env file parser
    dotenv::dotenv().expect("expect a .env file and valid syntax");

    let name = &*APP_NAME;
    let version = &*APP_VERSION;
    // setup sentry DSN from env if `sentry-io` feature is enabled
    let sentry_url = &*SENTRY_URL.to_owned();
    // setup sentry client hub and bind to the main thread
    let sentry_guard = sentry::init((
        sentry_url, // <- the DSN key
        sentry::ClientOptions {
            release: sentry::release_name!(), // <- the application/service name that show up in sentry dashboard
            integrations: vec![
                Arc::new(sentry_backtrace::AttachStacktraceIntegration), // <- attach stacktrace if our service crashed unexpectedly
                Arc::new(sentry_backtrace::ProcessStacktraceIntegration), // <- process stacktrace in case our service crashed
            ],
            session_mode: sentry::SessionMode::Request, // <- setup a session mode to be `per request`
            auto_session_tracking: true,                // <- made session tracking automatic
            ..Default::default()                        // <- leave other options to default
        },
    ));

    #[cfg(not(feature = "stdout"))]
    let file_appender =
        tracing_appender::rolling::hourly("/tmp/react-native-test-api/log", "hourly.log");
    #[cfg(not(feature = "stdout"))]
    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    #[cfg(feature = "stdout")]
    let (non_blocking_writer, _non_blocking_writer_guard) =
        tracing_appender::non_blocking(std::io::stdout());

    let bunyan_formatting_layer =
        BunyanFormattingLayer::new(format!("{}-{}", name, version), non_blocking_writer);
    let sentry_layer = sentry_tracing::layer().event_filter(|md| match *md.level() {
        tracing::Level::ERROR | tracing::Level::WARN => EventFilter::Breadcrumb,
        _ => EventFilter::Ignore,
    });

    let filter_layer = EnvFilter::new("INFO");
    let subscriber = Registry::default()
        .with(filter_layer)
        .with(JsonStorageLayer)
        .with(bunyan_formatting_layer)
        .with(sentry_layer);
    tracing::subscriber::set_global_default(subscriber)
        .expect("expect a tracing subscriber to complete the setup process");
    // initialize redis database connection manager
    let redis_pool = init_redis().await;
    // thread safe application shutdown signal notifier
    let shutdown_signal_notifier = Arc::new(Notify::new());

    // parse socket address from env
    let addr = format!("{}:{}", *APP_URL, *APP_PORT)
        .parse()
        .expect("expect a successfully parsed url");

    let test_messag_greeter = TestMessageGreeter {
        shutdown_signal_notifier: Arc::clone(&shutdown_signal_notifier),
        redis_pool: redis_pool.clone(),
    };

    // graceful shutdown handler
    spawn_with_name(
        {
            let root_span = info_span!("shutdown interceptor");
            let shutdown_signal_notifier = Arc::clone(&shutdown_signal_notifier);

            async move {
                debug!("waiting for ctrl-c signal...");
                // wait for ctrl-c signal
                signal::ctrl_c()
                    .await
                    .expect("expect ctrl-c signal to be successfully received");
                debug!("received ctrl-c signal");

                // notify all client about application shutting down
                shutdown_signal_notifier.notify_waiters();
            }
            .instrument(root_span)
        },
        "shutdown interceptor",
    );
    // setup service layer a.k.a. middleware service
    let layers = tower::ServiceBuilder::new().layer(TracingLayer);

    let layers = layers.layer(SentrySessionLayer::builder().emit_header(true).finish());

    let layers = layers
        .layer(ConfigSessionLayer(redis_pool.clone()))
        .layer(CookieSessionLayer);

    let layers = layers.into_inner();

    // setup google `grpc.health.v1.Health` compliant health reporter service
    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<TestMessageServiceServer<TestMessageGreeter>>()
        .await;

    // configure and build tonic gRPC server
    Server::builder()
        .layer(layers)
        // .accept_http1(true)
        .tcp_keepalive(Some(KEEP_ALIVE_TIMEOUT))
        .http2_keepalive_interval(Some(KEEP_ALIVE_TIMEOUT / 3))
        .http2_keepalive_timeout(Some(KEEP_ALIVE_TIMEOUT))
        .add_service(health_service)
        .add_service(TestMessageServiceServer::new(test_messag_greeter))
        // .add_service(amqp_subscription_http11)
        // bind shutdown signal for graceful shutdown
        .serve_with_shutdown(addr, shutdown_signal_notifier.notified())
        .await
        .expect("expect a server to be successfully served");

    // wait 10 seconds for all client to acknowledge the shutdown signal and sentry client to flush all events
    // or wait for ctrl-c signal to force application shutdown
    // FIXME! Caveats: stdout pipe seem to get disconnected when ctrl-c was received so these trace
    // will not show up in the console.
    // TODO! test whether they will show up in the log file or not
    info!("performing graceful shutdown which may take up to 10 seconds... or ctrl-c to force shutdown");
    tokio::select! {
        _ = tokio::task::spawn_blocking(move || sentry_guard.flush(Some(Duration::from_secs(10)))) => {
            debug!("exiting...");
        }
        _ = signal::ctrl_c() => {
            debug!("force shutting down...");
        }
    }
}
