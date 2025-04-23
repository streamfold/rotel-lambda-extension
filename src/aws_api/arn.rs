use crate::aws_api::error::Error;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_arn_valid() {
        let input = "arn:aws:secretsmanager:us-east-2:891477334659:secret:test-ohio-secret-L86lpn";

        let arn = input.parse::<AwsArn>().unwrap();

        assert_eq!("aws", arn.partition);
        assert_eq!("secretsmanager", arn.service);
        assert_eq!("us-east-2", arn.region);
        assert_eq!("891477334659", arn.account_id);
        assert_eq!("secret", arn.resource_type);
        assert_eq!("test-ohio-secret-L86lpn", arn.resource_id);
    }

    #[test]
    fn test_parse_arn_invalid() {
        assert!(
            !"arn:aws:secretsmanager:us-east-2:891477334659:secret:test-ohio-secret-L86lpn:extra"
                .parse::<AwsArn>()
                .is_ok()
        );
        assert!(
            !"arn:aws:secretsmanager::891477334659:secret:test-ohio-secret-L86lpn"
                .parse::<AwsArn>()
                .is_ok()
        );
        assert!(
            !"arn:aws:secretsmanager891477334659:secret:test-ohio-secret-L86lpn"
                .parse::<AwsArn>()
                .is_ok()
        );
    }
}
