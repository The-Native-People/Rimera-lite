# Gate 10 Slice 4 — Executor Facade, Task Identity, Local Scheduling, and Completions

## Goal

Introduce the minimal executor bridge needed to drive root coroutines while
leaving Python semantics in `rimera-runtime` and scheduling in the backend.

## Integrated work

- Add `rimera-async-runtime` below `rimera-runtime`, with opaque generational
  task ids, root-task submission, wake registration, cancellation request, and
  completion retrieval contracts documented in `rimera-abi`.
- Use one backend future wrapper per submitted root coroutine. The wrapper calls
  a narrow runtime resume callback and translates pending/ready status without
  inspecting `RValue`, exception, or frame layouts.
- Store task records in a constant-time generational arena and reject stale task
  ids deterministically. Keep task identity separate from a future public
  `asyncio.Task` implementation.
- Complete child-to-parent direct awaits inside the root task; submit no nested
  executor task merely because Python code contains another coroutine.
- Prove normal return, managed failure, external wakeup, cancellation request,
  dropped task, runtime shutdown, and executor re-entry boundaries.

## Performance invariants

The selected backend owns the only ready queue and poll loop. The local facade
uses no mirror queue, worker thread, general-purpose channel, `Arc`, mutex, or
cross-thread atomic in the steady path, and allocates no Rimera-owned storage
after task submission.

## Completion proof

ABI layout tests, stale-handle tests, allocation counters, wake/coalescing tests,
re-entrancy tests, constrained-heap native fixtures, and scheduler-instrumented
tests demonstrate one submitted executor task and one completion per root.

## Closure evidence

`rimera-runtime/tests/async_public.rs` constructs a native coroutine through the
public ABI and drives it through `AsyncRuntimeDriver::run_root` on a
caller-selected backend, proving exactly one submission, one completion, zero
Rimera wrapper allocations, and no surviving active task. The async-runtime
suite proves generational stale-handle rejection, wake coalescing, deterministic
re-entry failure, shutdown/drop exactly once, and absence of a hidden progress
queue. The synchronous native control is `nm`-scanned for zero `rimera_async`,
Compio, Monoio, or Tokio reachability. Warning-denied Clippy and formatting are
green for the touched crates.

## DONE BY CHATGPT
