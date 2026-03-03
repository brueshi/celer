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

---

## Phase 2: Type Inference + Control Flow (2026-03-03)

Added arithmetic, comparisons, while loops, if/else branching, and cross-function calls. Functions that return scalars now use direct native calling conventions instead of JSON output params.

**Scope**: All Phase 1 workloads plus fibonacci (pure compute) and business logic (branching + inter-function calls).

```
Workload                   Runner             Ops/sec     Avg (ns)    Speedup
---------------------------------------------------------------------------
json-serialize-static      cpython             127927         7817       1.0x
json-serialize-static      celer-aot          5870152          170      45.9x
json-serialize-dynamic     cpython             120245         8316       1.0x
json-serialize-dynamic     celer-aot          2981207          335      24.8x
fibonacci                  cpython             165568         6040       1.0x
fibonacci                  celer-aot          4711453          212      28.5x
business-logic             cpython             119119         8395       1.0x
business-logic             celer-aot          2774028          360      23.3x
```

### Target Code

```python
# Fibonacci: while loop, arithmetic, comparison
def fib(n: int) -> int:
    a, b, i = 0, 1, 0
    while i < n:
        t = a + b
        a = b
        b = t
        i = i + 1
    return a

# Business logic: if/else, cross-function call, floor division
def apply_discount(price: int, threshold: int) -> int:
    if price > threshold:
        return price * 90 // 100
    return price

def calculate_price(base_price: int) -> dict:
    final_price = apply_discount(base_price, 50)
    return {"price": final_price, "currency": "USD"}
```

### Key Observations

- Fibonacci compiles to a tight native loop with `icmp`/`add`/`br` -- no interpreter overhead per iteration
- Business logic demonstrates compiled cross-function calls (`calculate_price` calls `apply_discount` directly via native `call` instruction)
- Static JSON jumped from 28x to 46x due to release-mode optimizations in the benchmark runner
- All four workloads sit in the 23-46x range, consistent with eliminating CPython's per-operation overhead on type-resolved code
