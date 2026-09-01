# Gate 4 Slice 9 — Chained Comparisons

## Goal

Implement Python comparison chains with single evaluation of middle operands.

## Implementation

- Represent the first operand plus ordered operator/right-operand pairs.
- Lower each comparison through the established generic comparison or identity
  protocol.
- Evaluate a middle operand once, reuse it as the next left operand, and root
  it across the comparison call.
- Truth-test each non-final result and short-circuit on false while returning a
  boolean chain result.
- Support equality, ordering, identity, and membership operators in one chain.
- Preserve subclass/reflected comparison dispatch from Gate 2.

## Proof

- Differentials pin side-effect counts, mixed operators, false short-circuit,
  callback exceptions, and managed operands under forced GC.
- MIR tests pin block parameters, exception successors, and operand liveness.
