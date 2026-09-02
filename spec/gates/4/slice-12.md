# Gate 4 Slice 12 — Assignment Expressions (`:=`)

## Goal

Implement named expressions with Python 3.12 scope and evaluation behavior.

## Implementation

- Add owned named-expression syntax/HIR with a name-only target.
- Resolve the target using the surrounding scope rules, including special
  comprehension binding into the containing scope where Python requires it.
- Evaluate the value once, store it, and return the identical `RValue`.
- Enforce syntax restrictions for forbidden positions and comprehension target
  conflicts with precise spans.
- Lower through existing local/cell/global stores with explicit exception and
  root behavior.

## Proof

- Differentials cover conditions, loops, calls, lambdas, closures, and every
  legal comprehension placement.
- Negative fixtures cover invalid rebinding and syntax contexts with no
  artifact.

## Completion evidence

- `NamedExpression` is owned by syntax/HIR and resolves its target through the
  normal global/local/cell/free binding model. MIR evaluates the RHS once,
  stores that exact value, and returns it without recomputation.
- `gate4_assignment_expressions.py` matches CPython 3.12 for conditions, loops,
  function globals/nonlocals, closures, lambdas, class bodies, and object
  identity. The MIR regression pins the single RHS call, normal store, returned
  value, and failure edge.
- Comprehension-specific implicit-scope walrus classification remains with the
  explicit Slice 15 comprehension-scope contract, with public comprehension
  execution beginning in Slice 16. Slice 12 leaves no separate walrus runtime
  or compiler intrinsic for that later scope to bypass.

## DONE BY CHATGPT
