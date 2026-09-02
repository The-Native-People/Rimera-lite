# Gate 6 Slice 4 — Generator Protocol Surface, Return Values, and Exhaustion

## Goal

Expose complete ordinary synchronous generator iteration and result behavior
through managed methods and generic calls.

## Integrated work

- Implement self-iteration, `__next__`, `send`, method binding, reentrancy
  checks, and the error for sending non-`None` into a just-started generator.
- Convert normal generator return into managed `StopIteration` with the exact
  `.value`; preserve bare return and fallthrough behavior.
- Make exhaustion stable and repeatable through `next`, `send`, loops, generic
  iterator operations, and aliases to bound generator methods.
- Preserve terminal values long enough for the protocol operation that owns
  them, then clear persistent slots, delegates, saved exception state, and
  otherwise-dead graph edges.
- Prove user-visible generator type identity and representation through the
  existing object model without prematurely implementing Gate 7 inspection.

## Completion proof

Public differentials cover iteration, sends, returns, `StopIteration.value`,
reentrancy, repeated exhaustion, method aliasing, reclamation, and low-heap
behavior through the generic call and iterator paths.
