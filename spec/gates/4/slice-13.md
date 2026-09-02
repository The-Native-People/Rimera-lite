# Gate 4 Slice 13 — Function, Variable, and Class Annotations

## Goal

Represent and execute synchronous Python 3.12 annotation behavior needed by
functions, scopes, classes, and reflection.

## Implementation

- Preserve parameter, return, variable, and annotated-assignment expressions
  with source order and spans.
- Materialize function `__annotations__` when definitions execute.
- Store module and class annotations in their namespace `__annotations__`
  mapping; local variable annotations do not evaluate or create a local value
  unless an assignment exists.
- Preserve qualified names and closure/global lookup during annotation
  evaluation according to the selected Python 3.12 semantics.
- Support annotated assignment to name, attribute, and item targets where
  Python allows it.
- Route namespace changes through normal mapping and cache invalidation paths.

## Proof

- Public fixtures inspect annotations through already supported attributes and
  dictionaries.
- Tests cover evaluation order, exceptions, class scope, and no-value locals.

## Completion evidence

- Parameter, return, and annotated-assignment expressions survive owned
  syntax/HIR with source spans. Module/class annotations use their ordinary
  namespace mapping, while function-local variable annotations remain
  non-evaluated as in Python 3.12.
- Function objects own an optional GC-traced annotations dictionary exposed by
  ordinary `__annotations__` attribute access. `rimera_annotations_ensure`
  preserves an existing module/class mapping and initializes only when absent.
- `gate4_annotations.py` and the former annotation capability fixture match
  CPython 3.12 for value/annotation/default order, function reflection, class
  namespace reflection, local no-value behavior, and `None` reset. MIR proof
  pins module/class annotation initialization and local non-evaluation.

## DONE BY CHATGPT
