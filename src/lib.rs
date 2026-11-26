//! Stage 1 — Data Infrastructure & Reverse ETL.
//!
//! * `identity` — resolve one person across CRM, billing and product records (union-find on
//!   normalized email / phone) and build one golden record with survivorship rules.
//! * `sync`     — Hightouch/Census-style diff sync: warehouse rows vs destination rows give a
//!   plan of inserts, updates and no-ops, with a field mapping.
//! * `dag`      — dbt-style model graph: run order, layers and cycle detection.

mod api;
pub mod dag;
pub mod identity;
pub mod sync;
pub mod util;

pub use api::dispatch;
use serde_json::Value;

pub const OPS: &[&str] = &["resolve_identities", "sync_plan", "model_dag"];

pub fn handle(op: &str, i: &Value) -> Result<Value, String> {
    match op {
        "resolve_identities" => identity::resolve_op(i),
        "sync_plan" => sync::plan_op(i),
        "model_dag" => dag::dag_op(i),
        _ => Err(format!("unknown op `{op}`; known: {}", OPS.join(", "))),
    }
}
