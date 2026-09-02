# Gate 5 Slice 4 — Complete LEGB, Cells, and Scope Interactions

## Goal

Close compile-time binding and runtime cell behavior across all synchronous
scope kinds already owned by Rimera.

## Integrated work

- Complete local/global/free/cell classification for modules, functions,
  lambdas, comprehensions, generator expressions, class bodies, annotations,
  pattern bindings, and nested declarations.
- Enforce Python declaration conflicts and unbound-local/free-variable errors
  with stable spans and CPython-compatible observable behavior.
- Complete `global`, `nonlocal`, deletion, rebinding, shadowing, and shared-cell
  mutation across sibling closures and recursive activations.
- Preserve the class-namespace lookup rules, `__class__` cells, comprehension
  isolation, walrus binding rules, and the rule that methods do not close over
  ordinary class locals.
- Prove cell/function/class/generator cycles are traced and reclaimed without
  retaining dead activation state.

## Completion proof

The public matrix combines every scope kind with reads, writes, deletes,
closures, recursion, exceptions, comprehensions, class bodies, and forced GC;
compile-time declaration failures produce no artifact.
