use crate::lambda::otel_string_attr;
use chrono::{DateTime, Utc};
use opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue;
use opentelemetry_proto::tonic::common::v1::{AnyValue, InstrumentationScope};
use opentelemetry_proto::tonic::logs::v1::{LogRecord, ResourceLogs, ScopeLogs, SeverityNumber};
use opentelemetry_proto::tonic::resource::v1::Resource;
use opentelemetry_semantic_conventions::attribute::FAAS_INVOCATION_ID;
use serde_json::Value;
use std::time::SystemTime;
use tower::BoxError;

const LOG_SCOPE: &str = "github.com/streamfold/rotel-lambda-extension";

pub(crate) enum Log {
    Function(DateTime<Utc>, Value),
    Extension(DateTime<Utc>, Value),
}

impl Log {
    fn get_type(&self) -> String {
        match self {
            Log::Function { .. } => "function".to_string(),
            Log::Extension { .. } => "extension".to_string(),
        }
    }

    fn into_parts(self) -> (DateTime<Utc>, serde_json::Value) {
        match self {
            Log::Function(dt, l) => (dt, l),
            Log::Extension(dt, l) => (dt, l),
        }
    }
}

pub(crate) fn parse_logs(resource: Resource, logs: Vec<Log>) -> Result<ResourceLogs, BoxError> {
    let mut rl = ResourceLogs {
        resource: Some(resource),
        ..Default::default()
    };

    let mut sl = ScopeLogs {
        scope: Some(InstrumentationScope {
            name: LOG_SCOPE.to_string(),
            ..Default::default()
        }),
        ..Default::default()
    };

    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();

    let log_records: Result<Vec<_>, _> = logs
        .into_iter()
        .map(|log| {
            let log_type = log.get_type();
            let (time, record) = log.into_parts();

            let mut lr = LogRecord::default();

            lr.attributes
                .push(otel_string_attr("type", log_type.as_str()));
            lr.time_unix_nano = time.timestamp_nanos_opt().unwrap_or(now.as_nanos() as i64) as u64;
            lr.observed_time_unix_nano = now.as_nanos() as u64;

            // Logs can be JSON or String
            // https://docs.aws.amazon.com/lambda/latest/dg/telemetry-schema-reference.html#telemetry-api-function
            match record {
                Value::Object(mut rec) => {
                    if let Some(Value::String(ts)) = rec.get("value") {
                        if let Ok(dt) = DateTime::parse_from_rfc3339(ts.as_str()) {
                            if let Some(nanos) = dt.timestamp_nanos_opt() {
                                lr.time_unix_nano = nanos as u64;
                            }
                        }
                    }
                    if let Some(Value::String(level)) = rec.get("level") {
                        lr.severity_number = i32::from(severity_text_to_number(level));
                        lr.severity_text = lr.severity_number().as_str_name().to_string();
                    }
                    if let Some(Value::String(request_id)) = rec.get("requestId") {
                        lr.attributes
                            .push(otel_string_attr(FAAS_INVOCATION_ID, request_id));
                    }
                    if let Some(Value::String(msg)) = rec.remove("message") {
                        lr.body = Some(AnyValue {
                            value: Some(StringValue(msg)),
                        })
                    }
                }
                Value::String(rec) => {
                    lr.body = Some(AnyValue {
                        value: Some(StringValue(rec)),
                    })
                }
                _ => {
                    return Err(format!("invalid log record type: {:?}", record));
                }
            };

            Ok(lr)
        })
        .collect();

    match log_records {
        Ok(lr) => sl.log_records = lr,
        Err(e) => return Err(format!("Failed to parse log records: {}", e).into()),
    }

    rl.scope_logs = vec![sl];

    Ok(rl)
}

fn severity_text_to_number(level: &String) -> SeverityNumber {
    let upper = level.to_uppercase();

    match upper.as_str() {
        "TRACE" => SeverityNumber::Trace,
        "TRACE2" => SeverityNumber::Trace2,
        "TRACE3" => SeverityNumber::Trace3,
        "TRACE4" => SeverityNumber::Trace4,
        "DEBUG" => SeverityNumber::Debug,
        "DEBUG2" => SeverityNumber::Debug2,
        "DEBUG3" => SeverityNumber::Debug3,
        "DEBUG4" => SeverityNumber::Debug4,
        "INFO" => SeverityNumber::Info,
        "INFO2" => SeverityNumber::Info2,
        "INFO3" => SeverityNumber::Info3,
        "INFO4" => SeverityNumber::Info4,
        "WARN" => SeverityNumber::Warn,
        "WARN2" => SeverityNumber::Warn2,
        "WARN3" => SeverityNumber::Warn3,
        "WARN4" => SeverityNumber::Warn4,
        "ERROR" => SeverityNumber::Error,
        "ERROR2" => SeverityNumber::Error2,
        "ERROR3" => SeverityNumber::Error3,
        "ERROR4" => SeverityNumber::Error4,
        "FATAL" => SeverityNumber::Fatal,
        "FATAL2" => SeverityNumber::Fatal2,
        "FATAL3" => SeverityNumber::Fatal3,
        "FATAL4" => SeverityNumber::Fatal4,
        "CRITICAL" => SeverityNumber::Fatal,
        "ALL" => SeverityNumber::Trace,
        "WARNING" => SeverityNumber::Warn,
        _ => SeverityNumber::Unspecified,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use lambda_extension::LambdaTelemetryRecord;
    use serde::{Deserialize, Serialize};

    #[test]
    fn test_json_record() {
        let json_rec = r#"{
    "time": "2022-10-12T00:03:50.000Z",
    "type": "extension",
    "record": {
       "timestamp": "2022-10-12T00:03:50.000Z",
       "level": "INFO",
       "requestId": "79b4f56e-95b1-4643-9700-2807f4e68189",
       "message": "Hello world, I am an extension!"
    }
}"#;
        let str_rec = r#"{
    "time": "2022-10-12T00:03:50.000Z",
    "type": "function",
    "record": "[INFO] Hello world, I am an extension!"
}"#;

        let as_json: LambdaTelemetryRecord = serde_json::from_str(json_rec).unwrap();
        println!("as json: {:?}", as_json);

        let as_str: LambdaTelemetryRecord = serde_json::from_str(str_rec).unwrap();
        println!("as str: {:?}", as_str);
    }
}
