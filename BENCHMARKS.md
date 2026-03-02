# Celer Benchmark History

Tracks AOT compilation performance against CPython across project milestones.

**Hardware**: Apple Silicon (results are relative, not absolute)
**Runner**: `celerate bench --warmup 100 --iterations 10000`

---

## Phase 1: Proof of Concept (2026-03-02)

First end-to-end compilation of Python handlers to native code via LLVM.

**Scope**: Static and dynamic dict-returning functions compiled to JSON-producing native code.

```
Workload                   Runner             Ops/sec     Avg (ns)    Speedup
---------------------------------------------------------------------------
json-serialize-static      cpython             113863         8783       1.0x
json-serialize-static      celer-aot          3227759          310      28.3x
json-serialize-dynamic     cpython             103938         9621       1.0x
json-serialize-dynamic     celer-aot          2672903          374      25.7x
```

### Target Code

```python
# Static: compile-time constant JSON string, zero runtime cost
def root() -> dict:
    return {"message": "hello"}

# Dynamic: snprintf with format string, single C call
def get_item(item_id: int) -> dict:
    return {"item_id": item_id, "name": "test"}
```

### Key Observations

- Static dicts compile to global constant strings (pointer + length store, no runtime work)
- Dynamic dicts compile to a single `snprintf` call with a pre-built format string
- Entire Python interpreter overhead eliminated: no bytecode dispatch, no GIL, no object allocation
- 25-28x speedup represents the ceiling for pure computation/serialization workloads
