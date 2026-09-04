# Gate 6 Slice 7 — Complete `yield from` Delegation State Machine

## Goal

Implement PEP 380 delegation through compiled control flow and the existing
generator/iterator protocols.

## Integrated work

- Evaluate the delegate once, acquire its iterator through the generic
  protocol, and persist delegate plus forwarding state across suspension.
- Forward `next`, `send`, `throw`, and `close` with the required method lookup,
  missing-method behavior, exception propagation, and evaluation order.
- Extract `StopIteration.value` as the `yield from` expression result and
  distinguish delegated completion from PEP 479 conversion in the delegating
  generator.
- Support native generators, generator expressions, builtin iterators, and user
  iterator/delegate objects through one path; no native-generator-only shortcut.
- Handle nested delegation, delegate replacement/cleanup, reentrancy, thrown
  exceptions, closing, forced GC, and terminal slot clearing.

## Completion proof

Public CPython differentials cover every forwarded operation and fallback,
nested/native/user delegation, return extraction, callback logs, failures,
tracebacks, cleanup, and collection under constrained heaps.

## Completion evidence — 2026-09-03

- `RGeneratorDelegateOutcome::{Yielded, Completed, Propagate}` plus `rimera_generator_delegate_start/set/resume` form the additive delegation ABI; the active delegate is stored on and traced by the existing generator object.
- Native generators, generator expressions, builtin iterators, and user iterator/delegate objects all use generic iterator/method dispatch; delegated completion yields `StopIteration.value` as the `yield from` expression result.
- `gate6_yield_from_native_builtin_and_nested_match_cpython_312_under_gc_pressure` and `gate6_yield_from_user_delegate_matches_cpython_312_under_gc_pressure` prove send/throw/close forwarding, nested delegation, completion, and propagation.

## DONE BY CHATGPT
