# Phase 5: Hybrid Runtime -- Celer as a Drop-In Uvicorn Replacement

## Context

Phases 1-4 built the core compiler (parser, type inference, LLVM codegen), framework adapters (FastAPI, Flask, Django), an HTTP server, cloud compilation service, and benchmarks achieving 15-37x speedups on pure compute/JSON workloads. But these only work on simple typed functions with no I/O, no async, no dependencies.

The user runs a **production FastAPI app** on Uvicorn with heavy async (DB, Redis, httpx), AI workflows, Depends(), middleware, and complex handlers. The goal: make `celerate serve main:app` a **drop-in Uvicorn replacement** that routes hot-path functions (JSON serialization, data transforms, AI pre/post-processing) to native code while seamlessly falling back to CPython for everything else.

**Strategy**: Hybrid-first. Don't try to compile all of Python. Instead:
1. AOT-compile the hot paths that ARE compilable (pure compute, JSON returns)
2. Run the FULL unmodified FastAPI app in a persistent embedded CPython runtime
3. At the HTTP routing level, intercept native-eligible requests; everything else goes to Python

---

## Gap Analysis Summary

| Layer | Works Today | Missing for Production |
|-------|------------|----------------------|
| Parser | Simple typed functions, literals, arithmetic | f-strings, keyword args, try/except, await, comprehensions |
| Type Inference | Scalars, dicts, lists | Optional, Union, method calls, async types |
| Compilability | Pure compute/JSON functions | Correctly classifies async/await/try/except as not-compilable |
| Codegen | Scalar + JSON returns via snprintf | **Global buffers are NOT thread-safe** (data races under concurrency) |
| HTTP Server | Path params, method matching | No request body, query params, headers, cookies |
| Runtime Fallback | Re-executes source, i64-only args | No persistent state, can't pass complex types |
| ASGI | None | Uvicorn speaks ASGI; Celer speaks raw TCP |

---

## Architecture

```
                        incoming HTTP request
                               |
                        +------v------+
                        | Celer Hybrid|
                        |   Server    |
                        +------+------+
                               |
                    +----------+----------+
                    |                     |
              route compiled?        route NOT compiled
                    |                     |
             +------v------+      +------v------+
             | Native AOT  |      | ASGI Bridge |
             | (LLVM, 0    |      | (persistent |
             |  Python)    |      |  CPython)   |
             +-------------+      +-------------+
                                        |
                                  Full FastAPI app:
                                  Depends(), middleware,
                                  async DB, exception
                                  handlers, Pydantic...
```

The ASGI bridge hosts the real FastAPI app exactly as Uvicorn would. Native routes bypass Python entirely. The user's app behavior is 100% identical -- just faster on compilable routes.

---

## Stream A: Persistent CPython Runtime (`celer-pyhost`)

**New crate.** Manages a long-lived Python interpreter that imports the user's actual FastAPI/ASGI app.

### Structure
```
crates/celer-pyhost/src/
  lib.rs        -- re-exports
  host.rs       -- PythonHost: boots interpreter, imports module, holds app ref
  asgi.rs       -- AsgiDispatcher: forwards requests through Python ASGI app
  scope.rs      -- Builds ASGI scope dicts from HTTP request data
  error.rs      -- PyHostError
```

### Key Design

**PythonHost** initializes once at startup:
- Adds module directory to `sys.path`
- Imports the user's module (`import main`)
- Grabs reference to the ASGI app object (`main.app`)
- Creates a dedicated asyncio event loop on a background thread
- All subsequent ASGI dispatches use `asyncio.run_coroutine_threadsafe()` to submit work

**AsgiDispatcher** handles per-request dispatch:
- Converts HTTP request -> ASGI scope dict (type="http", method, path, query_string, headers, server)
- Creates `receive` callable that yields `{"type": "http.request", "body": <bytes>}`
- Creates `send` callable that collects `http.response.start` + `http.response.body`
- Calls `await app(scope, receive, send)` via the persistent event loop
- Returns `AsgiResponse { status, headers, body }`

**GIL strategy**: All Python calls happen within `tokio::task::spawn_blocking` to avoid blocking the async runtime. The dedicated asyncio event loop thread handles async Python work. This mirrors how Uvicorn's workers operate.

