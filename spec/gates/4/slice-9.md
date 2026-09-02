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

## Completion evidence

- Syntax/HIR now represent a comparison as the first operand plus ordered
  operator/right-operand pairs; the former `RIM-CAP-G4-09` denial is removed.
- Lowering evaluates each middle operand once and carries it to the next
  comparison as a block parameter. Non-final comparison results are truth-tested
  for short-circuiting; the false non-final result or final comparison result is
  preserved exactly, matching CPython's value semantics even when a comparison
  method returns a non-`bool` object.
- Equality, ordering, identity, membership, reflected/subclass dispatch, and
  mixed chains all reuse the existing generic runtime protocols. During this
  slice two protocol parity gaps exposed by mixed chains were corrected:
  `__ne__` fallback negates truth-tested `__eq__`, and sequence/iterator
  membership checks identity before equality as CPython does.
- `gate4_comparison_chains_match_cpython_312` is byte-for-byte green for middle
  operand side effects, false short-circuit, non-boolean comparison results,
  mixed `is`/`in`/ordering/equality operators, and callback exceptions.
- `gate4_comparison_chain_carries_middle_operand_and_exception_edges` pins the
  continuation block parameter, truth-branch roots, reuse of that parameter as
  the next left operand, and explicit exception successors on both comparisons.
- `gate4_comparison_callback_keeps_both_operands_alive_during_forced_collection`
  forces collection inside user `__lt__` and proves both managed operands remain
  live for the callback.

## DONE BY CHATGPT
