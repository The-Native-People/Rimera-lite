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