### Files to create
- `crates/celer-pyhost/Cargo.toml` (deps: pyo3, tokio, bytes, thiserror, tracing)
- `crates/celer-pyhost/src/{lib,host,asgi,scope,error}.rs`

### Files to modify
- `Cargo.toml` (workspace) -- add member + dep

---

## Stream B: Hybrid Server + Smart Router

**Modifies `celer-server`.** The server gains an ASGI fallback path alongside native dispatch.

### Key Types

```rust
// router.rs
pub enum RouteDisposition {
    Native(CompiledRoute),  // AOT-compiled, bypass Python
    Asgi,                   // Forward to Python ASGI app
}

impl Router {
    pub fn match_hybrid(&self, method: &str, path: &str)
        -> (RouteDisposition, HashMap<String, String>);
}
```

```rust
// server.rs
pub struct HybridServer {
    config: ServerConfig,
    router: Arc<Router>,
    native: Arc<NativeModule>,
    asgi: Arc<AsgiDispatcher>,
}
```

### Request Flow

```rust
async fn handle_request(req, router, native, asgi) {
    let (disposition, params) = router.match_hybrid(method, path);
    match disposition {
        Native(route) => {
            // Existing: extract path params -> call native -> JSON response
            handle_native(route, params, &native)
        }
        Asgi => {
            // NEW: collect body + headers -> forward to Python
            let headers = extract_headers(&req);
            let body = collect_body(req.into_body()).await;
            let resp = asgi.dispatch(method, path, query, &headers, body).await;
            asgi_response_to_hyper(resp)
        }
    }
}
```

### Files to create
- `crates/celer-server/src/body.rs` -- request body collection from hyper Incoming
- `crates/celer-server/src/query.rs` -- query string parsing

### Files to modify
- `crates/celer-server/src/server.rs` -- add `HybridServer` (keep `CelerServer` for backward compat)
- `crates/celer-server/src/router.rs` -- add `RouteDisposition`, `match_hybrid`, `QueryParam`
- `crates/celer-server/src/request.rs` -- add header extraction, query param conversion
- `crates/celer-server/src/response.rs` -- add `asgi_response_to_hyper`
- `crates/celer-server/src/lib.rs` -- export new modules
- `crates/celer-server/src/error.rs` -- new variants
- `crates/celer-server/Cargo.toml` -- add celer-pyhost dep

---

## Stream C: Thread-Safe Codegen

**Fixes a correctness bug.** The JSON codegen uses global buffers that cause data races under concurrent requests.

### Problem (in `crates/celer-codegen/src/emit_function.rs`)
```rust
// Line ~256: GLOBAL buffer shared across all threads
let buf_global = ctx.module.add_global(buf_ty, None, &buf_name);
```

Two concurrent requests calling the same handler will race on this buffer, producing corrupted JSON.

### Fix: Thread-Local Storage

Replace global buffers with TLS globals so each OS thread gets its own buffer:
```rust
buf_global.set_thread_local_mode(Some(ThreadLocalMode::GeneralDynamic));
```

Same for string conversion buffers (`str_buf_*`, `concat_buf_*`) elsewhere in the codegen.

### Also: Increase buffer size
The current 256-byte hard limit silently truncates responses. Increase `SNPRINTF_BUF_SIZE` to 4096 or 8192 for realistic JSON payloads.

### Files to modify
- `crates/celer-codegen/src/emit_function.rs` -- TLS for JSON output buffers
- `crates/celer-codegen/src/emit_expr.rs` -- TLS for string conversion/concat buffers (if global)

---

## Stream D: Parser Expansion

**Expands the parser** to recognize more Python constructs. Even if not compiled, better parsing means the compilability analyzer can correctly classify handlers instead of erroring.

### HIR Additions

**Expressions** (`crates/celer-hir/src/expr.rs`):
- `Await { value, ty }` -- `await some_coro()`
- `FString { parts, ty }` -- `f"hello {name}"`
- `ListComp { element, generators, ty }` -- `[x*2 for x in items]`
- `DictComp { key, value, generators, ty }` -- `{k: v for k, v in pairs}`

**Statements** (`crates/celer-hir/src/stmt.rs`):
- `Try { body, handlers, orelse, finalbody }` -- try/except/else/finally

**Call expressions** (`crates/celer-parser/src/convert_expr.rs`):
- Parse keyword arguments in function calls (currently silently dropped)
- This is critical: `Depends()`, `HTTPException(status_code=404)`, `Field(default=...)` all use kwargs

