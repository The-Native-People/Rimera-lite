# Gate 10 Slice 11 — Cross-Feature Composition, GC, Cancellation, and Performance Stress

## Goal

Prove that all Gate 10 pieces compose with Rimera's existing language/runtime
features under load, cancellation races, re-entrancy, and constrained memory.

## Integrated work

- Compose nested coroutines, generic awaitables, async iterators/comprehensions,
  async generators, async managers, classes/descriptors, imports, closures,
  exceptions, callbacks, and synchronous generators without special branches.
- Stress ready storms, timer storms, cancellation before/during/after wake,
  completion races, dropped roots, shutdown, re-entrant callbacks, deep chains,
  wide fan-out, and retained/dead cycles.
- Audit all suspension safepoints and shadow roots; prove live tasks remain live,
  dead task graphs collect, cleanup runs once, and low-heap failures are atomic.
- Run the Slice 1 benchmark suite in debug and release on the supported target
  matrix. Attribute regressions with allocation counters and profiles rather
  than relaxing budgets from an unexplained aggregate result.
- Verify fair progress as exposed by the chosen backend without claiming an
  ordering or cross-thread guarantee Python and Compio do not provide.

## Performance invariants

Composition must preserve one executor, direct nested awaits, no task-per-item,
zero steady-state adapter allocation, wake-driven idle behavior, and the fixed
latency, memory, throughput, and size budgets.

## Completion proof

Native public stress/differential fixtures pass repeatedly with deterministic
clocks and constrained heaps; race/model tests have no lost wake or duplicate
completion; benchmark records meet every fixed budget on supported targets.

## Closure evidence

The public `gate10_slice11` composition fixture matches CPython 3.12.11 across
repeated native runs and a 512 KiB managed heap while composing nested
coroutines, custom awaitables, async iteration/comprehensions, async generators,
async managers, descriptors/classes, imports, closures/callbacks, synchronous
generators, exceptions, deep await chains, live suspended frames, dead cycles,
and manually injected cancellation. The separate source-level re-entrant-root
fixture proves a nested executor is rejected without poisoning the outer or next
root. Async-runtime model tests cover cancellation coalescing, wake/completion
storms, timer cancel/drop/shutdown storms, stale generations, re-entrant polls,
shutdown during resume, exactly-once cleanup, no hidden progress queue, and many
Compio roots plus a timer; the real Compio steady-poll path performs zero Rimera
adapter allocations.

`verify_gate10.py` now requires macOS arm64 and CPython 3.12.11, rebuilds every
measured artifact from a fresh `.rimera` cache, compares executable/object/
manifest hashes, verifies synchronous `auto` and explicit `compio` are byte
identical, and enforces the frozen Slice 1 budgets. The accepted release record
has no failures: creation p95 174.5 ns, source direct-await p95 564.2 ns,
ready-root handoff p95 12.87 us, 1 ms timer overshoot p95 142.0 us,
cancellation delivery p95 3.29 us, retained idle memory 1,039.2 bytes/task,
ready-await throughput p50 1.83 million/s, and async release size delta 85,376
bytes. Debug and release executable/object/manifest rebuilds are reproducible.

## DONE BY CHATGPT
