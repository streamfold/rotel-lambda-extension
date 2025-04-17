use crate::lambda::logs::{Log, parse_logs};
use crate::lambda::otel_string_attr;
use bytes::Bytes;
use http::header::CONTENT_TYPE;
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Body;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use hyper_util::service::TowerToHyperService;
use lambda_extension::{LambdaTelemetry, LambdaTelemetryRecord};
use opentelemetry_proto::tonic::logs::v1::ResourceLogs;
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_semantic_conventions::attribute::FAAS_INVOKED_PROVIDER;
use opentelemetry_semantic_conventions::resource::{
    FAAS_MAX_MEMORY, FAAS_NAME, FAAS_VERSION, SERVICE_NAME,
};
use opentelemetry_semantic_conventions::trace::FAAS_INVOKED_REGION;
use rotel::bounded_channel::BoundedSender;
use rotel::listener::Listener;
use std::fmt::{Debug, Display};
use std::future::Future;
use std::net::SocketAddr;
use std::ops::Add;
use std::pin::Pin;
use std::sync::{LazyLock, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tower::{BoxError, Service, ServiceBuilder};
use tracing::{debug, error, info, warn};

// We don't want to create a logging loop, so limit how often we log
// failures in certain code paths that may loop.
const LOG_LIMIT_INTERVAL_SECS: u64 = 60;
static LOG_LIMIT_LAST_LOG: LazyLock<Mutex<Option<Instant>>> = LazyLock::new(|| Mutex::new(None));

pub struct TelemetryAPI {
    pub listener: Listener,
    pub logs_tx: BoundedSender<ResourceLogs>,
}

impl TelemetryAPI {
    pub fn new(listener: Listener, logs_tx: BoundedSender<ResourceLogs>) -> Self {
        Self { listener, logs_tx }
    }

    pub fn addr(&self) -> SocketAddr {
        self.listener.bound_address().unwrap()
    }

    // todo: abstract this with the server code in the otlp http receiver
    pub async fn run(
        self,
        bus_tx: BoundedSender<LambdaTelemetry>,
        cancellation: CancellationToken,
    ) -> Result<(), BoxError> {
        let resource = resource_from_env();
        let svc =
            ServiceBuilder::new().service(TelemetryService::new(resource, bus_tx, self.logs_tx));
        let svc = TowerToHyperService::new(svc);

        let timer = hyper_util::rt::TokioTimer::new();
        let graceful = hyper_util::server::graceful::GracefulShutdown::new();

        let mut builder = Builder::new(TokioExecutor::new());
        builder
            .http1()
            .header_read_timeout(Some(std::time::Duration::from_secs(3)))
            .timer(timer.clone());
        builder.http2().timer(timer);

        let listener = self.listener.into_async()?;
        loop {
            let stream = tokio::select! {
                r = listener.accept() => {
                    match r {
                        Ok((stream, _)) => stream,
                        Err(e) => return Err(e.into()),
                    }
                },
                _ = cancellation.cancelled() => break
            };

            let io = TokioIo::new(stream);

            let conn = builder.serve_connection(io, svc.clone());
            let fut = graceful.watch(conn.into_owned());

            tokio::spawn(async move {
                let _ = fut.await.map_err(|e| {
                    if let Some(hyper_err) = e.downcast_ref::<hyper::Error>() {
                        // xxx: is there any way to get the error kind?
                        let err_str = format!("{:?}", hyper_err);

                        // This may imply a client shutdown race: https://github.com/hyperium/hyper/issues/3775
                        let err_not_connected = err_str.contains("NotConnected");
                        // There is no idle timeout, so header timeout is hit first
                        let err_hdr_timeout = err_str.contains("HeaderTimeout");

                        if !err_not_connected && !err_hdr_timeout {
                            error!("error serving connection: {:?}", hyper_err);
                        }
                    } else {
                        error!("error serving connection: {:?}", e);
                    }
                });
            });
        }

        // gracefully shutdown existing connections
        graceful.shutdown().await;

        Ok(())
    }
}

#[derive(Clone)]
pub struct TelemetryService {
    resource: Resource,
    bus_tx: BoundedSender<LambdaTelemetry>,
    logs_tx: BoundedSender<ResourceLogs>,
}

impl TelemetryService {
    fn new(
        resource: Resource,
        bus_tx: BoundedSender<LambdaTelemetry>,
        logs_tx: BoundedSender<ResourceLogs>,
    ) -> Self {
        Self {
            resource,
            bus_tx,
            logs_tx,
        }
    }
}

impl<H> Service<Request<H>> for TelemetryService
where
    H: Body + Send + Sync + 'static,
    <H as Body>::Data: Send + Sync + Clone,
    <H as Body>::Error: Display + Debug + Send + Sync + ToString,
{
    type Response = Response<Full<Bytes>>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<H>) -> Self::Future {
        let (parts, body) = req.into_parts();

        // This part could be decoupled out to a layer, but they are complicated
        // to setup, so inlining for now.
        if parts.method != Method::POST {
            return Box::pin(futures::future::ok(
                response_4xx(StatusCode::METHOD_NOT_ALLOWED).unwrap(),
            ));
        }

        if parts
            .headers
            .get(CONTENT_TYPE)
            .is_none_or(|ct| ct != "application/json")
        {
            return Box::pin(futures::future::ok(
                response_4xx(StatusCode::BAD_REQUEST).unwrap(),
            ));
        }

        Box::pin(handle_request(
            self.bus_tx.clone(),
            self.logs_tx.clone(),
            self.resource.clone(),
            body,
        ))
    }
}

