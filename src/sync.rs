//! Diff-based reverse-ETL sync plan: what to insert, update or leave alone in the destination.
use crate::util::*;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub fn plan_op(i: &Value) -> R<Value> {
    let key = s(i, "key")?;
    let mapping = obj(i, "mapping")?; // warehouse column -> destination field
    let src = arr(i, "warehouse_rows")?;
    let dst = arr(i, "destination_rows")?;
    let delete_missing = i.get("delete_missing").and_then(Value::as_bool).unwrap_or(false);
    let mut dmap: BTreeMap<String, &Value> = BTreeMap::new();
    let dkey = mapping.get(key).and_then(Value::as_str).unwrap_or(key);
    for r in dst {
        let k = r.get(dkey).map(|v| v.to_string()).ok_or(format!("destination row missing `{dkey}`"))?;
        dmap.insert(k, r);
    }
    let (mut ins, mut upd, mut same) = (vec![], vec![], 0usize);
    let mut seen = std::collections::BTreeSet::new();
    for (n, r) in src.iter().enumerate() {
        let k = r.get(key).map(|v| v.to_string()).ok_or(format!("warehouse row {n} missing `{key}`"))?;
        if !seen.insert(k.clone()) {
            return Err(format!("duplicate key {k} in warehouse rows: fix the model before syncing"));
        }
        let mut mapped = Map::new();
        for (col, field) in mapping {
            let field = field.as_str().ok_or("mapping values must be strings")?;
            mapped.insert(field.to_string(), r.get(col).cloned().unwrap_or(Value::Null));
        }
        match dmap.get(&k) {
            None => ins.push(Value::Object(mapped)),
            Some(d) => {
                let mut changes = Map::new();
                for (f, v) in &mapped {
                    let old = d.get(f).cloned().unwrap_or(Value::Null);
                    if &old != v {
                        changes.insert(f.clone(), json!({"from": old, "to": v}));
                    }
                }
                if changes.is_empty() {
                    same += 1;
                } else {
                    upd.push(json!({"key": serde_json::from_str::<Value>(&k).unwrap(), "changes": changes}));
                }
            }
        }
    }
    let dels: Vec<Value> = if delete_missing {
        dmap.keys().filter(|k| !seen.contains(*k)).map(|k| serde_json::from_str(k).unwrap()).collect()
    } else {
        vec![]
    };
    Ok(json!({
        "summary": {"insert": ins.len(), "update": upd.len(), "unchanged": same, "delete": dels.len()},
        "api_calls_saved": same,
        "inserts": ins, "updates": upd, "deletes": dels,
    }))
}

#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn plans_insert_update_noop_delete() {
        let i = json!({"key":"id","mapping":{"id":"external_id","arr":"ARR__c"},"delete_missing":true,
            "warehouse_rows":[{"id":1,"arr":100},{"id":2,"arr":250},{"id":3,"arr":9}],
            "destination_rows":[{"external_id":1,"ARR__c":100},{"external_id":2,"ARR__c":200},{"external_id":9,"ARR__c":1}]});
        let r = plan_op(&i).unwrap();
        assert_eq!(r["summary"], json!({"insert":1,"update":1,"unchanged":1,"delete":1}));
        assert_eq!(r["updates"][0]["changes"]["ARR__c"]["to"], 250);
    }
    #[test]
    fn duplicate_keys_fail() {
        let i = json!({"key":"id","mapping":{"id":"id"},"warehouse_rows":[{"id":1},{"id":1}],"destination_rows":[]});
        assert!(plan_op(&i).is_err());
    }
}
