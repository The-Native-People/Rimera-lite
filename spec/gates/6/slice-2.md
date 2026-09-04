# Gate 6 Slice 2 — Source Generator Classification and Lazy Construction

## Goal

Compile ordinary functions containing source `yield` into the existing native
generator function/object path without executing their bodies at call time.

## Integrated work

- Add owned syntax/HIR forms for `yield` statements and expressions while
  preserving spans and rejecting illegal placements.
- Determine generator function kind from the function's own body without
  descending into nested function, lambda, class, or comprehension scopes.
- Reuse ordinary parameter binding, defaults, keyword defaults, closures,
  decorators, annotations, method binding, and qualified-name construction.
- Make calls allocate a suspended generator after binding while deferring body
  execution, local initialization effects, and exceptions until first resume.
- Preserve closure/default/function roots across the suspended-before-start
  state and reclaim abandoned never-started generators.

## Completion proof

Public differentials cover classification boundaries, argument failures,
construction-time versus resume-time side effects/errors, methods, closures,
never-started collection, and stable diagnostics for illegal `yield` forms.

## Completion evidence — 2026-09-03

- Source `yield`/`yield from` are owned syntax/HIR forms and semantic analysis rejects invalid placement before artifact output.
- Function-kind discovery recognizes yields in the current function without treating nested function bodies as yields of the owner; ordinary binding constructs a suspended generator without executing its body.
- `gate6_source_generator_protocol_matches_cpython_312_under_gc_pressure` differentially proves lazy start, first resume, invalid initial send, exhaustion, and close-before-start under a 96 KiB managed heap limit.

## DONE BY CHATGPT
