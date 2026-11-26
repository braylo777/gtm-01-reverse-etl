//! dbt-style model graph: topological run order (Kahn), parallel layers, cycle detection,
//! and the downstream models affected when one model changes.
use crate::util::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn dag_op(i: &Value) -> R<Value> {
    let models = arr(i, "models")?;
    let mut deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in models {
        let name = s(m, "name")?.to_string();
        let d = m.get("depends_on").and_then(Value::as_array).cloned().unwrap_or_default();
        let d: Vec<String> = d.iter().filter_map(|x| x.as_str().map(String::from)).collect();
        if deps.insert(name.clone(), d).is_some() {
            return Err(format!("model `{name}` is defined twice"));
        }
    }
    for (m, ds) in &deps {
        for d in ds {
            if !deps.contains_key(d) {
                return Err(format!("model `{m}` depends on unknown model `{d}`"));
            }
        }
    }
    let mut indeg: BTreeMap<&str, usize> = deps.iter().map(|(k, v)| (k.as_str(), v.len())).collect();
    let mut layers: Vec<Vec<String>> = vec![];
    let mut done = BTreeSet::new();
    loop {
        let ready: Vec<String> = indeg.iter().filter(|(k, &d)| d == 0 && !done.contains(**k)).map(|(k, _)| k.to_string()).collect();
        if ready.is_empty() {
            break;
        }
        for r in &ready {
            done.insert(r.clone());
        }
        for (m, ds) in &deps {
            let n = ds.iter().filter(|d| ready.contains(d)).count();
            if n > 0 {
                *indeg.get_mut(m.as_str()).unwrap() -= n;
            }
        }
        layers.push(ready);
    }
    if done.len() != deps.len() {
        let stuck: Vec<&String> = deps.keys().filter(|k| !done.contains(*k)).collect();
        return Err(format!("cycle detected among: {}", stuck.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
    }
    let mut affected = vec![];
    if let Some(ch) = i.get("changed").and_then(Value::as_str) {
        if !deps.contains_key(ch) {
            return Err(format!("changed model `{ch}` is unknown"));
        }
        let mut set = BTreeSet::from([ch.to_string()]);
        for layer in &layers {
            for m in layer {
                if deps[m].iter().any(|d| set.contains(d)) {
                    set.insert(m.clone());
                }
            }
        }
        for layer in &layers {
            affected.extend(layer.iter().filter(|m| set.contains(*m)).cloned());
        }
    }
    let order: Vec<&String> = layers.iter().flatten().collect();
    Ok(json!({"run_order": order, "layers": layers, "parallel_width": layers.iter().map(Vec::len).max().unwrap_or(0), "rebuild_if_changed": affected}))
}

#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn layers_and_downstream() {
        let i = json!({"changed":"stg_users","models":[
            {"name":"stg_users"},{"name":"stg_events"},
            {"name":"dim_accounts","depends_on":["stg_users"]},
            {"name":"fct_usage","depends_on":["stg_events","dim_accounts"]},
            {"name":"rpt_other","depends_on":["stg_events"]}]});
        let r = dag_op(&i).unwrap();
        assert_eq!(r["layers"][0], json!(["stg_events","stg_users"]));
        assert_eq!(r["rebuild_if_changed"], json!(["stg_users","dim_accounts","fct_usage"]));
    }
    #[test]
    fn cycle_and_unknown_fail() {
        assert!(dag_op(&json!({"models":[{"name":"a","depends_on":["b"]},{"name":"b","depends_on":["a"]}]})).unwrap_err().contains("cycle"));
        assert!(dag_op(&json!({"models":[{"name":"a","depends_on":["z"]}]})).is_err());
    }
}
