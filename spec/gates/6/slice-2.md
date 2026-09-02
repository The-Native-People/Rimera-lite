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
