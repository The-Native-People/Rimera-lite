# Gate 9 Slice 4 — Cached Identity, Initialization Cycles, and Failure Rollback

## Goal

Execute source modules once while preserving Python-visible partial state during
cycles and transactional behavior on failure.

## Integrated work

- Insert one created module into the authoritative cache before its initializer
  runs, transition through initializing to ready, and return the same object on
  repeated imports.
- Expose already-published attributes during cycles and report CPython-shaped
  partially initialized/missing-attribute failures.
- Remove failed first imports from the cache, retain successfully imported
  dependencies, preserve exception chaining/tracebacks, and permit later retry.
- Root initializing modules, namespaces, exceptions, and dependency edges across
  allocation and callback safepoints.

## Completion proof

Public differentials cover identity, once-only side effects, direct/indirect
cycles, partial attributes, failure rollback, retry, nested failures, and
constrained-heap collection without duplicate module objects.
