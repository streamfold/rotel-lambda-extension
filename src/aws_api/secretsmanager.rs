use serde::{Deserialize, Serialize};
use serde_json::json;
use http::{HeaderMap, HeaderValue, Method, Request, Uri};
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
        let endpoint = endpoint.parse::<Uri>()?;

        let payload = json!({
            "SecretId": secret_arn,
        });
        println!("loooking up payload: {:?}", payload);
        let payload_bytes = serde_json::to_vec(&payload)?;

        let mut hdrs = HeaderMap::new();
        hdrs.insert("X-Amz-Target", HeaderValue::from_static("secretsmanager.GetSecretValue"));
        hdrs.insert(CONTENT_TYPE, HeaderValue::from_static("application/x-amz-json-1.1"));

        // Sign the request
        let signer = AwsRequestSigner::new(
            self.service_name,
            &arn.region,
            &self.client.config.aws_access_key_id,
            &self.client.config.aws_secret_access_key,
            self.client.config.aws_session_token.as_deref(),
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
    use std::sync::Once;
    use crate::aws_api::client::AwsClient;
    use crate::aws_api::config::AwsConfig;

    #[tokio::test]
    #[ignore]
    async fn test_basic_secret_retrieval() {
        init_crypto();
        
        let client = AwsClient::new(AwsConfig{
            region: "us-east-2".to_string(),
            aws_access_key_id: std::env::var("AWS_ACCESS_KEY_ID").unwrap(),
            aws_secret_access_key: std::env::var("AWS_SECRET_ACCESS_KEY").unwrap(),
            aws_session_token: Some(std::env::var("AWS_SESSION_TOKEN").unwrap()),
        }).unwrap();
        
        let ss = client.secrets_manager();
        
        //let arn = "arn:aws:secretsmanager:us-east-1:891377354357:secret:lambda-api-key-CiiqiD";
        let arn = "arn:aws:secretsmanager:us-east-2:891377354357:secret:test-ohio-secret-L86lpn";
        let val = ss.get_secret_value(arn).await.unwrap();

        assert_eq!("1234abcd", val.secret_string.unwrap());
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