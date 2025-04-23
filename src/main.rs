extern crate core;

use bytes::Bytes;
use clap::{Parser, ValueEnum};
use dotenvy::Substitutor;
use http_body_util::Full;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use lambda_extension::{LambdaTelemetryRecord, NextEvent};
use rotel::bounded_channel::bounded;
use rotel::init::agent::Agent;
use rotel::init::args;
use rotel::init::args::{AgentRun, Exporter};
use rotel::init::misc::bind_endpoints;
use rotel::init::wait;
use rotel::listener::Listener;
use rotel::topology::flush_control::{FlushBroadcast, FlushSender};
use rotel_extension::aws_api::config::AwsConfig;
use rotel_extension::env::{EnvArnParser, resolve_secrets};
use rotel_extension::lambda;
use rotel_extension::lambda::telemetry_api::TelemetryAPI;
use rotel_extension::lifecycle::flush_control::{
    Clock, DEFAULT_FLUSH_INTERVAL_MILLIS, FlushControl, FlushMode,
};
use rustls::crypto::CryptoProvider;
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::ops::Add;
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::task::JoinSet;
use tokio::time::{Instant, Interval, timeout};
use tokio::{pin, select};
use tokio_util::sync::CancellationToken;
use tower_http::BoxError;
use tracing::{debug, error, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

pub const SENDING_QUEUE_SIZE: usize = 10;

//
// todo: these constants should be configurable

pub const LOGS_QUEUE_SIZE: usize = 50;

pub const FLUSH_PIPELINE_TIMEOUT_MILLIS: u64 = 500;
pub const FLUSH_EXPORTERS_TIMEOUT_MILLIS: u64 = 3_000;

#[derive(Debug, Parser)]
#[command(name = "rotel-lambda-extension")]
#[command(bin_name = "rotel-lambda-extension")]
struct Arguments {
    #[arg(long, global = true, env = "ROTEL_LOG_LEVEL", default_value = "info")]
    /// Log configuration
    log_level: String,

    #[arg(long, env = "ROTEL_TELEMETRY_ENDPOINT", default_value = "0.0.0.0:8990", value_parser = args::parse_endpoint)]
    telemetry_endpoint: SocketAddr,

    #[arg(
        value_enum,
        long,
        global = true,
        env = "ROTEL_LOG_FORMAT",
        default_value = "text"
    )]
    /// Log format
    log_format: LogFormatArg,

    #[arg(long, global = true, env = "ROTEL_ENVIRONMENT", default_value = "dev")]
    /// Environment
    environment: String,

    // This is ignored in these options, but we keep it here to avoid an error on unknown
    // options
    #[arg(long)]
    env_file: Option<String>,

    #[command(flatten)]
    agent_args: Box<AgentRun>,
}

// Minimal option to allow us to parse out the env from a file
#[derive(Debug, Parser)]
#[clap(ignore_errors = true)]
struct EnvFileArguments {
    #[arg(long, env = "ROTEL_ENV_FILE")]
    env_file: Option<String>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, ValueEnum)]
pub enum LogFormatArg {
    Text,
    Json,
}

