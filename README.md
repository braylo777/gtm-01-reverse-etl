# gtm-01-reverse-etl

**GTM Engineering — Stage 1 of 12: Data Infrastructure & Reverse ETL**

Make the warehouse the single source of truth: resolve one customer across CRM, billing and product data, plan diff-only syncs back to the CRM, and run dbt-style models in the right order.

**Live demo:** https://gtm-01-reverse-etl.braylo777.workers.dev — the Rust core runs in your browser as WebAssembly. No server, no tracking.

## Why this matters

Every GTM workflow downstream (routing, scoring, AI emails, churn alerts) reads customer data. If the CRM says one thing, billing another and the product a third, every automation is wrong in a different way. A warehouse-native setup fixes this once: merge identities in SQL models, build one golden record, and push only the changed fields back into the tools the reps use.

## What it does

- Identity resolution: union-find over normalized email (gmail dots and +tags) and phone (US/CA 11-digit), transitive merges across sources
- Survivorship rules: per field, newest non-empty value wins; ties go to the higher-priority source; field-level lineage shows where each value came from
- Reverse-ETL diff sync: field mapping, insert / update / unchanged / optional delete, per-field from→to changes, duplicate keys fail the run
- dbt-style model DAG: run order, parallel layers, cycle and unknown-ref detection, downstream rebuild set for a changed model

## What it does not do (yet)

- No live Snowflake / BigQuery / Hightouch / Census connection: inputs are JSON rows
- Matching is deterministic (exact normalized keys); no fuzzy name or address matching
- No SQL parsing: model dependencies are declared, not extracted from ref() calls

## Operations

Every operation takes JSON in and gives JSON out. The CLI, the tests and the demo page all use the same entry point.

| Operation | What it does |
|---|---|
| `resolve_identities` | Merge records that share a normalized email or phone into one person, then build a golden record with field lineage. |
| `sync_plan` | Compare warehouse rows with CRM rows through a field mapping. Only changed fields get an API call. |
| `model_dag` | Order warehouse models, show which can run in parallel, and list what must rebuild when one model changes. |

## Run it

You need Rust (stable). No Node, no npm.

```bash
git clone https://github.com/braylo777/gtm-01-reverse-etl
cd gtm-01-reverse-etl
cargo test
cargo run -- ops
```

Try the first demo example from the command line:

```bash
python3 -c 'import json;print(json.dumps(json.load(open("web/examples.json"))["ops"][0]["input"]))' | cargo run -q -- resolve_identities
```

## Build and deploy the demo

```bash
./build.sh    # tests, then compiles the core to web/app.wasm
./deploy.sh   # serves web/ as a Cloudflare Workers static-assets site (needs a Cloudflare login)
```

`web/` is a plain static site (`index.html`, `app.js`, `app.wasm`). It has no build step, so any static host works.

## Layout

```
src/lib.rs        operation list and router
src/api.rs        JSON entry point + the 3 WASM exports (alloc, dealloc, run)
src/main.rs       CLI
src/*.rs          one module per capability, each with unit tests
tests/            integration tests (every demo example must pass)
web/              demo page; examples.json drives both the page and this README
```

## Design choices

- **Rust, few dependencies.** Only `serde_json`, pinned to an exact version with a committed `Cargo.lock`.
- **Deterministic.** Time, randomness and keys are inputs, so every result can be replayed in a test or an audit.
- **Fail closed.** Unknown input or missing fields give an error, never a silent default.

## Part of a series

This is one of 12 repos that work through a public Go-To-Market (GTM) Engineering roadmap: [12 stages by Suraj Sharma (@suraj_sharma14)](https://x.com/suraj_sharma14/status/2125552583452270692). The roadmap is the inspiration; the code here is original. All 12: [github.com/braylo777?q=gtm-](https://github.com/braylo777?tab=repositories&q=gtm-).

## Data and privacy

Every example uses synthetic data. Nothing calls a paid API, scrapes a site, identifies a real visitor or sends an email.

## License

MIT — see [LICENSE](LICENSE). Built by [Braylo SI](https://braylo.si).
