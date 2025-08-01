use crate::secrets::client::AwsClient;
use crate::secrets::{MAX_LOOKUP_LEN, PARAM_STORE_SERVICE, SECRETS_MANAGER_SERVICE};
use regex::Regex;
use rotel::aws_api::arn::AwsArn;
use rotel::aws_api::config::AwsConfig;
use std::collections::HashMap;
use tokio::time::Instant;
use tower::BoxError;
use tracing::{debug, warn};

pub struct EnvArnParser {
    arn_sub_re: Regex,
    secret_prefix_re: Regex,
}

impl EnvArnParser {
    pub fn new() -> Self {
        Self {
            arn_sub_re: Regex::new(r"\$\{(arn:[^}]+)}").unwrap(),
            secret_prefix_re: Regex::new(r"^secret://(arn:.+)$").unwrap(),
        }
    }

    pub fn extract_arns_from_env(&self) -> HashMap<String, String> {
        let mut sec_subs = HashMap::new();
        for (k, v) in std::env::vars() {
            if !k.starts_with("ROTEL_") {
                continue;
            }

            // Check for ${arn:...} format
            for capture in self.arn_sub_re.captures_iter(v.as_str()) {
                let matched = capture.get(1).unwrap().as_str().to_string();
                sec_subs.insert(matched, "".to_string());
            }

            // Check for secret://arn:... format
            if let Some(capture) = self.secret_prefix_re.captures(v.as_str()) {
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

            let mut result = v.clone();

            // Handle ${arn:...} format
            result = self
                .arn_sub_re
                .replace_all(result.as_str(), |caps: &regex::Captures| {
                    let matched = caps.get(1).unwrap().as_str();

                    match arn_map.get(matched) {
                        None => "",
                        Some(v) => v,
                    }
                })
                .into_owned();

            // Handle secret://arn:... format
            if let Some(capture) = self.secret_prefix_re.captures(result.as_str()) {
                let matched = capture.get(1).unwrap().as_str();
                if let Some(secret_value) = arn_map.get(matched) {
                    result = secret_value.clone();
                }
            }

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

    let mut arns_by_svc = HashMap::new();
    for (arn_str, _) in secure_arns.iter() {
        let arn = arn_str.parse::<AwsArn>()?;

        if arn.service() != SECRETS_MANAGER_SERVICE && arn.service() != PARAM_STORE_SERVICE {
            return Err(format!("Unknown secret ARN service name: {}", arn.service()).into());
        }

        if arn.service() == PARAM_STORE_SERVICE && arn.resource_field() != "" {
            return Err(format!(
                "JSON field selection not allowed for parameter store: {}",
                arn.to_string()
            )
            .into());
        }

        // This should never happen, but avoid silent bugs later
        if arn.to_string() != *arn_str {
            return Err(format!(
                "ARN value did not match input string: {} != {}",
                arn.to_string(),
                arn_str
            )
            .into());
        }

        let arn_without_field = arn.clone().set_resource_field("".to_string());

        arns_by_svc
            .entry(arn.service().clone())
            .or_insert_with(|| HashMap::new())
            .entry(arn_without_field)
            .or_insert_with(|| Vec::new())
            .push(arn);
    }

    for (svc, arns_by_base) in arns_by_svc {
        for arn_chunk in arns_by_base
            .keys()
            .cloned()
            .collect::<Vec<AwsArn>>()
            .chunks(MAX_LOOKUP_LEN)
        {
            if svc == SECRETS_MANAGER_SERVICE {
                let sm = client.secrets_manager();

                match sm.batch_get_secret(arn_chunk).await {
                    Ok(res) => {
                        for (arn, secret) in res {
                            let aws_arn = arn.parse::<AwsArn>()?;
                            match arns_by_base.get(&aws_arn) {
                                None => {
                                    return Err(format!(
                                        "Returned secret ARN was not found: {}",
                                        arn
                                    )
                                    .into());
                                }
                                Some(entry) => {
                                    for full_arn in entry {
                                        if full_arn.resource_field() == "" {
                                            secure_arns.insert(
                                                full_arn.to_string(),
                                                secret.secret_string.clone(),
                                            );
                                            continue;
                                        }

                                        match serde_json::from_str::<HashMap<String, String>>(
                                            secret.secret_string.as_str(),
                                        ) {
                                            Ok(json) => match json.get(full_arn.resource_field()) {
                                                None => return Err(format!(
                                                    "Secret JSON did not contain field {}: {:?}",
                                                    full_arn.resource_field(),
                                                    full_arn
                                                )
                                                .into()),
                                                Some(value) => {
                                                    secure_arns.insert(
                                                        full_arn.to_string(),
                                                        value.to_string(),
                                                    );
                                                }
                                            },
                                            Err(_) => {
                                                return Err(format!(
                                                    "Unable to parse secret string as JSON: {:?}",
                                                    full_arn
                                                )
                                                .into());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(err) => {
                        warn!(
                            "Unable to resolve ARNs from secrets manager: {:?}: {:?}",
                            arn_chunk, err,
                        );
                        return Err("Unable to resolve ARNs from secrets manager".into());
                    }
                }
            } else {
                let ps = client.parameter_store();

                match ps.get_parameters(arn_chunk).await {
                    Ok(res) => {
                        for (arn, param) in res {
                            secure_arns.insert(arn, param.value);
                        }
                    }
                    Err(err) => {
                        warn!(
                            "Unable to resolve ARNs from parameter store: {:?}: {:?}",
                            arn_chunk, err,
                        );
                        return Err("Unable to resolve ARNs from parameter store".into());
                    }
                }
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
    use crate::env::{EnvArnParser, resolve_secrets};
    use crate::test_util::{init_crypto, parse_test_arns};
    use rotel::aws_api::config::AwsConfig;
    use std::collections::HashMap;

    #[test]
    fn test_extract_and_update_arns_from_env() {
        unsafe { std::env::set_var("ROTEL_DONT_EXPAND", "${SOMETHING}") }
        unsafe { std::env::set_var("ROTEL_SINGLE", "${arn:test1}") }
        unsafe { std::env::set_var("ROTEL_MULTI", "${arn:test2} - ${arn:test3}") }
        unsafe { std::env::set_var("ROTEL_ALREADY_EXISTS", "Bearer ${arn:test2}") }
        unsafe { std::env::set_var("ROTEL_WONT_UPDATE", "empty:${arn:test4}") }
        unsafe { std::env::set_var("ROTEL_SECRET_PREFIX", "secret://arn:test5") }

        let es = EnvArnParser::new();
        let mut hm = es.extract_arns_from_env();

        assert_eq!(5, hm.len());
        assert!(hm.contains_key("arn:test1"));
        assert!(hm.contains_key("arn:test2"));
        assert!(hm.contains_key("arn:test3"));
        assert!(hm.contains_key("arn:test4"));
        assert!(hm.contains_key("arn:test5"));

        hm.insert("arn:test1".to_string(), "result-1".to_string());
        hm.insert("arn:test2".to_string(), "result-2".to_string());
        hm.insert("arn:test3".to_string(), "result-3".to_string());
        hm.insert("arn:test5".to_string(), "secret-result".to_string());

        es.update_env_arn_secrets(hm);

        assert_eq!("${SOMETHING}", std::env::var("ROTEL_DONT_EXPAND").unwrap());
        assert_eq!("result-1", std::env::var("ROTEL_SINGLE").unwrap());
        assert_eq!("result-2 - result-3", std::env::var("ROTEL_MULTI").unwrap());
        assert_eq!(
            "Bearer result-2",
            std::env::var("ROTEL_ALREADY_EXISTS").unwrap()
        );
        assert_eq!("empty:", std::env::var("ROTEL_WONT_UPDATE").unwrap());
        assert_eq!(
            "secret-result",
            std::env::var("ROTEL_SECRET_PREFIX").unwrap()
        );

        unsafe { std::env::remove_var("ROTEL_DONT_EXPAND") }
        unsafe { std::env::remove_var("ROTEL_SINGLE") }
        unsafe { std::env::remove_var("ROTEL_MULTI") }
        unsafe { std::env::remove_var("ROTEL_ALREADY_EXISTS") }
        unsafe { std::env::remove_var("ROTEL_WONT_UPDATE") }
        unsafe { std::env::remove_var("ROTEL_SECRET_PREFIX") }
    }

    #[tokio::test]
    async fn test_resolve_multiple_secrets() {
        // TEST_ENVSECRET_ARNS should be set to a comma-separated list of k=v pairs,
        // where k is an ARN of a secret and v is the secret value to test against.
        let test_envsecret_arns = std::env::var("TEST_ENVSECRET_ARNS");
        if !test_envsecret_arns.is_ok() {
            println!("Skipping test_resolve_multiple_secrets due to unset envvar");
            return;
        }

        let test_arns = parse_test_arns(test_envsecret_arns.unwrap());

        init_crypto();

        let mut test_arn_map = HashMap::new();
        for (test_arn, _) in &test_arns {
            test_arn_map.insert(test_arn.clone(), "".to_string());
        }

        let res = resolve_secrets(&AwsConfig::from_env(), &mut test_arn_map).await;
        assert!(res.is_ok());

        for (test_arn, test_value) in test_arns {
            let result = test_arn_map.get(&test_arn).unwrap();
            assert_eq!(test_value, *result);
        }
    }

    #[tokio::test]
    async fn test_resolve_secrets_with_failures() {
        let test_envsecret_arns = std::env::var("TEST_ENVSECRET_FAIL_ARNS");
        if !test_envsecret_arns.is_ok() {
            println!("Skipping test_resolve_secrets_with_failures due to unset envvar");
            return;
        }

        let test_arns = parse_test_arns(test_envsecret_arns.unwrap());

        init_crypto();

        for (test_arn, _) in &test_arns {
            let mut test_arn_map = HashMap::new();
            test_arn_map.insert(test_arn.clone(), "".to_string());

            let res = resolve_secrets(&AwsConfig::from_env(), &mut test_arn_map).await;
            assert!(res.is_err());
        }
    }
}
