use std::sync::Once;

// used for testing
#[allow(dead_code)]
static INIT_CRYPTO: Once = Once::new();
#[allow(dead_code)]
pub fn init_crypto() {
    INIT_CRYPTO.call_once(|| {
        rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .unwrap()
    });
}
