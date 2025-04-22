use http_body_util::BodyExt;
use hyper::body::Incoming;
use tower::BoxError;

pub async fn response_string(body: Incoming) -> Result<String, BoxError> {
    Ok(body
        .collect()
        .await
        .map_err(|e| format!("Failed to read response {}", e))
        .map(|c| c.to_bytes())
        .map(|s| String::from_utf8(s.to_vec()))?
        .map_err(|e| format!("Unable to convert response body to string: {}", e))?)
}