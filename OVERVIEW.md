# Celer

**Native speed for Python backends. The performance runtime Python never had.**

---

## The Problem

Python is the language of AI labs, rapid prototyping, and backend development. FastAPI, Flask, and Django power a significant portion of the worlds production web services. Yet the pattern is always the same: teams choose Python for velocity, then rewrite in Go or Rust when scale demands it.

The cause is not Python's syntax or ecosystem. It is the CPython runtime itself. Every variable is a heap allocated object. Every operation requires type dispatch. Every import traverses a module graph at startup. For a web service handling thousands of requests per second, this overhead compounds into real latency and real infrastructure cost.

No existing tool solves this for backend Python specifically. JIT runtimes like PyPy require a full interpreter swap. AOT compilers like Codon diverge from Python semantics and target scientific computing. The backend developer has no good option today.

**Celer is that option.**

---

## What Celer Is

Celer is an AOT compiler and lightweight runtime for Python backends, built in Rust, targeting Flask, FastAPI, and Django. It analyzes your existing typed Python code, infers and verifies types across your request handlers and business logic, and compiles hot paths to native machine code via LLVM.

The result is a drop in replacement runtime that requires zero changes to application code.

```bash
celerate run main:app
```

That single command replaces uvicorn or gunicorn, compiles your application transparently, and serves native compiled request handlers with CPython fallback for anything that cannot be statically typed.

---

## Core Design Principles

**Zero code changes.** Celer does not require you to rewrite your application, add decorators, or learn a new API. If your code is typed, Celer compiles it. If it is not, Celer runs it normally.

**Selective compilation with transparent fallback.** Functions that can be fully typed and analyzed compile to native extensions. Functions that cannot fall back to CPython silently. You get maximum acceleration without breakage.

**Framework native.** Celer understands FastAPI's dependency injection model, Django's ORM patterns, and Flask's request context. It does not treat your framework as an obstacle.

**Open source first.** The compiler, runtime, and all framework adapters are fully open source. The community builds the ecosystem. A managed cloud compilation service funds continued research.

---

## Architecture

Celer is a Rust workspace with clean crate boundaries:

```
celer/
  crates/
    celer-parser/      # Python AST ingestion via RustPython parser
    celer-hir/         # Typed high level intermediate representation
    celer-typeinfer/   # Type inference engine
    celer-codegen/     # Native code generation via LLVM (inkwell)
    celer-runtime/     # Thin runtime shim and CPython bridge
    celer-cli/         # celerate CLI
    celer-fastapi/     # FastAPI adapter
    celer-flask/       # Flask adapter
    celer-django/      # Django adapter
```

The pipeline flows left to right. Source Python enters the parser, emerges as an HIR with resolved types, passes through the inference engine for specialization, and exits as LLVM IR compiled to a native shared library loaded by the runtime shim at startup.

---

## The Type Inference Engine

The central research contribution of Celer is its type inference engine targeting Python backend patterns specifically.

Modern FastAPI code is statically analyzable at a level that general Python is not. Pydantic models are struct definitions with known field types. Route handler signatures carry explicit annotations. The async IO layer is already optimized. The actual CPU bound work, parsing, validation, business logic, serialization, is narrow and typed.

Celer's inference engine exploits this. It treats Pydantic model definitions as ground truth type sources, propagates types through function call chains within a single request handler, and identifies specialization opportunities where dynamic dispatch can be replaced with direct native calls.

Functions that achieve full type coverage compile to native code. Functions that do not fall back gracefully.

---

## Target Frameworks

| Framework | Entry Point | Protocol | Adapter Status |
|-----------|-------------|----------|----------------|
| FastAPI | ASGI lifespan | ASGI | Phase 1 |
| Flask | WSGI callable | WSGI | Phase 2 |
| Django | WSGI / ASGI | Both | Phase 2 |

FastAPI is the initial target because its type annotation requirements align most naturally with Celer's compilation model. Flask and Django follow with adapters that instrument their respective request lifecycle hooks.

---

## The Meta Experiment

Celer is also a public experiment in frontier AI assisted compiler engineering. The project is being built in close collaboration with Anthropic's Claude models, documenting which problems models solve fluently, where they reach their limits, and how compiler research changes when AI is a primary engineering partner.

