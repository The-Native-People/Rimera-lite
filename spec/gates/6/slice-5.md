# Gate 6 Slice 5 — Exception Injection, `throw`, `close`, and PEP 479

## Goal

Implement generator-directed exception and close operations through the native
resume ABI and managed exception state.

## Integrated work

- Implement Python 3.12 generator `throw` call shapes, normalization, injection
  at the suspension expression, handler interception, traceback propagation,
  and terminal behavior.
- Implement `close` with `GeneratorExit`, including never-started, suspended,
  running, completed, ignored-exit, yielded-during-close, and cleanup-failure
  cases.
- Implement PEP 479 conversion of an escaping `StopIteration` while preserving
  explicit generator return and delegated termination semantics.
- Preserve cause/context/suppression and handled-exception state across injected
  resumes without overwriting an exception raised by generator code.
- Clear terminal state exactly once and keep reentrancy/failure paths valid
  under forced collection and heap limits.

## Completion proof

Runtime and public CPython differentials cover all operation states, throw
normalization, caught/uncaught injection, close outcomes, PEP 479, traceback
shape, repeated operations, GC, and error replacement ordering.
