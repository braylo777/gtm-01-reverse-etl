//! Identity resolution: link records that share a normalized email or phone, then pick
//! surviving field values (newest non-empty value wins; ties go to the higher-priority source).
use crate::util::*;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Lowercase; for gmail.com / googlemail.com drop dots and `+tag` in the local part.
pub fn norm_email(e: &str) -> Option<String> {
    let e = e.trim().to_lowercase();
    let (local, domain) = e.split_once('@')?;
    if local.is_empty() || !domain.contains('.') {
        return None;
    }
    let local = local.split('+').next().unwrap_or(local);
    let (local, domain) = if domain == "gmail.com" || domain == "googlemail.com" {
        (local.replace('.', ""), "gmail.com".to_string())
    } else {
        (local.to_string(), domain.to_string())
    };
    Some(format!("{local}@{domain}"))
}

/// Digits only; 11 digits starting with 1 become 10 (US/CA). Fewer than 7 digits is not a key.
pub fn norm_phone(p: &str) -> Option<String> {
    let mut d: String = p.chars().filter(char::is_ascii_digit).collect();
    if d.len() == 11 && d.starts_with('1') {
        d.remove(0);
    }
    (d.len() >= 7).then_some(d)
}

struct Dsu(Vec<usize>);
impl Dsu {
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut c = x;
        while self.0[c] != r {
            let n = self.0[c];
            self.0[c] = r;
            c = n;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            self.0[hi] = lo;
        }
    }
}

pub fn resolve_op(i: &Value) -> R<Value> {
    let recs = arr(i, "records")?;
    let priority: Vec<String> = i
        .get("source_priority")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let prio = |src: &str| priority.iter().position(|p| p == src).unwrap_or(priority.len());
    let mut dsu = Dsu((0..recs.len()).collect());
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut keys_per: Vec<Vec<String>> = Vec::new();
    for (n, r) in recs.iter().enumerate() {
        if !r.is_object() {
            return Err(format!("record {n} must be an object"));
        }
        let mut keys = vec![];
        if let Some(e) = r.get("email").and_then(Value::as_str).and_then(norm_email) {
            keys.push(format!("email:{e}"));
        }
        if let Some(p) = r.get("phone").and_then(Value::as_str).and_then(norm_phone) {
            keys.push(format!("phone:{p}"));
        }
        for k in &keys {
            match seen.get(k) {
                Some(&m) => dsu.union(m, n),
                None => {
                    seen.insert(k.clone(), n);
                }
            }
        }
        keys_per.push(keys);
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for n in 0..recs.len() {
        let r = dsu.find(n);
        groups.entry(r).or_default().push(n);
    }
    let mut out = vec![];
    for (g, members) in groups.values().enumerate() {
        // survivorship: per field, newest updated_at, then source priority.
        let mut best: BTreeMap<String, (String, usize, usize, Value)> = BTreeMap::new();
        for &m in members {
            let r = &recs[m];
            let ts = s_or(r, "updated_at", "").to_string();
            let src = s_or(r, "source", "");
            for (k, v) in r.as_object().unwrap() {
                if ["id", "source", "updated_at"].contains(&k.as_str()) || v.is_null() || v == "" {
                    continue;
                }
                let cand = (ts.clone(), usize::MAX - prio(src), usize::MAX - m, v.clone());
                let better = match best.get(k) {
                    None => true,
                    Some(b) => (&cand.0, cand.1, cand.2) > (&b.0, b.1, b.2),
                };
                if better {
                    best.insert(k.clone(), cand);
                }
            }
        }
        let mut golden = Map::new();
        let mut lineage = Map::new();
        for (k, (_, _, idx, v)) in best {
            let m = usize::MAX - idx;
            lineage.insert(k.clone(), json!(format!("{}:{}", s_or(&recs[m], "source", "?"), s_or(&recs[m], "id", "?"))));
            golden.insert(k, v);
        }
        let mut keys: Vec<String> = members.iter().flat_map(|&m| keys_per[m].clone()).collect();
        keys.sort();
        keys.dedup();
        out.push(json!({
            "person_id": format!("p{:04}", g + 1),
            "golden": golden,
            "field_source": lineage,
            "members": members.iter().map(|&m| json!(format!("{}:{}", s_or(&recs[m], "source", "?"), s_or(&recs[m], "id", "?")))).collect::<Vec<_>>(),
            "match_keys": keys,
        }));
    }
    let merged = recs.len() - out.len();
    Ok(json!({"input_records": recs.len(), "people": out.len(), "records_merged": merged, "golden_records": out}))
}

#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn gmail_and_phone_normalize() {
        assert_eq!(norm_email(" Jo.Ann+promo@GMAIL.com").unwrap(), "joann@gmail.com");
        assert_eq!(norm_email("a.b@acme.io").unwrap(), "a.b@acme.io");
        assert!(norm_email("nope").is_none());
        assert_eq!(norm_phone("+1 (555) 010-2000").unwrap(), "5550102000");
        assert!(norm_phone("123").is_none());
    }
    #[test]
    fn transitive_merge_and_survivorship() {
        let i = json!({"source_priority":["crm","billing"],"records":[
            {"id":"1","source":"crm","email":"jo@acme.io","title":"VP","updated_at":"2026-01-01"},
            {"id":"2","source":"billing","email":"JO@acme.io","phone":"555-010-2000","title":"CRO","updated_at":"2026-03-01"},
            {"id":"3","source":"product","phone":"+1 555 010 2000","plan":"pro","updated_at":"2026-02-01"},
            {"id":"4","source":"crm","email":"other@x.io"}]});
        let r = resolve_op(&i).unwrap();
        assert_eq!(r["people"], 2);
        let g = &r["golden_records"][0];
        assert_eq!(g["golden"]["title"], "CRO");
        assert_eq!(g["golden"]["plan"], "pro");
        assert_eq!(g["members"].as_array().unwrap().len(), 3);
    }
    #[test]
    fn tie_goes_to_priority_source() {
        let i = json!({"source_priority":["crm","billing"],"records":[
            {"id":"1","source":"billing","email":"a@b.io","name":"Bill","updated_at":"2026-01-01"},
            {"id":"2","source":"crm","email":"a@b.io","name":"Crm","updated_at":"2026-01-01"}]});
        assert_eq!(resolve_op(&i).unwrap()["golden_records"][0]["golden"]["name"], "Crm");
    }
}
