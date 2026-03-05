# Phase 4: Ecosystem -- Implementation Plan

## Context

Phases 1-3 delivered the core compiler pipeline (parser, type inference, LLVM codegen, runtime), framework adapters (FastAPI, Flask), HTTP server, and a benchmark suite with 8 workloads achieving 15-37x speedups over CPython. Phase 4 completes the ecosystem: cross-language benchmarks (Go/Rust comparison), a plugin system for framework adapters, a complete Django adapter, and a cloud compilation service.

---

## Stream A: Go/Rust Benchmark Equivalents

Hand-written Go and Rust implementations of all 8 benchmark workloads, integrated into the existing benchmark runner.

### Structure

```
benches/
  go/
    go.mod
    cmd/
      json_serialize_static/main.go
      json_serialize_dynamic/main.go
      fibonacci/main.go
      for_loop_sum/main.go
      business_logic/main.go
      http_path_param/main.go
      http_compute_endpoint/main.go
  rust/
    Cargo.toml                    # standalone project, NOT in workspace
    src/bin/
      json_serialize_static.rs
      json_serialize_dynamic.rs
      fibonacci.rs
      for_loop_sum.rs
      business_logic.rs
      http_path_param.rs
      http_compute_endpoint.rs
```

Kept outside the Rust workspace since Go is not a Cargo artifact, and the Rust benchmarks are standalone binaries.

### Binary Contract

Each binary accepts `--iterations N --warmup N`, prints one JSON line:
```json
{"iterations": 100000, "total_ns": 1234567}
```

### Runner Integration

Add `run_external()` to `crates/celer-bench/src/runner.rs`:
- Invokes Go/Rust binaries via `std::process::Command`
- Parses JSON output into `BenchResult`
- Runner names: "go", "rust"

### CLI Changes

Modify `crates/celer-cli/src/commands/bench.rs`:
- Add `--compare` flag: `cpython,celer,go,rust` (default: `cpython,celer`)
- Auto-build Go binaries with `go build` and Rust binaries with `cargo build --release`
- Look for binaries in `benches/go/cmd/*/` and `benches/rust/target/release/`

### Reporter Update

Modify `crates/celer-bench/src/reporter.rs`:
- Widen table for 4 runners
- Compute speedup vs CPython for all runners
- Add geometric mean summary row

### Files to create
- `benches/go/go.mod`
- `benches/go/cmd/{7 workload dirs}/main.go`
- `benches/rust/Cargo.toml`
- `benches/rust/src/bin/{7 workloads}.rs`

### Files to modify
- `crates/celer-bench/src/runner.rs` -- add `run_external()`
- `crates/celer-bench/src/reporter.rs` -- widen table, add summary
- `crates/celer-bench/Cargo.toml` -- add serde_json if missing
- `crates/celer-cli/src/commands/bench.rs` -- add `--compare` flag, external runner integration

---

## Stream B: Framework Adapter Plugin System

### New crate: `celer-adapter-core`

```
crates/celer-adapter-core/src/
  lib.rs          # re-exports
  route.rs        # HttpMethod, ParamSource, RouteParam, RouteInfo (moved from celer-fastapi)
  traits.rs       # FrameworkAdapter trait
  detect.rs       # Auto-detection: try adapters in priority order
  error.rs        # Shared adapter errors
```

### The trait

```rust
pub trait FrameworkAdapter {
    fn name(&self) -> &'static str;
    fn detect(&self, module: &Module) -> bool;
    fn extract_routes(&self, module: &Module) -> Result<Vec<RouteInfo>, Box<dyn std::error::Error>>;
}
```

### Shared types

Move `HttpMethod`, `ParamSource`, `RouteParam`, `RouteInfo` from `celer-fastapi/src/route.rs` into `celer-adapter-core/src/route.rs`. Include the `RouteInfo::new()` constructor and `extract_path_params()` utility.

### Adapter refactoring

- `celer-fastapi`: Remove duplicated types from `route.rs`, re-export from adapter-core, implement `FrameworkAdapter` for `FastApiAdapter`
- `celer-flask`: Same treatment. Keep Flask-specific `normalize_flask_path` logic in `route.rs` but use shared types

### Auto-detection in `celerate serve`

Modify `crates/celer-cli/src/commands/serve.rs`:
- Replace hardcoded `celer_fastapi::FastApiAdapter::extract_routes()` (line 72)
- Use `detect_framework()` with ordered adapter list: FastAPI -> Flask -> Django
- Convert from `celer_adapter_core::HttpMethod` instead of `celer_fastapi::HttpMethod` (lines 84-89)

### Files to create
- `crates/celer-adapter-core/Cargo.toml`
- `crates/celer-adapter-core/src/{lib,route,traits,detect,error}.rs`

### Files to modify
- `Cargo.toml` (workspace) -- add member + dep
- `crates/celer-fastapi/Cargo.toml` -- add adapter-core dep
- `crates/celer-fastapi/src/route.rs` -- remove types, re-export from adapter-core
- `crates/celer-fastapi/src/adapter.rs` -- implement trait
- `crates/celer-fastapi/src/lib.rs` -- re-export from adapter-core
- `crates/celer-flask/Cargo.toml` -- add adapter-core dep
- `crates/celer-flask/src/route.rs` -- use shared types
- `crates/celer-flask/src/adapter.rs` -- implement trait
- `crates/celer-flask/src/lib.rs` -- re-export from adapter-core
- `crates/celer-cli/Cargo.toml` -- add adapter-core dep
- `crates/celer-cli/src/commands/serve.rs` -- use auto-detection

