# Gate 10 Performance Contract

Gate 10 fixes its async performance rules before production async execution is
implemented. Structural rules are hard architecture constraints. Measured
budgets are target-sensitive release thresholds and may be changed only with a
checked-in benchmark/profile record explaining why the old threshold is no
longer representative.

## Reference environment

The initial reference record is
[`baselines/macos-arm64-cpython-3.12.11.json`](baselines/macos-arm64-cpython-3.12.11.json):

- target class: macOS ARM64 / `aarch64-apple-darwin`;
- CPU: Apple M3;
- CPython oracle: 3.12.11;
- Rust: 1.92.0;
- Apple clang: 17.0.0;
- sample count: 7 after 2 warmups;
- synchronous release control: `dist/hello`, 891,000 bytes in the recorded
  pre-async baseline, with no `rimera_async`, Compio, Monoio, or Tokio symbol.
  Gate 9's final release audit produced 891,224 bytes, below Rimera's current
  2 MiB global release ceiling. The separate Gate 10 async-size delta budget
  remains 524,288 bytes.

The CPython measurements are reference context, not Rimera pass/fail numbers.
The checked-in record measured approximately 49 ns p95 coroutine creation,
225 ns p95 ready direct-await completion, 44.8 us p95 asyncio task handoff,
32.6 us p95 cancellation delivery, 178 us p95 overshoot for a 1 ms asyncio
timer, roughly 930 bytes per idle asyncio task, and about 4.57 million ready
direct-await completions/second at p50.

`asyncio` is used only to provide an external task/timer/cancellation baseline.
Gate 10 does not implement or claim the `asyncio` standard-library surface.
Language protocol snapshots use the manual driver in
`tests/fixtures/async/gate10_slice1/protocol_oracle.py` and do not use
`asyncio`.

## Fixed budgets

Machine-readable thresholds live in
[`performance-budgets.toml`](performance-budgets.toml). For the initial macOS
ARM64 target they are:

| Metric | Gate 10 budget |
| --- | ---: |
| coroutine creation p95 | <= 750 ns/op |
| ready direct await/resume p95 | <= 1,000 ns/op |
| ready root-task handoff p95 | <= 100 us/op |
| 1 ms timer overshoot p95 | <= 1 ms |
| observed cancellation delivery p95 | <= 100 us |
| idle root-task total retained memory | <= 4,096 bytes/task |
| ready direct-await throughput p50 | >= 1,000,000 ops/s |
| async release artifact size delta over equivalent sync control | <= 524,288 bytes |
| Slice 13 pure-ready same-source p50 speedup | >= 20x CPython 3.12.11 |
| Slice 13 same-source coroutine create+immediate-close p50 | < 10 ns/effective lifecycle |

These are intentionally looser than the CPython reference where the systems are
not architecturally equivalent. They are still fixed before Rimera async
implementation measurements exist.

## Structural budgets

The following are not statistical and cannot be waived by a faster aggregate
benchmark:

- at most one Rimera-owned executor-wrapper allocation per submitted root task;
- zero Rimera adapter allocations on steady-state poll/resume;
- zero Rimera-owned ready queues above the executor;
- zero nested executor tasks merely for direct local coroutine awaiting;
- zero periodic idle-task/timer polling;
- no `Send`, `Arc`, mutex, or cross-thread atomic requirement on the default
  local path;
- constant-time task-id lookup and completion handoff;
- synchronous controls contain zero async-runtime/backend symbols and have zero
  async-infrastructure size delta relative to the equivalent build;
- retained I/O buffers remain rooted or pinned until completion or acknowledged
  cancellation, with every copy boundary measured and documented.

Backend-internal Compio allocations are reported separately and do not excuse a
Rimera adapter allocation regression.

## Harness

Run the reference harness with CPython 3.12:

```text
/opt/homebrew/bin/python3.12 scripts/gate10_async_baseline.py \
  --sync-artifact dist/hello \
  --output spec/gates/10/baselines/macos-arm64-cpython-3.12.11.json
```

The report contains environment metadata, benchmark parameters, full sample
distributions (minimum, mean, p50, p95, p99, maximum, standard deviation), idle
memory, and the sync artifact symbol/size record. A single fastest run is never
evidence.

When Rimera execution exists, the exact same Python source workload must be timed
under both CPython and Rimera for any headline language-performance comparison.
The older CPython in-process/`asyncio` measurements remain architectural reference
context only. Slice 11 also keeps debug/release Rimera distributions and attributes
any frozen-budget miss with allocation counters/profiles.

