# Gate 4 Slice 18 — Generator Expressions and Suspension Handoff

## Goal

Make generator expressions executable native generators rather than parser-only
syntax or eagerly materialized collections.

## Implementation

- Reuse the Slice 15 implicit function scope and clause CFG.
- Mark the implicit function as a generator and lower its sink to a suspension
  terminator using the single Gate 6 generator object/resume ABI.
- Evaluate the outermost iterable immediately when the generator expression is
  created; evaluate inner clauses lazily during resumption.
- Persist only values live across suspension, including iterators, targets,
  closures, and pending exception/cleanup state.
- Support `next`, iteration, exhaustion, exceptions, and closure capture through
  the ordinary generator protocol.
- Do not add a special eager iterator object or runtime MIR interpreter.

## Dependency rule

If suspension MIR/codegen is not available, this slice implements the minimum
shared generator machinery required and Gate 6 extends it later with source
`yield`, `send`, `throw`, `close`, and `yield from`. Gate 4 cannot close with a
capability diagnostic for generator expressions.

## Proof

Public differentials prove laziness, evaluation order, exceptions, closure
mutation, repeated exhaustion, and forced-GC survival.

## Completion evidence

- `lower::tests::gate4_generator_expression_mir_uses_yield_and_persists_only_live_state`
  proves hidden `Generator` MIR, the `.0` outer iterator, explicit `Yield`,
  liveness-derived persistence, and suspension roots without whole-frame
  retention.
- `mir::tests::generator_yield_verification_and_persistent_liveness_are_explicit`
  proves the verifier rejects `Yield` outside generator functions and separates
  yield-time roots from values that actually remain live after resumption.
- `gate4_generator_expressions_match_cpython_312` covers immediate outer
  iterable evaluation, lazy filters/elements/inner clauses, closure mutation,
  nested iteration, exceptions, ordinary iteration, and repeated exhaustion.
- `gate4_generator_expression_state_survives_forced_gc` runs the public native
  artifact under a 32 KiB managed-heap limit while suspended state crosses
  repeated allocation/collection pressure and compares stdout/stderr/status
  with CPython 3.12.11.
- Public artifacts are scanned by `assert_native_only_artifact` for CPython,
  generated-C, `setjmp`, and `longjmp` symbols.

## DONE BY CHATGPT
