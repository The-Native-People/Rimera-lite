# Gate 6 Slice 8 — Composition, GC, Failure Atomicity, and Artifact Proof

## Goal

Prove all Gate 6 behavior composes with the completed synchronous core and
survives allocation, callbacks, exceptions, and collection pressure.

## Integrated work

- Combine closures, defaults, methods, class bodies, descriptors,
  comprehensions, generator expressions, pattern matching, expanded calls,
  recursive generators, `yield from`, injected exceptions, and cleanup.
- Stress resume reentrancy and callback mutation while slots, delegates,
  exception graphs, pending completions, and roots are live.
- Force collection at every generator allocation/resume/delegation/call
  safepoint; prove live suspended state survives and abandoned/completed graphs
  become collectible.
- Force `MemoryError` during construction, suspension saves, exception
  injection, delegation, and terminal cleanup; published state remains valid
  and operations are retry-safe where Python permits.
- Run every public fixture through the real build API and scan artifacts for
  CPython, generated C, `setjmp`, and `longjmp`.

## Completion proof

A compact composition corpus matches CPython 3.12.11 stdout, stderr, status,
traceback shape, and side-effect order with normal and constrained heaps. MIR
root plans and ABI reachability are audited against the final artifact.
