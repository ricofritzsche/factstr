use serde_json::Value;

pub(crate) fn payload_subset_matches(payload_predicate: &Value, payload: &Value) -> bool {
    match (payload_predicate, payload) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(expected), Value::Bool(actual)) => expected == actual,
        (Value::Number(expected), Value::Number(actual)) => expected == actual,
        (Value::String(expected), Value::String(actual)) => expected == actual,
        (Value::Array(expected_items), Value::Array(actual_items)) => {
            expected_items.iter().all(|expected_item| {
                actual_items
                    .iter()
                    .any(|actual_item| payload_subset_matches(expected_item, actual_item))
            })
        }
        (Value::Object(expected_fields), Value::Object(actual_fields)) => {
            expected_fields.iter().all(|(field_name, expected_value)| {
                actual_fields.get(field_name).is_some_and(|actual_value| {
                    payload_subset_matches(expected_value, actual_value)
                })
            })
        }
        _ => false,
    }
}
