use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use http::{HeaderValue, Method, Request};
use http::header::CONTENT_TYPE;
use crate::aws_api::arn::AwsArn;
use crate::aws_api::auth::AwsRequestSigner;
use crate::aws_api::client::AwsClient;
use crate::aws_api::error::Error;

pub struct SecretsManager<'a> {
    client: &'a AwsClient,
    service_name: &'static str,
    api_version: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct GetSecretValueResponse {
    pub ARN: Option<String>,
    pub Name: Option<String>,
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
    pub fn new(client: &'a AwsClient) -> Self {
        Self {
            client,
            service_name: "secretsmanager",
            api_version: "2017-10-17",
        }
    }

    pub async fn get_secret_value(&self, secret_arn: &str) -> Result<GetSecretValueResponse, Error> {
        let arn = secret_arn.parse::<AwsArn>()?;
        
        if arn.service != self.service_name {
            return Err(Error::ArnParseError(secret_arn.to_string()));
        }
        
        let endpoint = arn.get_endpoint();

        let payload = json!({
            "SecretId": arn.resource_id,
        });
        let payload_bytes = serde_json::to_vec(&payload)?;

        let req_builder = Request::builder()
            .method(Method::POST)
            .uri(&endpoint)
            .header("X-Amz-Target", HeaderValue::from_static("secretsmanager.GetSecretValue"))
            .header(CONTENT_TYPE, HeaderValue::from_static("application/x-amz-json-1.1"));
            //.body(payload_bytes);
        
        // Sign the request
        let signer = AwsRequestSigner::new(
            self.service_name,
            &arn.region,
            &self.client.config.aws_access_key_id,
            &self.client.config.aws_secret_access_key,
            self.client.config.aws_session_token.as_deref(),
        );
        let signed_request = signer.sign(req_builder, &payload_bytes)?;

        // Send the request
        let response = self.client.perform(signed_request).await?;
        
        let result: GetSecretValueResponse = serde_json::from_slice(response.as_ref())?;

        Ok(result)
    }
}