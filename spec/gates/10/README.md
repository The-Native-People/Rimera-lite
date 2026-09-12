# Gate 10 — Async, Await, and Asynchronous Protocols

Gate 10 implements Python 3.12 asynchronous language semantics over Rimera's
existing native call, exception, generator, cleanup, object, and GC machinery.
It also adds the smallest executor boundary required to run those semantics.
It may not add an async interpreter, duplicate Python object model, or a
Rimera scheduler layered on top of a Rust executor.

The accepted concrete executor backend is Compio. `rimera-runtime` owns
Python-visible coroutine state, exceptions, suspension, and GC. The new
`rimera-async-runtime` crate owns only task submission, wakeups, timers,
cancellation delivery, and completion transport. Compio performs the actual
polling and platform I/O. Future Monoio and Tokio adapters must implement this
same boundary instead of adding backend-specific Python semantics.

The gate proves language and object protocols independently of the `asyncio`
standard-library API. Public `asyncio` event loops, `asyncio.Task`, networking,
subprocesses, and framework compatibility belong to Gate 14 and later proof.

## Synchronized slice contract

- Execute slices in numeric order. Slice 1 freezes ownership, behavioral
  oracles, measurements, and performance budgets before implementation changes
  are judged. Later slices may refine future slice documents when evidence
  exposes a missing dependency, but may not weaken a completed contract.
- Each executable slice reaches syntax/HIR/sema/verified MIR/Cranelift, the
  documented Rust ABI/runtime, precise GC ownership, and a public CPython
  3.12 differential. An executor-only scaffold does not complete a slice.
- `rimera-runtime` may depend on `rimera-async-runtime`, which may depend on
  `rimera-abi`; the async crate may not depend back on the Python runtime or
  inspect `RValue` internals. Cross-boundary values are opaque handles and
  status/completion records with explicit ownership.
- There is exactly one scheduler. Compio owns ready queues, polling, wakers,
  timers, and I/O submission. Rimera tracks Python task identity and completion
  but never busy-polls, mirrors Compio's ready queue, or wraps each poll in a
  second scheduled task.
- Backend selection is static for one executable. There is no per-poll virtual
  dispatch, string lookup, backend switch, or general-purpose channel.
- A coroutine frame and executor task may allocate at creation. A steady-state
  poll/resume of an already-created task must perform no heap allocation in
  Rimera's adapter. The default local path must not require `Send`, `Arc`, a
  mutex, or cross-thread atomics merely to await another local coroutine.
- Timers and I/O are wake-driven. Cancellation is observed only at documented
  suspension points, resumes through normal exception completion, and runs
  every cleanup action exactly once.
- Synchronous programs do not link the async runtime or Compio. Native-symbol
  and artifact-size checks must prove zero async reachability for a synchronous
  control fixture.
- Focused checks run while implementing slices. The full workspace,
  warning-denied Clippy, formatting, release-size, reproducibility, and
  forbidden-symbol audits run once in Slice 12.

## Performance contract

Gate 10 does not promise speed through architecture prose. Slice 1 records a
reproducible baseline and fixes budgets before optimization results are known.
The benchmark suite measures coroutine construction, direct await/resume,
ready-task handoff, timer wakeup, cancellation, idle-task memory, allocation
count, throughput, and binary-size impact. Results record target, profile,
toolchain, backend version, CPU, and sample distribution; a single best run is
not evidence.

The following structural budgets are mandatory regardless of benchmark noise:

- one Rimera-owned executor-wrapper allocation per submitted root task, not per
  poll; backend-internal allocations are measured separately;
- zero Rimera adapter allocations on steady-state poll/resume;
- constant-time task-id lookup and completion handoff;
- no periodic polling for idle tasks or timers;
- no lock/atomic tax on the default single-thread local executor path;
- no async runtime symbols or size contribution in a synchronous control;
- rooted or pinned I/O buffers remain valid until completion, with every copy
  boundary documented and measured.

Latency and memory thresholds are committed by Slice 1 from the recorded
baseline and may change later only with an explained benchmark/profiling record,
never merely to make a regression pass.

## Progress ledger

- [x] Slice 1 — ownership, oracle matrix, benchmark harness, and fixed budgets
- [x] Slice 2 — async syntax, HIR/MIR suspension, resume, and injection contracts
- [x] Slice 3 — coroutine objects, lazy calls, direct `await`, and lifecycle
- [x] Slice 4 — executor facade, task identity, local scheduling, and completions
- [x] Slice 5 — timers, wakeups, cancellation delivery, and buffer ownership
- [x] Slice 6 — generic `__await__`, delegation, injected failures, and cleanup
- [x] Slice 7 — async iteration, `async for`, and async comprehensions
- [x] Slice 8 — async generators, `asend`, `athrow`, `aclose`, and finalization
- [x] Slice 9 — `async with`, partial entry, suppression, and suspended cleanup
- [x] Slice 10 — Compio adapter, static linking, CLI selection, and diagnostics
- [x] Slice 11 — cross-feature composition, GC, cancellation, and performance stress
- [x] Slice 12 — reproducibility, final audit, documentation, and gate closure
- [x] Slice 13 — proven-ready AOT collapse and CPython performance lead

## Slice order

1. [Ownership, oracle matrix, benchmark harness, and fixed budgets](slice-1.md)
2. [Async syntax, HIR/MIR suspension, resume, and injection contracts](slice-2.md)
3. [Coroutine objects, lazy calls, direct `await`, and lifecycle](slice-3.md)
4. [Executor facade, task identity, local scheduling, and completions](slice-4.md)
5. [Timers, wakeups, cancellation delivery, and buffer ownership](slice-5.md)
6. [Generic `__await__`, delegation, injected failures, and cleanup](slice-6.md)
7. [Async iteration, `async for`, and async comprehensions](slice-7.md)
8. [Async generators, `asend`, `athrow`, `aclose`, and finalization](slice-8.md)
9. [`async with`, partial entry, suppression, and suspended cleanup](slice-9.md)
10. [Compio adapter, static linking, CLI selection, and diagnostics](slice-10.md)
11. [Cross-feature composition, GC, cancellation, and performance stress](slice-11.md)
12. [Reproducibility, final audit, documentation, and gate closure](slice-12.md)
13. [Proven-ready AOT collapse and CPython performance lead](slice-13.md)

Gate 10 is closed through all thirteen slices on the published macOS ARM64 /
Compio matrix. Slice 13 adds a strict performance qualification: every comparable
p50 speed/throughput row in the checked-in matrix beats CPython 3.12.11, the
one-million pure-ready workload is at least 20x faster, and exact same-source
coroutine create+immediate-close is below 10 ns effective p50 while materialized
lifecycle cost remains reported separately. Async syntax/protocols,
lifecycle/cleanup, one-root execution, static backend selection, cross-feature
GC/cancellation stress, fixed performance budgets, clean-build reproducibility,
and native-only release proof remain accepted. The single active compatibility
item is Gate 11 Slice 1. Monoio/Tokio adapters and the `asyncio` standard library
remain later work and are not implied by Gate 10 closure.
