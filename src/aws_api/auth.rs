use crate::aws_api::error::Error;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use http::header::{AUTHORIZATION, HOST};
use http::request::Builder;
use http::{HeaderMap, HeaderValue, Method, Request, Uri};
use http_body_util::Full;
use sha2::Digest;
use sha2::Sha256;
use std::str;
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

pub struct AwsRequestSigner<'a> {
    service: &'a str,
    region: &'a str,
    access_key: &'a str,
    secret_key: &'a str,
    session_token: Option<&'a str>,
    time: DateTime<Utc>,
}

impl<'a> AwsRequestSigner<'a> {
    pub fn new(
        service: &'a str,
        region: &'a str,
        access_key: &'a str,
        secret_key: &'a str,
        session_token: Option<&'a str>,
    ) -> Self {
        Self {
            service,
            region,
            access_key,
            secret_key,
            session_token,
            time: Utc::now(),
        }
    }

    pub fn sign(
        &self,
        uri: Uri,
        method: Method,
        headers: HeaderMap,
        payload: Vec<u8>,
    ) -> Result<Request<Full<Bytes>>, Error> {
        let amz_date = self.time.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = self.time.format("%Y%m%d").to_string();

        let host = uri.host().unwrap();

        // Add host header if it doesn't exist
        let mut headers_mut = headers;
        if !headers_mut.contains_key(HOST) {
            let port = uri.port();
            let host_value = if let Some(port) = port {
                format!("{}:{}", host, port)
            } else {
                host.to_string()
            };

            headers_mut.insert(
                HOST,
                HeaderValue::from_str(&host_value)
                    .map_err(|_| Error::SignatureError("Invalid host header".to_string()))?,
            );
        }

        // Add session token if provided
        if let Some(token) = self.session_token {
            headers_mut.insert(
                "X-Amz-Security-Token",
                HeaderValue::from_str(token)
                    .map_err(|_| Error::SignatureError("Invalid session token".to_string()))?,
            );
        }

        // Add date header
        headers_mut.insert(
            "X-Amz-Date",
            HeaderValue::from_str(&amz_date)
                .map_err(|_| Error::SignatureError("Invalid date".to_string()))?,
        );

        // Step 1: Create canonical request
        let canonical_uri = uri.path();

        let query = uri.path_and_query().unwrap().query();
        let canonical_querystring = match query {
            None => "".to_string(),
            Some(q) => {
                // Collect and sort query parameters
                let mut query_params: Vec<(String, String)> = uri
                    .path_and_query()
                    .unwrap()
                    .query()
                    .unwrap_or("")
                    .split("&")
                    .map(|s| {
                        let splits: Vec<&str> = s.splitn(2, "=").collect();
                        if splits.len() > 1 {
                            (splits[0], splits[1])
                        } else {
                            (splits[0], "")
                        }
                    })
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                query_params.sort();

                let canonical_querystring = query_params
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<String>>()
                    .join("&");

                canonical_querystring
            }
        };
        
        // Get and sort headers
        let mut canonical_headers = String::new();
        let mut signed_headers = Vec::new();

        let mut headers: Vec<(String, String)> = headers_mut
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_lowercase(),
                    value.to_str().unwrap_or_default().trim().to_string(),
                )
            })
            .collect();
        headers.sort_by(|a, b| a.0.cmp(&b.0));

        for (name, value) in &headers {
            canonical_headers.push_str(&format!("{}:{}\n", name, value));
            signed_headers.push(name.clone());
        }

        let signed_headers_str = signed_headers.join(";");
        
        // Calculate payload hash
        let payload_hash = hex::encode(Sha256::digest(&payload));

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method,
            canonical_uri,
            canonical_querystring,
            canonical_headers,
            signed_headers_str,
            payload_hash
        );
        
        // Step 2: Create the string to sign
        let algorithm = "AWS4-HMAC-SHA256";
        let credential_scope = format!(
            "{}/{}/{}/aws4_request",
            date_stamp, self.region, self.service
        );
        let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));

        let string_to_sign = format!(
            "{}\n{}\n{}\n{}",
            algorithm, amz_date, credential_scope, canonical_request_hash
        );

        // Step 3: Calculate the signature
        let signature = self.calculate_signature(&date_stamp, &string_to_sign)?;

        // Step 4: Add signature to request header
        let authorization_header = format!(
            "{} Credential={}/{}, SignedHeaders={}, Signature={}",
            algorithm, self.access_key, credential_scope, signed_headers_str, signature
        );

        headers_mut.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&authorization_header)
                .map_err(|_| Error::SignatureError("Invalid authorization header".to_string()))?,
        );

        let mut req_builder = Request::builder()
            .uri(uri)
            .method(method);
        
        let builder_headers = req_builder.headers_mut().unwrap();
        for (k, v) in headers_mut.iter() {
            builder_headers.insert(k, v.clone());
        }
        
        Ok(req_builder
            .body(Full::from(Bytes::from(payload)))
            .map_err(|e| Error::RequestBuildError(e))?)
    }

    fn calculate_signature(&self, date_stamp: &str, string_to_sign: &str) -> Result<String, Error> {
        // Create signing key
        let k_secret = format!("AWS4{}", self.secret_key);

        let k_date = self.sign_hmac(k_secret.as_bytes(), date_stamp.as_bytes())?;
        let k_region = self.sign_hmac(&k_date, self.region.as_bytes())?;
        let k_service = self.sign_hmac(&k_region, self.service.as_bytes())?;
        let k_signing = self.sign_hmac(&k_service, b"aws4_request")?;

        // Sign the string to sign with the signing key
        let signature = self.sign_hmac(&k_signing, string_to_sign.as_bytes())?;
        Ok(hex::encode(signature))
    }

    fn sign_hmac(&self, key: &[u8], message: &[u8]) -> Result<Vec<u8>, Error> {
        let mut mac = HmacSha256::new_from_slice(key)
            .map_err(|_| Error::SignatureError("Invalid HMAC key".to_string()))?;
        mac.update(message);
        Ok(mac.finalize().into_bytes().to_vec())
    }
}
