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
