# Gate 8 Slice 4 — Return, Break, Continue, and Nested Cleanup Composition

## Goal

Run context exits exactly once for every non-exception completion and compose
them with existing `finally` and loop cleanup.

## Integrated work

- Extend compiler-planned completion handling so return values, break,
  continue, and normal fallthrough cross one or more context-exit actions in
  inner-to-outer order.
- Compose nested `with`, `try`/`finally`, loops, functions, class bodies, and
  comprehensions without duplicating cleanup CFG or losing pending payloads.
- Allow an exit method's exception or control effect to replace the pending
  completion according to the authoritative cleanup rules.
- Verify cleanup-stack shape, continuation targets, dominance, exception edges,
  and roots for pending return values and manager graphs.
- Ensure callback reentrancy and collection cannot rerun, skip, or reorder an
  exit action.

## Completion proof

Public differentials pin side-effect order and final results for nested normal,
return, break, and continue paths combined with `finally`, callbacks, forced GC,
and heap limits.
