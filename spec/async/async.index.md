# Async runtime design note

**Status:** Gate 10 is closed through Slice 13 on macOS ARM64 with Compio. The
backend-neutral one-root executor facade, deterministic wake/timer/cancellation
transport, managed buffer lifetime, generic awaitables, async
iteration/comprehensions, async generators/finalization, `async with`, concrete
Compio integration, cross-feature GC/cancellation stress, fixed performance
budgets, clean-build reproducibility, and Slice 13's guarded AOT performance
qualification are accepted within Gate 10's boundary. Every comparable checked-in
p50 speed/throughput row beats CPython 3.12.11; create+immediate-close is below
10 ns effective p50 on the exact same source while forced materialization remains
reported separately.
The active compatibility work has advanced to Gate 11 dynamic compilation.

The authoritative design and slice order live in
[`../gates/10/README.md`](../gates/10/README.md).

Rimera does not implement a second scheduler. Python-visible coroutine state,
frames, suspension, exceptions, cleanup, task identity, and GC remain owned by
`rimera-runtime`. The `rimera-async-runtime` crate is only the narrow executor
facade for opaque task submission/completion, wakeups, timers, and cancellation
transport. It may not become a second Python runtime or scheduler. Compio is the
accepted first backend adapter and owns the actual ready queue, polling, wakers,
timers, and platform I/O for the current local execution path.

The dependency direction is:

```text
rimera-runtime -> rimera-async-runtime -> rimera-abi
                                      \
                                       -> Compio adapter
```

The async-runtime crate may not inspect `RValue` or Python frame/exception
layouts and may not maintain a mirror ready queue. Direct local coroutine awaits
stay inside one submitted root task rather than creating a backend task per
`await`.

Compio is the first accepted backend adapter. Monoio and Tokio remain future
adapters to the same facade, not separate Python semantic implementations.

The current CLI contract is `--async <auto|compio|monoio|tokio>` with the
`tool.rimera.async` project setting underneath it and CLI precedence. `auto`
selects Compio when source-level async root execution is reachable. `monoio` and
`tokio` are recognized but currently fail with a stable no-artifact unavailable-
backend diagnostic. Synchronous source does not link or advertise a backend.
