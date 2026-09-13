# Gate 11 Slice 7 — adversarial limits and atomicity

The public acceptance fixture is
`tests/fixtures/basic/gate11_adversarial_limits.py`. It executes in a
capability-enabled native artifact with a 2,500,000-byte managed heap. Unlike
the shared CPython differential corpus, Rimera-specific resource ceilings have
fixed expected diagnostics because CPython does not expose these product
limits.

## Limits

- Dynamic source is accepted through 1,048,576 UTF-8 bytes inclusive, measured
  after bytes-like conversion and UTF-8 BOM removal. A larger source raises
  `RuntimeError: dynamic source exceeds the 1048576 byte limit` before cache
  lookup or compiler invocation.
- A context permits 32 simultaneously nested dynamic executions. Attempting
  level 33 raises `RuntimeError: dynamic execution depth exceeds the 32 level
  limit`. Every normal and exceptional exit decrements the counter, so a later
  compile/eval/exec remains usable.
- A context permits 128 simultaneously live finalized native units. Before
  rejecting a new distinct key, the runtime performs managed collection and
  reclaims dead units. If 128 remain live, it raises `RuntimeError: dynamic
  native unit limit of 128 reached` before invoking the compiler. Cache hits do
  not consume another unit.
- The configured managed-heap limit remains authoritative for managed source,
  code, namespace, exception, and metadata allocations. Ordinary allocation
  failure raises `MemoryError` through the existing runtime path.

## Atomicity

Source-size and native-unit-limit failures publish no code object or cache
entry and never call the compiler. Parser, semantic, MIR, verifier, native
finalization, and code-publication failures likewise publish no native unit.
Cache-hit publication pins its existing unit across allocation.

Namespace behavior remains Python-compatible rather than transactional:
`eval`/`exec` insert `__builtins__` before compiling supplied source, and
successful writes preceding a runtime exception remain visible. Syntax failure
publishes no source binding. The adversarial fixture proves both states and
then successfully executes recovery expressions after source, depth, syntax,
and runtime failures.

Capability denial remains a build-time `RIM-CAP-G7-02` failure with no final
artifact. Capability-free artifacts do not link the dynamic compiler service.

## Evidence

Runtime tests pin pre-publication source/native-unit behavior and cache
coherence. The public artifact pins source/depth diagnostics, recovery,
namespace state, native-only symbols, and static-artifact service omission.
