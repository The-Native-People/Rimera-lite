# Gate 12 — Weak References, Finalizers, and Object Resurrection

Gate 12 activates the lifecycle phase reserved by the tracing collector. Weak
observation and finalization must extend the existing heap identity/generation
model and must not be approximated with raw addresses or Rust destructors.

## Progress ledger

- [x] Slice 1 — lifecycle states, reachability contract, and oracle matrix
- [x] Slice 2 — weak-reference objects, callbacks, hashing, equality, and proxies
- [x] Slice 3 — weakly held containers and mutation-safe iteration
- [x] Slice 4 — `__del__`, finalization order, exception reporting, and shutdown
- [ ] Slice 5 — resurrection, cyclic isolates, once-only guarantees, and reentrancy
- [ ] Slice 6 — `weakref.finalize`, module teardown, and callback composition
- [ ] Slice 7 — stress, concurrent mutation boundaries, heap limits, and atomicity
- [ ] Slice 8 — final audit, documentation, and gate closure

## Slice acceptance

1. Specify strong/weak/finalizable/resurrected/dead states and collector phases
   before exposing any Python API.
2. Implement `weakref.ref`, callback ordering, dead referents, stable hash and
   equality behavior, callable refs, and transparent proxies.
3. Implement weak key/value dictionaries and weak sets without accidentally
   strengthening referents across callbacks or iteration.
4. Run `__del__` with Python-visible exception reporting and deterministic
   once-only state across ordinary collection and runtime shutdown.
5. Match resurrection and cyclic-isolate behavior while preventing double
   finalization, stale handles, and generation reuse bugs.
6. Implement finalizer objects and teardown ordering over real module globals.
7. Stress callback allocation, mutation, reentrancy, exceptions, and repeated
   collection under constrained heaps.
8. Audit lifecycle APIs and collector invariants before promotion.

This gate owns object lifetime semantics; individual standard-library helpers
outside `weakref` remain Gate 14 work.

The exact state machine, phase ordering, CPython 3.12.11 oracle matrix, exposed
surface, and remaining boundaries are recorded in
[`lifecycle-contract.md`](lifecycle-contract.md). Gate 12 Slice 5 is active.