fn main() -> ExitCode {
    let start_time = Instant::now();

    let env_opt = EnvFileArguments::parse();
    if let Some(env_file) = env_opt.env_file {
        if let Err(e) = load_env_file(&env_file) {
            eprintln!("Can not load envfile: {}", e);
            return ExitCode::FAILURE;
        }
    }

    let opt = Arguments::parse();

    let _logger = setup_logging(&opt.log_level);
    let agent = opt.agent_args;
    let mut port_map = match bind_endpoints(&[
        agent.otlp_grpc_endpoint,
        agent.otlp_http_endpoint,
        opt.telemetry_endpoint,
    ]) {
        Ok(ports) => ports,
        Err(e) => {
            eprintln!("ERROR: {}", e);

            return ExitCode::from(1);
        }
    };

    // Remove this, the rest are passed to the agent
    let telemetry_listener = port_map.remove(&opt.telemetry_endpoint).unwrap();

    match run_extension(
        start_time,
        agent,
        port_map,
        telemetry_listener,
        &opt.environment,
    ) {
        Ok(_) => {}
        Err(e) => {
            error!(error = ?e, "Failed to run agent.");
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

fn load_env_file(env_file: &String) -> Result<(), BoxError> {
    let subs = load_env_file_updates(env_file)?;

    for (key, val) in subs {
        unsafe { env::set_var(key, val) }
    }

    Ok(())
}

fn load_env_file_updates(env_file: &String) -> Result<Vec<(String, String)>, BoxError> {
    let mut updates = Vec::new();
    for item in dotenvy::from_filename_iter_custom_sub(env_file, ArnEnvSubstitutor {})
        .map_err(|e| format!("failed to open env file {}: {}", env_file, e))?
    {
        let (key, val) = item.map_err(|e| format!("unable to parse line: {}", e))?;
        updates.push((key, val))
    }

    Ok(updates)
}

#[derive(Clone)]
struct ArnEnvSubstitutor;
impl Substitutor for ArnEnvSubstitutor {
    fn substitute(&self, val: &str) -> Option<String> {
        // We'll expand this later
        if val.starts_with("arn:") {
            // need to escape curly braces
            return Some(format!("${{{}}}", val));
        }

        // Fall back to normal env expansion
        match std::env::var(val) {
            Ok(s) => Some(s),
            Err(_) => None,
        }
    }
}

#[tokio::main]
async fn run_extension(
    start_time: Instant,
    mut agent_args: Box<AgentRun>,
    port_map: HashMap<SocketAddr, Listener>,
    telemetry_listener: Listener,
    env: &String,
) -> Result<(), BoxError> {
    let mut tapi_join_set = JoinSet::new();
    let mut agent_join_set = JoinSet::new();

    let client = build_hyper_client();

    let (bus_tx, mut bus_rx) = bounded(10);
    let (logs_tx, logs_rx) = bounded(LOGS_QUEUE_SIZE);

    let aws_config = AwsConfig::from_env();

    //
    // Resolve secrets
    //
    let es = EnvArnParser::new();
    let mut secure_arns = es.extract_arns_from_env();
    if !secure_arns.is_empty() {
        if CryptoProvider::get_default().is_none() {
            rustls::crypto::ring::default_provider()
                .install_default()
                .unwrap();
        }

        resolve_secrets(&aws_config, &mut secure_arns).await?;
        es.update_env_arn_secrets(secure_arns);

        // We must reparse arguments now that the environment has been updated
        agent_args = Arguments::parse().agent_args;
    }

    let r = match lambda::api::register(client.clone()).await {
        Ok(r) => r,
        Err(e) => return Err(format!("Failed to register extension: {}", e).into()),
    };

    let (mut flush_pipeline_tx, flush_pipeline_sub) = FlushBroadcast::new().into_parts();
    let (mut flush_exporters_tx, flush_exporters_sub) = FlushBroadcast::new().into_parts();

    let agent_cancel = CancellationToken::new();
    {
        // We control flushing manually, so set this to zero to disable the batch timer
        agent_args.otlp_exporter.otlp_exporter_batch_timeout = "0s".parse().unwrap();

        if agent_args.exporter == Exporter::Otlp {
            if agent_args.otlp_exporter.otlp_exporter_endpoint.is_none()
                && agent_args
                    .otlp_exporter
                    .otlp_exporter_traces_endpoint
                    .is_none()
                && agent_args
                    .otlp_exporter
                    .otlp_exporter_metrics_endpoint
                    .is_none()
                && agent_args
                    .otlp_exporter
                    .otlp_exporter_logs_endpoint
                    .is_none()
            {
                // todo: We should be able to startup with no config and not fail, identify best
                // default mode.
                info!("Automatically selecting blackhole exporter due to missing endpoint configs");
                agent_args.exporter = Exporter::Blackhole;
            }
        }

        let agent = Agent::new(agent_args, port_map, SENDING_QUEUE_SIZE, env.clone())
            .with_logs_rx(logs_rx)
            .with_pipeline_flush(flush_pipeline_sub)
            .with_exporters_flush(flush_exporters_sub);
        let token = agent_cancel.clone();
        let agent_fut = async move { agent.run(token).await };

        agent_join_set.spawn(agent_fut);
    };

    if let Err(e) = lambda::api::telemetry_subscribe(
        client.clone(),
        &r.extension_id,
        &telemetry_listener.bound_address()?,
    )
    .await
    {
        return Err(format!("Failed to subscribe to telemetry: {}", e).into());
    }

    let telemetry = TelemetryAPI::new(telemetry_listener, logs_tx);
    let telemetry_cancel = CancellationToken::new();
    {
        let token = telemetry_cancel.clone();
        let telemetry_fut = async move { telemetry.run(bus_tx.clone(), token).await };
        tapi_join_set.spawn(telemetry_fut)
    };

    // Set up our global flush interval, will be reset when we flush periodically
    let mut default_flush_interval =
        tokio::time::interval(Duration::from_millis(DEFAULT_FLUSH_INTERVAL_MILLIS));
    default_flush_interval.tick().await; // first tick is instant

    info!(
        "Rotel Lambda Extension started in {}ms",
        start_time.elapsed().as_millis()
    );

    // Must perform next_request to get the first INVOKE call
    let next_evt = match lambda::api::next_request(client.clone(), &r.extension_id).await {
        Ok(evt) => evt,
        Err(e) => return Err(format!("Failed to read next event: {}", e).into()),
    };
    handle_next_response(next_evt);

    let mut flush_control = FlushControl::new(SystemClock {});

    'outer: loop {
        let mode = flush_control.pick();
        let should_shutdown;

        match mode {
            FlushMode::AfterCall => {
                'inner: loop {
                    //
                    // We must flush after every invocation
                    //
                    select! {
                        msg = bus_rx.next() => {
                            if let Some(evt) = msg {
                                if let LambdaTelemetryRecord::PlatformRuntimeDone {..} = evt.record {
                                    break 'inner;
                                }
                            }
                        },
                        e = wait::wait_for_any_task(&mut tapi_join_set) => {
                            match e {
                                Ok(()) => warn!("Unexpected early exit of TelemetryAPI."),
                                Err(e) => return Err(e),
                            }
                        },
                        e = wait::wait_for_any_task(&mut agent_join_set) => {
                            match e {
                                Ok(()) => warn!("Unexpected early exit of extension."),
                                Err(e) => return Err(e),
                            }
                        },
                        _ = default_flush_interval.tick() => {
                            force_flush(&mut flush_pipeline_tx, &mut flush_exporters_tx, &mut default_flush_interval).await;
                        }
                    }
                }

                //
                // Force a flush
                //
                force_flush(
                    &mut flush_pipeline_tx,
                    &mut flush_exporters_tx,
                    &mut default_flush_interval,
                )
                .await;

                debug!("Received a platform runtime done message, invoking next request");
                let next_evt =
                    match lambda::api::next_request(client.clone(), &r.extension_id).await {
                        Ok(evt) => evt,
                        Err(e) => return Err(format!("Failed to read next event: {}", e).into()),
                    };

                should_shutdown = handle_next_response(next_evt);
            }
            FlushMode::Periodic(mut control) => {
                // Check if we need to force a flush, this should happen concurrently with the
                // function invocation.
                if control.should_flush() {
                    force_flush(
                        &mut flush_pipeline_tx,
                        &mut flush_exporters_tx,
                        &mut default_flush_interval,
                    )
                    .await;
                }

                let next_event_fut = lambda::api::next_request(client.clone(), &r.extension_id);
                pin!(next_event_fut);

                'periodic_inner: loop {
                    select! {
                        biased;

                        next_resp = &mut next_event_fut => {
                            // Reset the default flush timer on invocation, since we are checking whether to flush
                            // at the top of the invocation anyways
                            default_flush_interval.reset();

                            match next_resp {
                                Err(e) => return Err(format!("Failed to read next event: {}", e).into()),
                                Ok(next_evt) => {
                                    should_shutdown = handle_next_response(next_evt);

                                    break 'periodic_inner;
                                }

                            }
                        }

                        _ = bus_rx.next() => {
                            // Mostly ignore these here for now
                        },

                        e = wait::wait_for_any_task(&mut tapi_join_set) => {
                            match e {
                                Ok(()) => warn!("Unexpected early exit of TelemetryAPI."),
                                Err(e) => return Err(e),
                            }
                        },

                        e = wait::wait_for_any_task(&mut agent_join_set) => {
                            match e {
                                Ok(()) => warn!("Unexpected early exit of extension."),
                                Err(e) => return Err(e),
                            }
                        },

                        _ = default_flush_interval.tick() => {
                            force_flush(&mut flush_pipeline_tx, &mut flush_exporters_tx, &mut default_flush_interval).await;
                        }
                    }
                }
            }
        }

        if should_shutdown {
            info!("Shutdown received, exiting");
            break 'outer;
        }
    }

    // We have two seconds to completely shutdown
    let final_stop = Instant::now().add(Duration::from_secs(2));

    // Wait up to 500ms for the TelemetryAPI to shutdown, this will stop the logs pipeline
    telemetry_cancel.cancel();
    wait::wait_for_tasks_with_timeout(&mut tapi_join_set, Duration::from_millis(500)).await?;

    agent_cancel.cancel();

    // Wait for agent
    wait::wait_for_tasks_with_deadline(&mut agent_join_set, final_stop).await?;

    Ok(())
}