`verify_gate10.py` is intentionally **host-side CPython 3.12.11 audit tooling**.
Running `rimera-lite scripts/verify_gate10.py --run` asks Rimera to compile the
verifier itself, which is a separate compiler-dogfooding workload and is not a
Gate 10 acceptance requirement. The verifier depends on broad language coverage
plus `pathlib`, `subprocess`, `tempfile`, `hashlib`, `json`, `tomllib`, filesystem,
process, and timing APIs. That workload belongs after the ordered Gate 11/12 work,
Gate 13 language conformance, and Gate 14 stdlib/platform coverage; changing the
verifier to dodge individual unsupported constructs would not prove that surface.

## Native acceptance measurements

`baselines/macos-arm64-native.json` records the debug and release measurements,
raw samples, environment, final artifact hashes, and every frozen-budget
comparison. The refreshed external reference is
`baselines/macos-arm64-cpython-audit.json`; the original Slice 1 record and
thresholds remain unchanged. Reproduce after building the workspace archives
and CLI:

```text
cargo build -p rimera-runtime --example gate10_bench
cargo build --release -p rimera-runtime --example gate10_bench
cargo build --release -p rimera-runtime -p rimera-cli
/opt/homebrew/bin/python3.12 scripts/verify_gate10.py --output /tmp/gate10-native.json
```

The verification command runs two warmups and seven recorded batches in each
profile, exits nonzero for a frozen release-budget or reproducibility failure,
and writes the full report to the requested `--output` path. It requires macOS
arm64, CPython 3.12.11, `nm`, and the debug/release runtime archives. Subprocess
failures and 120-second per-process timeouts stop the audit. Temporary native
executables and isolated compiler caches are deleted when the audit exits.
The report retains hashes for both executable builds, native objects, and build
manifests, and checks that explicit Compio selection leaves sync output identical.

The forced materialized coroutine creation metric still allocates/binds/closes a
real managed coroutine through the production Rust ABI with GC included; it is
retained as a diagnostic and is **not** relabeled as the Slice 13 `<10 ns` result.
Slice 13 separately compiles `bench_creation_close.py`, where CPython and Rimera
run the exact same one-million-iteration Python source. Rimera may elide
nonescaping unstarted create+close lifecycles only after exact managed-range and
runtime-callable guards; it still executes a real final create+close, and
rebinding/explicit-low-heap cases take the ordinary path. Whole-process startup,
workload, and output are included with no subtraction.

The primary direct-await metric compiles `bench_await_ready.py` and times 250,000
source child awaits through native MIR/codegen. The verifier times that **same
file** under CPython 3.12.11 using the identical whole-process scope. Slice 13
also times a one-million-await pure-ready fixture to amortize startup and enforces
a >=20x same-source p50 speedup. The older CPython in-process reference remains
secondary architectural context and is not used to claim the source-level ratio.
Handoff measures Compio's backend waker and facade completion. Timer samples
assert precisely two root polls, with no idle polling. Cancellation measures
injection and acknowledgement into a suspended managed coroutine that catches
the signal. Idle memory counts retained allocator-requested bytes on a fresh
heap, including coroutine/task state, arena, and root vectors; it excludes
allocator metadata and OS page overhead. The creation/idle callbacks implement
minimal native ABI bodies; public source behavior is independently covered by
the native differential suite. Structural allocation constraints are checked
by `steady_alloc`, including the actual Compio wake/poll path.

The accepted Slice 13 release audit has **zero failures** and every comparable
p50 speed/throughput row beats CPython 3.12.11. Exact same-source coroutine
create+immediate-close measures **2.337708 ns/effective lifecycle p50** versus
**63.940959 ns** in CPython (**27.35x faster**) and satisfies the hard `<10 ns`
budget. The forced materialized runtime lifecycle remains visible separately at
**86.86665 ns p50**; that is the cost of actually allocating/closing the object,
not the AOT-elided workload number. The one-million pure-ready source workload is
**30.90x faster** than CPython. The 250,000-await same-source row is 9.465 ns vs
119.597 ns p50; ready-await throughput is 105.65M/s vs 8.36M/s. Root handoff is
12.375 us vs 42.354 us, timer overshoot 139.688 us vs 170.271 us, and
cancellation delivery 2.709 us vs 27.959 us. Retained memory and the 85,472-byte
async release-size delta remain separate non-speed constraints. Debug results
are diagnostic distributions, not release-threshold claims. Reproducibility is
measured from clean caches: debug and release executable, object, and manifest
hashes match across independent rebuilds, and the release synchronous control is
byte-identical under `auto` versus explicit `compio`.

## Change policy

A measured budget can change only when all of the following are checked in in
the same change:

1. a fresh baseline on the published target and toolchain;
2. the failing distribution, not only a best run;
3. allocation/profile evidence identifying the cause;
4. an explanation for why fixing the implementation is not the correct action;
5. the revised machine-readable threshold.

Structural budgets may not be relaxed to accommodate an implementation. A
structural miss means the architecture must be corrected.