### Compilability Impact

The analyzer (`crates/celer-typeinfer/src/compilability.rs`) gains rules:
- `async def` functions -> `NotCompilable` (require Python runtime)
- `await` expressions -> `NotCompilable`
- `try/except` blocks -> `NotCompilable`
- f-strings -> `NotCompilable` (no native format string support)
- Comprehensions -> `NotCompilable`

This means the hybrid router correctly sends these to ASGI instead of attempting native dispatch.

### Files to modify
- `crates/celer-hir/src/expr.rs` -- add Await, FString, ListComp, DictComp variants
- `crates/celer-hir/src/stmt.rs` -- add Try variant, ExceptHandler struct
- `crates/celer-parser/src/convert_expr.rs` -- handle await, f-strings, comprehensions, keyword args
- `crates/celer-parser/src/convert_stmt.rs` -- handle try/except
- `crates/celer-typeinfer/src/compilability.rs` -- mark new constructs as not-compilable
- `crates/celer-typeinfer/src/engine.rs` -- basic type handling for new expression types

---

## Stream E: CLI Rewrite (`celerate serve` as Uvicorn replacement)

**Rewrites the serve command** to orchestrate the full hybrid startup.

### CLI Interface
```
celerate serve main:app --host 0.0.0.0 --port 8000 --workers 4 --no-native
```

New flags: `--workers` (thread count), `--log-level`, `--max-body-size`, `--no-native` (pure ASGI debug mode).

### Startup Sequence
```
$ celerate serve main:app --host 0.0.0.0 --port 8000

Celer v0.1.0 -- Hybrid AOT Python Server
Analyzing main.py...

Compilability report:
  [native]   root          -- static JSON
  [native]   get_item      -- dynamic JSON
  [asgi]     create_user   -- async, Depends(), DB access
  [asgi]     login         -- try/except, HTTPException

Detected framework: FastAPI
Compiled 2/4 handlers to native code

Route table:
  GET  /            -> root         [native]
  GET  /items/{id}  -> get_item     [native]
  POST /users       -> create_user  [asgi]
  POST /login       -> login        [asgi]

Starting Python runtime...
  Imported main:app (FastAPI)
  ASGI fallback ready

Listening on http://0.0.0.0:8000
```

### Startup Pipeline
1. Parse module, run type inference, compilability analysis
2. AOT-compile native-eligible handlers to shared library
3. Initialize `PythonHost` -- import user module, start asyncio loop
4. Build hybrid router (native routes + ASGI catch-all)
5. Start `HybridServer`

### Files to modify
- `crates/celer-cli/src/commands/serve.rs` -- full rewrite of `execute()`
- `crates/celer-cli/Cargo.toml` -- add celer-pyhost dep

---

## Implementation Order

```
Phase 5a (parallel, no dependencies):
  [Stream C] Thread-safe codegen fix        ~1 session
  [Stream D] Parser expansion               ~2 sessions
  [Stream A] celer-pyhost crate             ~2 sessions

Phase 5b (depends on 5a):
  [Stream B] Hybrid server + smart router   ~2 sessions (needs A)

Phase 5c (integration, depends on 5b):
  [Stream E] CLI rewrite + integration      ~1 session (needs A, B)
```

Streams C, D, and A are fully independent and will be executed in parallel via worktree-isolated agents. Stream B depends on A (needs AsgiDispatcher). Stream E wires everything together.

---

## Verification

1. `cargo test --workspace` -- all 159 existing tests pass + new tests
2. `cargo build --release` -- clean build
3. **Thread safety**: blast native routes with `wrk -t4 -c100` -- no corrupted JSON
4. **Hybrid routing**: FastAPI app with mixed native/async handlers:
   - `GET /health` -> native, sub-millisecond
   - `POST /users` with JSON body -> ASGI, full Depends() + DB
   - Middleware (CORS) applies correctly to ASGI routes
5. **Drop-in test**: same app, `uvicorn main:app` vs `celerate serve main:app` -- identical behavior
6. **Benchmark**: `celerate serve` vs `uvicorn` on native-eligible routes -- target 10x+ throughput
7. **Graceful degradation**: if compilation fails, ALL routes fall to ASGI -- app still works
8. **`--no-native` mode**: pure ASGI pass-through, verify correctness baseline
