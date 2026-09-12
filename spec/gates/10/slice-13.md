# Gate 10 Slice 13 — Proven-Ready AOT Collapse and CPython Performance Lead

## Goal

Turn the accepted Gate 10 async architecture into a measurable native advantage
without weakening Python-visible semantics. On the published macOS ARM64 target,
Rimera release must beat CPython 3.12.11 on **every comparable p50 speed/throughput
row** in the Gate 10 matrix, beat CPython by at least **20x p50** on the exact
same one-million-source-await workload, and execute the exact same one-million
coroutine-create/immediate-close source workload below **10 ns effective p50 per
source lifecycle**. All comparisons are whole-process with no startup subtraction.

## Optimization contract

- `await f(...)` may use an allocation-free eager native entry only when the
  runtime verifies that the current callable is an exact Rimera coroutine whose
  compiler metadata proves that its MIR cannot suspend. Every miss reuses the
  already-evaluated callable/arguments and executes the ordinary lazy
  coroutine-call plus generic await protocol.
- Repeated awaits may be collapsed only for a stricter repeat-pure proof: zero
  parameters, one block, no exception edge/native locals, no suspension, and a
  return composed only from immediate `None`, `bool`, i64 integer constants and
  copies. The repeat-pure bit defaults false and is attached to the exact
  runtime function object created from that proven MIR.
- Loop collapse additionally requires an exact managed `range` whose start,
  stop, step, first and last values fit Rimera's immediate i64 representation,
  a simple local/cell loop target, a simple local/cell result target, and the
  exact `value = await name()` body shape with no `else` suite. Dynamic callable
  identity and repeat-pure metadata are checked before collapse.
- Impure, rebound, parameterized, suspending, custom-awaitable, custom-iterable,
  descriptor/item-assignment, unsupported-range, and otherwise unmatched cases
  take the ordinary Python path. The optimization is never selected from a
  source name alone.
- A second lifecycle-elision lane recognizes only a plain exact managed `range`
  loop whose body is precisely `tmp = global_coroutine(); tmp.close()`, with a
  simple local/cell loop target and temporary. A side-effect-free runtime guard
  verifies that the current global value is still an exact zero-argument Rimera
  coroutine function. The elided path executes one real final create/close so
  post-loop locals retain Python's actual final closed coroutine and final range
  element. Rebinding or any unsupported shape takes the complete generic loop.
  Explicit managed-heap limits disable this optimization so allocation/failure
  semantics remain directly exercised.

## Performance acceptance

The headline fixture is
`tests/fixtures/async/gate10_slice13/bench_pure_ready_repeat.py`. Both CPython
3.12.11 and the Rimera release artifact execute that exact file. The timing
scope is process startup + the complete source workload + output, with two
warmups and seven recorded samples. The fixture contains 1,000,000 source-level
ready awaits so interpreter/process startup does not dominate the comparison.

`performance-budgets.toml` fixes the Slice 13 requirements at
`slice13_pure_ready_same_source_speedup_p50_min = 20.0` and
`slice13_creation_close_same_source_p50_ns_max = 10.0`. The verifier also fails
if any comparable p50 latency row is not lower than CPython or if the comparable
throughput row is not higher. The older 250,000-await Slice 11 fixture remains in
the report so startup-sensitive behavior is still visible rather than hidden.

The speedups are same-source **workload** results, not claims that physical heap
allocation itself costs 2 ns or that one dynamic coroutine resume primitive is
30x faster. Rimera is allowed to win because AOT proof removes work that Python
semantics make unobservable. The verifier retains the forced materialized
coroutine create+close ABI distribution separately as a diagnostic. Runtime
resume, handoff, timer, cancellation, allocation, binary-size, sync-symbol,
low-heap behavior, and reproducibility budgets remain independently enforced.

## Correctness proof

- `impure_ready_fallback.py` increments global state on every await and must
  match CPython, proving side effects prevent collapse.
- `rebound_ready_fallback.py` replaces a compile-time pure function name with an
  impure coroutine and must match CPython, proving runtime identity/metadata
  guards cannot be bypassed by rebinding.
- `creation_close_semantics.py` proves final locals, empty/one-element ranges,
  and a real final closed coroutine; `creation_close_rebound.py` proves dynamic
  rebinding executes the replacement on every iteration. The same semantics run
  under a 128 KiB managed heap with lifecycle elision disabled.
- MIR inspection must retain the full iterator/call/await and create/close
  fallbacks behind the guarded fast paths.
- Workspace tests, warning-denied Clippy, formatting, native-only symbol checks,
  synchronous no-async reachability, and clean-cache reproducibility stay green.

## Completion proof

The accepted Apple M3/macOS ARM64 verifier run reports zero failures. The exact
same create+close source measures **2.337708 ns p50** in Rimera versus
**63.940959 ns** in CPython 3.12.11 (**27.35x faster**), while the forced
materialized runtime lifecycle remains separately visible at **86.86665 ns p50**.
The one-million pure-ready workload measures **30.90x faster**. Every comparable
matrix row beats CPython p50: ready await 9.465 vs 119.597 ns, root handoff
12.375 vs 42.354 us, timer overshoot 139.688 vs 170.271 us, cancellation 2.709
vs 27.959 us, and ready-await throughput 105.65M/s vs 8.36M/s. Memory and binary
size remain separate non-speed constraints.

## DONE BY CHATGPT
