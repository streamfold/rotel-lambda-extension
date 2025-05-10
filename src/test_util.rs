use std::sync::Once;

static INIT_CRYPTO: Once = Once::new();
pub fn init_crypto() {
    INIT_CRYPTO.call_once(|| {
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .unwrap()
    });
}

pub fn parse_test_arns(test_arns: String) -> Vec<(String, String)> {
    test_arns
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
        .collect()
}