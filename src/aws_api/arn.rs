use crate::aws_api::error::Error;
use crate::aws_api::error::Error::ArnParseError;
use std::str::FromStr;

pub(crate) struct AwsArn {
    pub(crate) partition: String,
    pub(crate) service: String,
    pub(crate) region: String,
    pub(crate) account_id: String,
    pub(crate) resource_type: String,
    pub(crate) resource_id: String,
}

impl FromStr for AwsArn {
    type Err = Error;

    // Note, this will only handle ARNs were the resource type is included and split with ':'
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Split one more than we need to verify it's valid
        let mut parts: Vec<String> = s.splitn(8, ":").map(|s| s.to_string()).collect();
        if parts.len() != 7 {
            return Err(Error::ArnParseError(s.to_string()));
        }

        for part in &parts {
            if part.is_empty() {
                return Err(Error::ArnParseError(s.to_string()));
            }
        }

        let mut arn = AwsArn {
            partition: "".to_string(),
            service: "".to_string(),
            region: "".to_string(),
            account_id: "".to_string(),
            resource_type: "".to_string(),
            resource_id: "".to_string(),
        };

        arn.resource_id = parts.pop().unwrap();
        arn.resource_type = parts.pop().unwrap();
        arn.account_id = parts.pop().unwrap();
        arn.region = parts.pop().unwrap();
        arn.service = parts.pop().unwrap();
        arn.partition = parts.pop().unwrap();

        if parts.pop().unwrap() != "arn" {
            return Err(Error::ArnParseError(s.to_string()));
        }

        Ok(arn)
    }
}

impl AwsArn {
    pub fn get_endpoint(&self) -> String {
        let domain = if self.region.starts_with("cn-") {
            "amazonaws.com.cn"
        } else {
            "amazonaws.com"
        };

        format!("https://{}.{}.{}/", self.service, self.region, domain)
    }
}
