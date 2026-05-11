//! Compute `{ field: { from, to } }` delta between two JSON snapshots.
//!
//! Used by `AuditWriter::with_audit` to materialise the audit row's `delta`
//! column. Identical fields are omitted.

use serde_json::{Map, Value};

/// Diff two JSON objects at the top level. Non-object inputs degrade to
/// `{ ".": { from, to } }` so the audit row is never empty.
pub fn compute_delta(before: &Value, after: &Value) -> Value {
    match (before, after) {
        (Value::Object(b), Value::Object(a)) => Value::Object(diff_objects(b, a)),
        (b, a) if b == a => Value::Object(Map::new()),
        (b, a) => {
            let mut wrapper = Map::new();
            let mut inner = Map::new();
            inner.insert("from".into(), b.clone());
            inner.insert("to".into(), a.clone());
            wrapper.insert(".".into(), Value::Object(inner));
            Value::Object(wrapper)
        }
    }
}

fn diff_objects(before: &Map<String, Value>, after: &Map<String, Value>) -> Map<String, Value> {
    let mut out: Map<String, Value> = Map::new();
    let keys: std::collections::BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    for key in keys {
        let b = before.get(key).cloned().unwrap_or(Value::Null);
        let a = after.get(key).cloned().unwrap_or(Value::Null);
        if b == a {
            continue;
        }
        let mut entry = Map::new();
        entry.insert("from".into(), b);
        entry.insert("to".into(), a);
        out.insert(key.clone(), Value::Object(entry));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn omits_identical_fields() {
        let before = json!({ "a": 1, "b": 2, "c": 3 });
        let after = json!({ "a": 1, "b": 99, "c": 3 });
        let delta = compute_delta(&before, &after);
        let obj = delta.as_object().unwrap();
        assert_eq!(obj.len(), 1);
        assert_eq!(obj["b"]["from"], json!(2));
        assert_eq!(obj["b"]["to"], json!(99));
    }

    #[test]
    fn handles_added_and_removed_fields() {
        let before = json!({ "a": 1 });
        let after = json!({ "b": 2 });
        let delta = compute_delta(&before, &after);
        let obj = delta.as_object().unwrap();
        assert_eq!(obj["a"]["to"], json!(null));
        assert_eq!(obj["b"]["from"], json!(null));
    }
}