async fn handle_request<H>(
    bus_tx: BoundedSender<LambdaTelemetry>,
    logs_tx: BoundedSender<ResourceLogs>,
    resource: Resource,
    body: H,
) -> Result<Response<Full<Bytes>>, BoxError>
where
    H: Body,
    <H as Body>::Error: Debug,
{
    let buf = body.collect().await.unwrap().to_bytes();

    let events: Vec<LambdaTelemetry> = serde_json::from_slice(&buf.to_vec())
        .map_err(|e| format!("unable to parse telemetry events from json: {}", e))?;

    let mut log_events = vec![];
    for event in events {
        // We should avoid logging on Extension or Function events, since it can cause a logging
        // loop
        match event.record {
            LambdaTelemetryRecord::Extension(log) => {
                log_events.push(Log::Extension(event.time, log));
                continue;
            }
            LambdaTelemetryRecord::Function(log) => {
                log_events.push(Log::Function(event.time, log));
                continue;
            }
            _ => {
                // Keep this for debugging for now
                debug!("received telemetry event from lambda: {:?}", event);
            }
        }

        match event.record {
            LambdaTelemetryRecord::PlatformRuntimeDone { .. } => {
                if let Err(e) = bus_tx.send(event.clone()).await {
                    error!("unable to send telemetry event to bus: {}", e);
                    // Should handle this?
                }
            }
            _ => {} // todo: handle more
        }
    }

    if !log_events.is_empty() {
        // Error logging here could create a loop, make sure to rate limit
        let logs = parse_logs(resource, log_events);
        match logs {
            Ok(rl) => {
                if let Err(e) = logs_tx.send(rl).await {
                    log_with_limit(move || warn!("Failed to send logs: {}", e));
                }
            }
            Err(e) => {
                log_with_limit(move || warn!("Failed to convert log events: {}", e));
            }
        }
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Full::default())
        .unwrap())
}

fn response_4xx(code: StatusCode) -> Result<Response<Full<Bytes>>, hyper::Error> {
    response_4xx_with_body(code, Bytes::default())
}

fn response_4xx_with_body(
    code: StatusCode,
    body: Bytes,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    Ok(Response::builder()
        .status(code)
        .body(Full::new(body))
        .unwrap())
}

fn resource_from_env() -> Resource {
    let mut r = Resource::default();

    r.attributes
        .push(otel_string_attr(FAAS_INVOKED_PROVIDER, "aws"));
    if let Ok(val) = std::env::var("AWS_LAMBDA_FUNCTION_NAME") {
        r.attributes
            .push(otel_string_attr(SERVICE_NAME, val.as_str()));
        r.attributes.push(otel_string_attr(FAAS_NAME, val.as_str()));
    } else {
        r.attributes
            .push(otel_string_attr(SERVICE_NAME, "unknown_service"));
    }

    if let Ok(val) = std::env::var("AWS_LAMBDA_FUNCTION_MEMORY_SIZE") {
        r.attributes
            .push(otel_string_attr(FAAS_MAX_MEMORY, val.as_str()));
    }
    if let Ok(val) = std::env::var("AWS_LAMBDA_FUNCTION_VERSION") {
        r.attributes
            .push(otel_string_attr(FAAS_VERSION, val.as_str()));
    }
    if let Ok(val) = std::env::var("AWS_REGION") {
        r.attributes
            .push(otel_string_attr(FAAS_INVOKED_REGION, val.as_str()))
    }

    r
}

fn log_with_limit<F: Fn()>(f: F) {
    // Don't block under any circumstance, prefer to just not log
    match LOG_LIMIT_LAST_LOG.try_lock() {
        Err(_) => return,
        Ok(mut g) => {
            let now = Instant::now();
            if g.is_none() {
                f();
                *g = Some(now)
            } else {
                if g.unwrap()
                    .add(Duration::from_secs(LOG_LIMIT_INTERVAL_SECS))
                    .lt(&now)
                {
                    f();
                    *g = Some(now);
                }
            }
        }
    };
}
