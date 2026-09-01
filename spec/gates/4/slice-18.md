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