async fn force_flush(
    pipeline_tx: &mut FlushSender,
    exporters_tx: &mut FlushSender,
    default_flush: &mut Interval,
) {
    let start = Instant::now();
    match timeout(
        Duration::from_millis(FLUSH_PIPELINE_TIMEOUT_MILLIS),
        pipeline_tx.broadcast(),
    )
    .await
    {
        Err(_) => {
            warn!("timeout waiting to flush pipelines");
            return;
        }
        Ok(Err(e)) => {
            warn!("failed to flush pipelines: {}", e);
            return;
        }
        _ => {}
    }
    let duration = Instant::now().duration_since(start);
    debug!(?duration, "finished flushing pipeline");

    let start = Instant::now();
    match timeout(
        Duration::from_millis(FLUSH_EXPORTERS_TIMEOUT_MILLIS),
        exporters_tx.broadcast(),
    )
    .await
    {
        Err(_) => {
            warn!("timeout waiting to flush exporters");
            return;
        }
        Ok(Err(e)) => {
            warn!("failed to flush exporters: {}", e);
            return;
        }
        _ => {}
    }
    let duration = Instant::now().duration_since(start);
    debug!(?duration, "finished flushing exporters");
    default_flush.reset();
}

fn handle_next_response(evt: NextEvent) -> bool {
    match evt {
        NextEvent::Invoke(invoke) => debug!("Received an invoke request: {:?}", invoke),
        NextEvent::Shutdown(_) => return true,
    }

    false
}

