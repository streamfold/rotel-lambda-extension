
pub struct AwsConfig {
    pub(crate) region: String,
    pub(crate) aws_access_key_id: String,
    pub(crate) aws_secret_access_key: String,
    pub(crate) aws_session_token: Option<String>,
}