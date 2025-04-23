use crate::aws_api::client::AwsClient;
use crate::aws_api::config::AwsConfig;
use regex::Regex;
use std::collections::HashMap;
use tokio::time::Instant;
use tower::BoxError;
use tracing::{debug, warn};

pub struct EnvArnParser {
    arn_sub_re: Regex,
}

impl EnvArnParser {
    pub fn new() -> Self {
        Self {
            arn_sub_re: Regex::new(r"\$\{(arn:[^}]+)}").unwrap(),
        }
    }

    pub fn extract_arns_from_env(&self) -> HashMap<String, String> {
        let mut sec_subs = HashMap::new();
        for (k, v) in std::env::vars() {
            if !k.starts_with("ROTEL_") {
                continue;
            }

            for capture in self.arn_sub_re.captures_iter(v.as_str()) {
                let matched = capture.get(1).unwrap().as_str().to_string();
                sec_subs.insert(matched, "".to_string());
            }
        }

        sec_subs
    }

    pub fn update_env_arn_secrets(&self, arn_map: HashMap<String, String>) {
        let mut updates = HashMap::new();
        for (k, v) in std::env::vars() {
            if !k.starts_with("ROTEL_") {
                continue;
            }

            let result = self
                .arn_sub_re
                .replace_all(v.as_str(), |caps: &regex::Captures| {
                    let matched = caps.get(1).unwrap().as_str();

                    match arn_map.get(matched) {
                        None => "",
                        Some(v) => v,
                    }
                })
                .into_owned();

            if v != result {
                updates.insert(k, result);
            }
        }

        for (k, v) in updates {
            unsafe { std::env::set_var(k, v.to_string()) }
        }
    }
}

pub async fn resolve_secrets(
    aws_config: &AwsConfig,
    secure_arns: &mut HashMap<String, String>,
) -> Result<(), BoxError> {
    let secrets_start = Instant::now();

    let client = AwsClient::new(aws_config.clone())?;
    let sm = client.secrets_manager();

    for (arn, value) in secure_arns.iter_mut() {
        match sm.get_secret_value(arn.as_str()).await {
            Ok(resp) => {
                if let Some(secret) = resp.secret_string {
                    *value = secret;
                }
            }
            Err(err) => {
                warn!(
                    "Unable to resolve the secrets arn {}: {}, skipping for now",
                    arn, err
                );
                // should this be fatal?
            }
        }
    }

    debug!(
        "Resolved all secrets in {} ms",
        Instant::now().duration_since(secrets_start).as_millis()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::env::EnvArnParser;

    #[test]
    fn test_extract_and_update_arns_from_env() {
        unsafe { std::env::set_var("ROTEL_DONT_EXPAND", "${SOMETHING}") }
        unsafe { std::env::set_var("ROTEL_SINGLE", "${arn:test1}") }
        unsafe { std::env::set_var("ROTEL_MULTI", "${arn:test2} - ${arn:test3}") }
        unsafe { std::env::set_var("ROTEL_ALREADY_EXISTS", "Bearer ${arn:test2}") }
        unsafe { std::env::set_var("ROTEL_WONT_UPDATE", "empty:${arn:test4}") }

        let es = EnvArnParser::new();
        let mut hm = es.extract_arns_from_env();

        assert_eq!(4, hm.len());
        assert!(hm.contains_key("arn:test1"));
        assert!(hm.contains_key("arn:test2"));
        assert!(hm.contains_key("arn:test3"));
        assert!(hm.contains_key("arn:test4"));

        hm.insert("arn:test1".to_string(), "result-1".to_string());
        hm.insert("arn:test2".to_string(), "result-2".to_string());
        hm.insert("arn:test3".to_string(), "result-3".to_string());

        es.update_env_arn_secrets(hm);

        assert_eq!("${SOMETHING}", std::env::var("ROTEL_DONT_EXPAND").unwrap());
        assert_eq!("result-1", std::env::var("ROTEL_SINGLE").unwrap());
        assert_eq!("result-2 - result-3", std::env::var("ROTEL_MULTI").unwrap());
        assert_eq!(
            "Bearer result-2",
            std::env::var("ROTEL_ALREADY_EXISTS").unwrap()
        );
        assert_eq!("empty:", std::env::var("ROTEL_WONT_UPDATE").unwrap());

        unsafe { std::env::remove_var("ROTEL_DONT_EXPAND") }
        unsafe { std::env::remove_var("ROTEL_SINGLE") }
        unsafe { std::env::remove_var("ROTEL_MULTI") }
        unsafe { std::env::remove_var("ROTEL_ALREADY_EXISTS") }
        unsafe { std::env::remove_var("ROTEL_WONT_UPDATE") }
    }
}
