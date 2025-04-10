use opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue;
use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};

pub mod api;
mod constants;
mod logs;
pub mod telemetry_api;
pub mod types;

pub(crate) fn otel_string_attr(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(StringValue(value.to_string())),
        }),
    }
}