---

## Stream C: Django Adapter (depends on B1)

### Structure

```
crates/celer-django/src/
  lib.rs           # FrameworkAdapter impl, re-exports
  error.rs         # DjangoError (expand existing)
  adapter.rs       # URL pattern extraction from urlpatterns
  patterns.rs      # Django path converter parsing (<int:id> -> {id})
```

### Design

Django uses `urlpatterns = [path('route/', view_func)]` not decorators. The HIR represents this as `Statement::Assign { target: "urlpatterns", value: Expression::List { elements: [Expression::Call { ... }] } }`.

The adapter:
1. `detect()`: scan for `Assign` with target `"urlpatterns"`
2. `extract_routes()`: parse each `path()` call in the list, match view name to `FunctionDef` in module
3. Path converters: `<int:id>` -> `{id}` with type mapping (Int, Str, Uuid)
4. Default to GET for all Django views (Django URL routing is method-agnostic; method dispatch happens in the view)

### Files to create
- `crates/celer-django/src/adapter.rs`
- `crates/celer-django/src/patterns.rs`

### Files to modify
- `crates/celer-django/Cargo.toml` -- add adapter-core dep
- `crates/celer-django/src/lib.rs` -- replace stub with full impl
- `crates/celer-django/src/error.rs` -- expand error variants

---

## Stream D: Cloud Compilation Service

### New crate: `celer-cloud`

```
crates/celer-cloud/src/
  lib.rs           # re-exports
  config.rs        # CloudConfig (port, max_source_size, job_ttl)
  server.rs        # hyper HTTP server setup (consistent with celer-server)
  handlers.rs      # POST /compile, GET /health, GET /status/:id, GET /download/:id
  job.rs           # Job struct, JobStatus enum, JobStore (DashMap)
  compiler.rs      # Async compilation wrapper using pipeline
  error.rs         # CloudError
```

### API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/compile` | POST | Submit Python source, returns job_id (202) |
| `/status/:job_id` | GET | Job status + compile time |
| `/download/:job_id` | GET | Download compiled .so/.dylib |

### Job Store

`Arc<DashMap<String, Job>>` for lock-free concurrent access. Background cleanup task removes expired jobs (default TTL: 1 hour).

### Security

- Source size limit (default 1 MB)
- Compilation timeout (30s via `tokio::time::timeout`)
- Input validation on module name
- No code execution -- pipeline only parses/infers/compiles

### CLI

Add `celerate cloud` subcommand:
```
celerate cloud [--host 0.0.0.0] [--port 9000] [--max-source-bytes 1048576]
```

### Files to create
- `crates/celer-cloud/Cargo.toml`
- `crates/celer-cloud/src/{lib,config,server,handlers,job,compiler,error}.rs`
- `crates/celer-cli/src/commands/cloud.rs`

### Files to modify
- `Cargo.toml` (workspace) -- add member + deps (dashmap, uuid)
- `crates/celer-cli/Cargo.toml` -- add celer-cloud dep
- `crates/celer-cli/src/commands/mod.rs` -- add Cloud variant
- `crates/celer-cli/src/main.rs` -- route Cloud command

---

## Implementation Order

Streams A, B, and D are fully independent and can execute in parallel. Stream C depends on B1 only.

```
Parallel execution:

[Stream A] Go/Rust benchmarks
  A1: Go binaries (8 workloads)
  A2: Rust binaries (8 workloads)        -- parallel with A1
  A3: run_external() in BenchRunner
  A4: CLI --compare flag
  A5: Reporter table update

[Stream B] Adapter plugin system
  B1: celer-adapter-core crate
  B2: Refactor celer-fastapi
  B3: Refactor celer-flask                -- parallel with B2
  B4: Update celerate serve

[Stream C] Django adapter (after B1)
  C1: patterns.rs
  C2: adapter.rs
  C3: FrameworkAdapter impl + tests

[Stream D] Cloud compilation service
  D1: celer-cloud crate skeleton
  D2: Job store
  D3: Compilation handler
  D4: HTTP server + endpoints
  D5: celerate cloud CLI command

Convergence:
  E1: Run full benchmark suite (all 4 runners)
  E2: Update BENCHMARKS.md with results
  E3: Update OVERVIEW.md Phase 4 status
```

---

## Verification

1. `cargo test --workspace` -- all existing 149+ tests pass, plus new tests
2. `cargo build --release` -- full workspace builds clean
3. `celerate bench --compare cpython,celer,go,rust` -- 4-runner comparison table
4. `celerate serve examples/fastapi_app:app` -- auto-detects FastAPI
5. Django test: create a Django-style module, `celerate serve django_app:app` -- auto-detects Django
6. `celerate cloud` -- starts cloud service, test with `curl -X POST /compile`
7. `cd benches/go && go build ./cmd/...` -- Go binaries build independently
8. `cd benches/rust && cargo build --release` -- Rust binaries build independently