Every significant architectural decision, debugging session, and research breakthrough will be documented publicly. This creates a second artifact alongside the compiler itself: a detailed record of what AI can build when given a hard systems programming problem.

---

## Roadmap

**Phase 1: Proof of Concept**
Compile a single typed FastAPI route handler to native code and benchmark against CPython. Establish the parser, HIR, and codegen pipeline end to end.

**Phase 2: Type Inference Engine**
Build the inference engine with Pydantic model support. Achieve full type coverage for common FastAPI patterns. Implement transparent CPython fallback.

**Phase 3: Framework Adapters**
Ship the FastAPI adapter with production grade stability. Begin Flask and Django adapters. Open source the project with full documentation.

**Phase 4: Ecosystem**
Community contribution model for framework adapters. Managed cloud compilation service. Benchmark suite comparing Celer against Go and Rust equivalents for common backend workloads.

---

## Benchmarking

Celer includes a built-in benchmark suite that measures compiled handler performance against standard Python runners. Every compilation target is validated with real numbers, not claims.

### What We Measure

| Metric | Description |
|--------|-------------|
| Requests/sec | Throughput under sustained load (wrk/oha) |
| p50 / p95 / p99 latency | Tail latency distribution per handler |
| Cold start time | Time from process launch to first request served |
| Compilation time | Time to parse, infer, and codegen a module |
| Memory footprint | RSS under load vs CPython baseline |

### Comparison Targets

| Runner | Description |
|--------|-------------|
| uvicorn (CPython) | Standard ASGI server, the default baseline |
| gunicorn + uvicorn workers | Production multi-worker deployment |
| PyPy + uvicorn | JIT-compiled Python runtime |
| Celer (AOT compiled) | Native compiled handlers with CPython fallback |
| Go (net/http) | Reference implementation for "rewrite in Go" baseline |
| Rust (axum) | Reference implementation for "rewrite in Rust" baseline |

### Benchmark Workloads

Each benchmark uses an identical API surface implemented in every runner:

1. **JSON serialization** -- Return a Pydantic model as JSON. Pure compute, no IO. Isolates serialization overhead.
2. **Path parameter parsing** -- Extract typed parameters from URL paths. Measures routing and type coercion cost.
3. **Request body validation** -- Accept a POST with a Pydantic model. Measures deserialization and validation.
4. **Business logic** -- Fibonacci, string processing, list operations inside a handler. Measures raw compute in a request context.
5. **Database round-trip** -- Single query, serialize result. Measures end-to-end with IO (Phase 2+).

### Running Benchmarks

```bash
# Run the full suite against all runners
celerate bench --all

# Compare a single workload
celerate bench --workload json-serialize --compare uvicorn,pypy,celer

# Output formats
celerate bench --format table    # Terminal table (default)
celerate bench --format json     # Machine-readable
celerate bench --format markdown # For documentation
```

### Expected Output

```
Workload: json-serialize (1000 concurrent, 30s duration)
-------------------------------------------------------------
Runner              Req/s     p50     p95     p99     Memory
uvicorn (CPython)   12,400    4.2ms   8.1ms   15.3ms  85MB
PyPy + uvicorn      28,600    1.8ms   3.4ms   6.2ms   210MB
Celer (AOT)         89,200    0.6ms   1.1ms   2.0ms   42MB
Go (net/http)       95,100    0.5ms   1.0ms   1.8ms   28MB
Rust (axum)        112,000    0.4ms   0.8ms   1.4ms   18MB
-------------------------------------------------------------
Celer speedup vs CPython: 7.2x throughput, 7.0x p50 latency
```

*Note: Numbers above are projected targets, not measured results. Actual benchmarks will be published as each phase reaches stability.*

---

## Why Now

CPython 3.13 introduced experimental JIT support. The Python community is finally taking performance seriously. The tooling to build on top of, RustPython's parser, inkwell's LLVM bindings, mypy's type research, is mature enough that a small team can build something real.

The backend Python niche is underserved, commercially valuable, and technically tractable. Celer is the right project at the right moment.

---

*Celer. Derived from the Latin celerius. Faster.*