type LoggerGuard = tracing_appender::non_blocking::WorkerGuard;

// todo: match logging to the recommended lambda extension approach
fn setup_logging(log_level: &str) -> std::io::Result<LoggerGuard> {
    let (non_blocking_writer, guard) = tracing_appender::non_blocking(std::io::stdout());

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking_writer)
        // disable printing of the module
        .with_target(false)
        // cloudwatch will add time
        .without_time()
        // cloudwatch doesn't play nice with escape codes
        .with_ansi(false)
        .compact();

    let subscriber = Registry::default()
        .with(EnvFilter::new(log_level))
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber).unwrap();
    Ok(guard)
}

fn build_hyper_client() -> Client<HttpConnector, Full<Bytes>> {
    hyper_util::client::legacy::Client::builder(TokioExecutor::new())
        // todo: make configurable
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(5)
        .timer(TokioTimer::new())
        .build::<_, Full<Bytes>>(HttpConnector::new())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_env_var_subs() {
        let tf = write_env_file(vec![
            "ROTEL_FOO=nottouched",
            "ROTEL_SUB=\"Bearer ${TOKEN}\"",
            "ROTEL_DOUBLE_SUB=${TEAM}-${TOKEN}",
            "ROTEL_ESCAPED=\"NotMe\\${TEAM}\"",
        ]);

        unsafe { std::env::set_var("TOKEN", "123abc") };
        unsafe { std::env::set_var("TEAM", "frontend") };

        let tf_path = tf.path().to_str().unwrap().to_string();
        let updates = load_env_file_updates(&tf_path).unwrap();

        assert_eq!(
            vec![
                ("ROTEL_FOO".to_string(), "nottouched".to_string()),
                ("ROTEL_SUB".to_string(), "Bearer 123abc".to_string()),
                (
                    "ROTEL_DOUBLE_SUB".to_string(),
                    "frontend-123abc".to_string()
                ),
                ("ROTEL_ESCAPED".to_string(), "NotMe${TEAM}".to_string())
            ],
            updates
        );
    }

    fn write_env_file(envs: Vec<&str>) -> NamedTempFile {
        let mut tf = NamedTempFile::new().unwrap();

        for env in envs {
            tf.write_all(format!("{}\n", env).as_ref()).unwrap();
        }
        tf.flush().unwrap();

        tf
    }
}

#[derive(Clone)]
struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}
