# Gate 4 Slice 15 — Comprehension Scope and Lowering Foundation

## Goal

Build the single semantic and MIR model shared by list, set, dictionary, and
generator comprehensions.

## Implementation

- Represent an element expression plus ordered clauses, recursive targets, and
  ordered filters.
- Create the implicit comprehension function scope. The outermost iterable is
  evaluated in the containing scope; targets and remaining clauses live in the
  implicit scope.
- Prevent target leakage and implement closure capture, cells, globals,
  nonlocals, and walrus rules correctly.
- Lower clauses to nested iterator CFG regions with exhaustion, filters,
  exception successors, and exact roots.
- Reuse Slice 5 target binding. No runtime AST/comprehension evaluator.
- Define sink operations for list append, set insert, dict insert, and yield;
  concrete forms land in Slices 16–18.

## Proof

- Symbol-table tests pin scope classification and non-leakage.
- MIR tests pin nested loops, filters, target failures, and safepoint liveness.
- A hand-built sink test is insufficient; public execution begins in Slice 16.
