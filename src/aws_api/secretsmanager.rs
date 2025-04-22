use crate::aws_api::arn::AwsArn;
use crate::aws_api::auth::{AwsRequestSigner, SystemClock};
use crate::aws_api::client::AwsClient;
use crate::aws_api::error::Error;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, Method, Uri};
use serde::Deserialize;
use serde_json::json;

pub struct SecretsManager<'a> {
    client: &'a AwsClient,
    service_name: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct GetSecretValueResponse {
    #[serde(rename = "ARN")]
    pub arn: Option<String>,
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "VersionId")]
    pub version_id: Option<String>,
    #[serde(rename = "SecretString")]
    pub secret_string: Option<String>,
    #[serde(rename = "SecretBinary")]
    pub secret_binary: Option<String>,
    #[serde(rename = "VersionStages")]
    pub version_stages: Option<Vec<String>>,
    #[serde(rename = "CreatedDate")]
    pub created_date: Option<f64>,
}

impl<'a> SecretsManager<'a> {
    pub(crate) fn new(client: &'a AwsClient) -> Self {
        Self {
            client,
            service_name: "secretsmanager",
        }
    }

    pub async fn get_secret_value(
        &self,
        secret_arn: &str,
    ) -> Result<GetSecretValueResponse, Error> {
        let arn = secret_arn.parse::<AwsArn>()?;

        if arn.service != self.service_name {
            return Err(Error::ArnParseError(secret_arn.to_string()));
        }

        let endpoint = arn.get_endpoint();
        let endpoint = endpoint.parse::<Uri>()?;

        let payload = json!({
            "SecretId": secret_arn,
        });
        let payload_bytes = serde_json::to_vec(&payload)?;

        let mut hdrs = HeaderMap::new();
        hdrs.insert(
            "X-Amz-Target",
            HeaderValue::from_static("secretsmanager.GetSecretValue"),
        );
        hdrs.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-amz-json-1.1"),
        );

        // Sign the request
        let signer = AwsRequestSigner::new(
            self.service_name,
            &arn.region,
            &self.client.config.aws_access_key_id,
            &self.client.config.aws_secret_access_key,
            self.client.config.aws_session_token.as_deref(),
            SystemClock {},
        );
        let signed_request = signer.sign(endpoint, Method::POST, hdrs, payload_bytes)?;

        // Send the request
        let response = self.client.perform(signed_request).await?;

        let result: GetSecretValueResponse = serde_json::from_slice(response.as_ref())?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aws_api::config::AwsConfig;
    use std::sync::Once;

    #[tokio::test]
    async fn test_basic_secret_retrieval() {
        // TEST_SECRETSMANAGER_ARNS should be set to a comma-separated list of k=v pairs,
        // where k is an ARN of a secret and v is the secret value to test against.
        let test_secret_arns = std::env::var("TEST_SECRETSMANAGER_ARNS");
        if !test_secret_arns.is_ok() {
            println!("Skipping test_basic_secret_retrieval due to unset envvar");
            return;
        }

        let test_arns: Vec<(String, String)> = test_secret_arns
            .unwrap()
            .split(",")
            .filter(|s| !s.is_empty())
            .filter_map(|pair| {
                let parts: Vec<&str> = pair.splitn(2, '=').collect();
                if parts.len() == 2 {
                    Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
                } else {
                    None // Skip malformed pairs that don't have an equals sign
                }
            })
            .collect();

        init_crypto();

        let client = AwsClient::new(AwsConfig::from_env()).unwrap();

        let ss = client.secrets_manager();

        for test_arn in &test_arns {
            let val = ss.get_secret_value(test_arn.0.as_str()).await.unwrap();

            assert_eq!(test_arn.1, val.secret_string.unwrap());
        }
    }

    static INIT_CRYPTO: Once = Once::new();
    pub fn init_crypto() {
        INIT_CRYPTO.call_once(|| {
            rustls::crypto::ring::default_provider()
                .install_default()
                .unwrap()
        });
    }
}
