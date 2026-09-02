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

## Completion evidence

- Ordered boolean operands remain first-class HIR and lower to native MIR branch
  blocks. `and`/`or` truth-test only the operands needed to select a result and
  merge the selected original `RValue` through block parameters rather than
  canonicalizing it to `bool`.
- `gate4_boolean_short_circuit_finishing_matches_cpython_312` is byte-for-byte
  green for user `__bool__`, `__len__`, multi-operand expressions, skipped
  exceptions, selected-object identity, nested calls, and control-flow use.
- `gate4_boolean_short_circuit_cfg_roots_selected_values_at_truth_safepoints`
  pins the two short-circuit branches for a three-operand expression, their
  parameterized value merges, and the branch-root sets containing the truth
  operand and any selected edge value.
- `gate4_boolean_truth_callback_survives_forced_collection` forces collection
  inside a user `__bool__` callback and proves the receiver survives the
  invocation boundary.

## DONE BY CHATGPT
