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

## Completion evidence

- Syntax/HIR own one shared comprehension representation with ordered clauses,
  recursive targets, filters, list/set/dict/generator kind, and explicit hidden
  scope metadata. The outermost iterable is lowered in the containing scope;
  the remaining clauses execute in a hidden native function with positional
  parameter `.0`.
- Semantic planning classifies iteration targets as hidden-scope locals while
  closure reads and walrus bindings use ordinary free/cell/global resolution.
  Invalid walrus placement in iterable expressions, rebinding of an iteration
  variable, and class-body comprehension walrus use fail before MIR. Generator
  execution remains explicitly owned by Slice 18.
- MIR lowers nested clauses to explicit iterator/exhaustion/filter CFG regions.
  Safepoint tests pin hidden-call exception edges plus live result, iterator,
  key/value, and element roots.
- `gate4_comprehension_scope.py` publicly matches CPython 3.12 for outermost
  iterable ordering, class-scope lookup, non-leakage, closure capture, module
  and function walrus behavior, nested comprehensions, and global writes.

## DONE BY CHATGPT
