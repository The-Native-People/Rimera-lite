# Gate 10 Slice 1 — Ownership, Oracle Matrix, Benchmark Harness, and Fixed Budgets

## Goal

Freeze the semantic and performance contract before async implementation begins.

## Integrated work

- Inventory Python 3.12 `async def`, coroutine, awaitable, asynchronous iterator,
  asynchronous generator, and asynchronous context-manager behavior, including
  invalid syntax, reuse, injection, shutdown, traceback, and warning cases.
- Assign syntax, HIR, MIR, runtime, executor, backend, CLI, and linker ownership.
  Record the one-way `rimera-runtime` -> `rimera-async-runtime` -> `rimera-abi`
  dependency and prohibit backend crates from inspecting Python values.
- Add deterministic CPython oracle fixtures and a manual native driver that can
  resume protocol objects without claiming the `asyncio` standard library.
- Establish benchmark fixtures for creation, direct await, task handoff, timers,
  cancellation, idle memory, allocations, throughput, and artifact size.
- Check in the measurement environment and numeric latency/memory budgets before
  implementation results are known. Separate hard structural invariants from
  platform-sensitive measured budgets.

## Performance invariants

The harness can detect per-poll allocation, periodic idle polling, lock/atomic
use on the local path, duplicate executor queues, and async symbols in a
synchronous control artifact. Benchmark samples include distributions and
environment metadata rather than a single fastest observation.

## Completion proof

The oracle matrix has a named owner and expected outcome for every Gate 10 form;
the benchmark harness produces reproducible baseline records; fixed budgets and
their change policy are reviewed; and no production async behavior is claimed.

## Preparatory evidence

The pre-implementation evidence package is now checked in at
[`oracle-matrix.md`](oracle-matrix.md), [`performance.md`](performance.md), and
[`performance-budgets.toml`](performance-budgets.toml). Deterministic CPython
3.12.11 snapshots live under `tests/fixtures/async/gate10_slice1/`; the manual
protocol oracle drives `send`/`throw`/`close` directly without `asyncio`.
`scripts/gate10_async_baseline.py` records distributions/environment/artifact
metadata, and `scripts/verify_gate10_slice1.py` verifies the snapshots, baseline,
and frozen thresholds.

Gate 9 is now closed. The frozen ownership/oracle/performance package is
reverified by `gate10_slice1_preimplementation_oracles_and_budgets_are_frozen`,
which passed against CPython 3.12.11 and the checked-in budgets. Slice 1 is
therefore complete; production coroutine/executor behavior remains owned by the
later slices.

## DONE BY CHATGPT
