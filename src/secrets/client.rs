use crate::secrets::error::Error;
use crate::secrets::paramstore::ParameterStore;
use crate::secrets::secretsmanager::SecretsManager;
use crate::util::http::response_string;
use bytes::Bytes;
use http::Request;
use http_body_util::{BodyExt, Full};
use hyper_rustls::ConfigBuilderExt;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client as HyperClient;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use rotel::aws_api::creds::AwsCreds;
use rustls::ClientConfig;
use std::time::Duration;
use tower::BoxError;

/// Main client for AWS services
pub struct AwsClient {
    pub(crate) creds: AwsCreds,
    client: HyperClient<HttpsConnector<HttpConnector>, Full<Bytes>>,
}

impl AwsClient {
    /// Create a new AWS client
    pub fn new(creds: AwsCreds) -> Result<Self, BoxError> {
        let client = build_hyper_client()?;

        Ok(Self { client, creds })
    }

    /// Get an instance of the SecretsManager service
    pub fn secrets_manager(&self) -> SecretsManager<'_> {
        SecretsManager::new(self)
    }

    /// Get an instance of the ParameterStore service
    pub fn parameter_store(&self) -> ParameterStore<'_> {
        ParameterStore::new(self)
    }

    pub async fn perform(&self, req: Request<Full<Bytes>>) -> Result<Bytes, Error> {
        let resp = self.client.request(req).await?;

        // Handle AWS errors
        let (parts, body) = resp.into_parts();
        if !parts.status.is_success() {
            let error_body = response_string(body).await?;

            return Err(Error::AwsError {
                code: parts.status.as_str().to_string(),
                message: error_body,
            });
        }

        // Parse success response
        Ok(body.collect().await?.to_bytes())
    }
}

fn build_hyper_client() -> Result<HyperClient<HttpsConnector<HttpConnector>, Full<Bytes>>, BoxError>
{
    let tls_config = ClientConfig::builder()
        .with_native_roots()?
        .with_no_client_auth();

    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config(tls_config)
        .https_or_http()
        .enable_http2()
        .build();

    let client = hyper_util::client::legacy::Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(2)
        .timer(TokioTimer::new())
        .build::<_, Full<Bytes>>(https);

    Ok(client)
}
