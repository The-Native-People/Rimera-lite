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
