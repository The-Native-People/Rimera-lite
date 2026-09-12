# Gate 8 Slice 2 — Single-Manager Lookup, Enter, Body, and Normal Exit

## Goal

Execute the fundamental single-manager `with` path through compiler-managed
cleanup and ordinary object protocols.

## Integrated work

- Evaluate the manager once and resolve `__enter__` and `__exit__` from the
  manager type/MRO with correct descriptor binding and custom metaclass effects.
- Call `__enter__`, execute the body, and call the captured exit method exactly
  once with `(None, None, None)` on normal fallthrough.
- Preserve CPython lookup/call order when attributes are missing, descriptors
  raise, `__enter__` fails, the body mutates the class, or returned objects
  retain manager state.
- Lower the path through explicit HIR/MIR blocks and exception successors;
  verify exact roots for manager, methods, entered value, and body temporaries.
- Reclaim dead manager/entered-value cycles and leave no partial artifact or
  corrupted state after lookup, entry, or exit failure.

## Completion proof

Public differentials cover normal order, descriptors, inherited/custom methods,
mutation after entry, missing methods, failures, aliases, forced GC, and low
heap limits through the real build API.

## Status

Complete. Public CPython 3.12.11 differentials cover type/MRO descriptor lookup,
metaclass managers, captured-exit mutation behavior, exact lookup/call order,
missing methods, enter/exit failures, aliases, retained state, dead cycles, and
constrained-heap execution through native artifacts.
