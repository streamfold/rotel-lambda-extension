use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue};
use opentelemetry_proto::tonic::common::v1::any_value::Value::StringValue;

pub mod api;
mod constants;
pub mod telemetry_api;
pub mod types;
mod logs;


pub(crate) fn otel_string_attr(key: &str, value: &str) -> KeyValue {
    KeyValue {
        key: key.to_string(),
        value: Some(AnyValue {
            value: Some(StringValue(value.to_string())),
        }),
    }
}
