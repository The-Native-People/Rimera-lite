# Gate 5 Slice 2 — Function Identity, Metadata, Defaults, and Decorators

## Goal

Complete Python-visible function construction without moving broad inspection
APIs out of Gate 7.

## Integrated work

- Complete `__name__`, `__qualname__`, `__annotations__`, positional defaults,
  keyword defaults, closure ownership, and the code metadata required by the
  later reflection gate.
- Evaluate default expressions and decorators once, in Python order, with
  bottom-up decorator application and failure-atomic publication.
- Preserve mutable default identity, traced annotation/default graphs, nested
  qualified names, lambdas, methods, class-body functions, and comprehension
  boundaries.
- Route decorated and aliased functions through the ordinary generic call path;
  do not add compiler-only invocation shortcuts.
- Prove collection and heap-limit behavior for cycles involving functions,
  defaults, annotations, cells, classes, and bound methods.

## Completion proof

Public CPython differentials cover evaluation order, metadata values, mutation,
decorator replacement/failure, nested scopes, forced GC, and exact callable
identity across aliasing and rebinding.
