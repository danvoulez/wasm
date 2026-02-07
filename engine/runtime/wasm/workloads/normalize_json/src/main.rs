/// Normalize-JSON workload: reads JSON from stdin, outputs with sorted keys.
use std::io::{self, Read, Write};

fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).expect("failed to read stdin");

    let value: serde_json::Value = serde_json::from_str(&input).expect("invalid JSON input");
    let sorted = sort_value(&value);
    let output = serde_json::to_string_pretty(&sorted).expect("failed to serialize");
    io::stdout().write_all(output.as_bytes()).expect("failed to write stdout");
    io::stdout().write_all(b"\n").expect("failed to write newline");
}

fn sort_value(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by_key(|(k, _)| k.clone());
            let sorted: serde_json::Map<String, Value> = entries
                .into_iter()
                .map(|(k, v)| (k.clone(), sort_value(v)))
                .collect();
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sort_value).collect()),
        other => other.clone(),
    }
}
