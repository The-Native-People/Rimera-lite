# Gate 4 Slice 3 — Nested Exact Unpacking

## Goal

Implement recursively nested unpacking without starred targets.

## Implementation

- Lower each sequence level through generic iteration, never collection layout
  inspection in generated code.
- Detect too few and too many values at the correct nesting level with
  CPython-shaped `ValueError` messages.
- Preserve left-to-right target stores and observable partial assignment when
  a later nested target fails, matching CPython.
- Support name, attribute, and item leaves through the Slice 2 target plan.
- Root every consumed value and iterator across nested safepoints.

## Public fixtures

- Tuple/list nesting, mixed target shapes, custom iterators, and generator-like
  user iterators.
- Failing inner and outer arity with observable earlier assignments.
- Attribute and item targets whose receiver/index has side effects.

## Completion criteria

Source, HIR, MIR, Cranelift, runtime iteration, GC pressure, and CPython 3.12.11
differentials all pass.

## Completion evidence

- Recursive sequence targets are admitted by semantic analysis and lower
  through the Slice 2 target writer rather than being flattened into names.
- Every exact nesting level emits the shared MIR `Unpack` operation and then
  recursively writes its child targets left-to-right. Name, attribute, and
  item leaves therefore inherit the proven single-evaluation store machinery.
- Runtime unpacking uses generic `iterator_new`/`iterator_next`; generated code
  never inspects list/tuple storage or requires a length. The source iterator
  and all already-consumed managed values are temporary roots across each
  fallible iteration/allocation safepoint.
- Exact arity errors are managed `ValueError` objects with CPython-shaped
  `not enough values to unpack (expected N, got M)` / `too many values to
  unpack (expected N)` messages at the nesting level that failed.
- `gate4_nested_unpacking.py` covers nested user single-pass iterators,
  mixed name/attribute/item leaves, receiver/index side-effect order, inner
  failure with observable earlier outer assignment, outer failure with no
  stores, and iterator-raised exception propagation.

## Proof

`gate4_nested_exact_unpacking_matches_cpython_312` compares status, stdout, and
stderr byte-for-byte with CPython 3.12.11 and verifies the resulting executable
contains no legacy Python/unwind fallback symbols.

## DONE BY CHATGPT
