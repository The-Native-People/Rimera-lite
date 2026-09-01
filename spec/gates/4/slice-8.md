# Gate 4 Slice 8 — Boolean Short-Circuit Expressions

## Goal

Implement `and` and `or` as value-preserving short-circuit control flow.

## Implementation

- Add ordered boolean-operation HIR rather than reducing to booleans.
- Lower each operand into MIR blocks using the generic truth protocol.
- Evaluate operands once and return the selected original `RValue`.
- Preserve exception edges and skip all later operands after selection.
- Merge values with block parameters and root selected managed values across
  subsequent safepoints.

## Public fixtures

- Truthy/falsy builtin and user objects, multiple operands, side effects,
  exceptions in skipped operands, and managed values under forced GC.
- Nesting with calls, assignments, loops, and conditionals.

## Completion criteria

Output, side-effect order, exception behavior, and result identity match
CPython 3.12.11.
