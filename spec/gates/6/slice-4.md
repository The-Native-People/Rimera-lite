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

## Completion evidence — 2026-09-03

- Managed generator attribute dispatch exposes `__iter__`, `__next__`, `send`, `throw`, and `close`; builtin `next()` uses the same native resume state.
- Bare/fallthrough return and valued return produce CPython-shaped repeated exhaustion and `StopIteration.value` behavior; just-started non-`None` send is a managed `TypeError`.
- `gate6_source_generator_protocol_matches_cpython_312_under_gc_pressure` matches CPython 3.12.11 status/stdout/stderr through the public build pipeline.

## DONE BY CHATGPT
