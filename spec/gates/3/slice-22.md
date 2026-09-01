# Gate 3 Slice 22 — Minimal Proof for Newly Fixed Gaps

## Goal
Keep Gate 3 validation lightweight and focused on proving the implementation rather than building a large new test project.

## Proof strategy
Reuse the existing runtime unit tests, MIR/rooting tests, native-pipeline infrastructure, GC tests, no-artifact failures, and artifact scans. Add only compact regressions for behavior newly fixed by Slices 1–21.

## Completed minimal proof matrix
- `gate3_minimal_regressions_match_cpython_312` adds the previously uncovered compact cases: all three mutable set update methods, middle insertion into list/bytearray via an empty slice, and negative-step deletion for list/bytearray.
- `gate3_structured_exceptions_and_complex_match_cpython_312` covers representative typed failures, zero-imaginary complex equality/arithmetic, complex true division, and complex floor-division rejection.
- `gate3_numeric_hash_and_collision_semantics_match_cpython_312` covers the large int/float equality-hash key invariant plus callback/collision mutation safety.
- `gate3_constructor_and_builtin_subclass_semantics_match_cpython_312` covers `int`, `float`, and `complex` parser/conversion edges and typed memoryview-to-bytes conversion.
- `gate3_repr_ascii_and_format_semantics_match_cpython_312` covers recursive repr, Unicode repr/ascii edges, and nontrivial formatting.
- `gate3_round_and_float_divmod_semantics_match_cpython_312` covers nontrivial rounding behavior.
- `gate3_reversed_and_range_semantics_match_cpython_312` covers `reversed(bytes)`, reversed dictionaries/views, huge-range length overflow with valid truthiness, and range attributes/methods.
- `gate3_slice_and_memoryview_remaining_semantics_match_cpython_312` covers slice attributes/methods, typed/cross-format memoryview equality, conversion, and exporter-chain hash rejection.
- `gate3_small_builtin_type_surfaces_match_cpython_312` covers the small float/complex surface added by Slice 21.

## Existing proofs to preserve rather than duplicate
- Precise tracing GC and cycle collection tests.
- Low heap-limit/lazy builtin tests.
- MIR verifier and exact-root tests.
- Unsupported/later-gate syntax leaves no native artifact.
- Public native CPython differential infrastructure.
- Forbidden artifact/symbol checks.

## Rules
- No giant adversarial corpus solely for Gate 3.
- Prefer one compact fixture covering several ordinary capabilities.
- Test failures should drive code fixes; do not weaken semantics to satisfy old assertions.
- Do not mark Gate 3 complete merely because unit tests pass; final acceptance is Slice 23.

## Completion criteria
Every newly repaired high-risk semantic area has at least one small regression and the established broader proof suites remain green.

## Proof discipline
Slice 22 deliberately adds only `gate3_minimal_regressions.py` for the three public-proof holes found by audit. All other checklist items reuse the named compact fixtures above; no duplicate adversarial Gate 3 corpus was introduced.

## DONE BY CHATGPT
