# Gate 9 Slice 1 — Ownership, Canonical Identity, State Machine, and Oracle Matrix

## Goal

Establish one compiler/runtime contract for module identity and initialization
before project discovery or multi-module execution expands the import surface.

## Integrated work

- Define validated canonical absolute module names and distinguish source
  modules, regular packages, namespace packages, and registered native shells.
- Replace the single-path graph placeholder with ordered module nodes and
  explicit import edges carrying source spans and relative-import levels.
- Extend the existing managed `ModuleObject` with one initialization state:
  `created`, `initializing`, `ready`, or `failed`. The context-owned cache is
  authoritative throughout every transition.
- Specify insertion-before-execution, successful publication, failed-first-
  import rollback, retry, and cycle observation without creating a second
  module identity.
- Assign canonical metadata, binding forms, packages, public cache mutation,
  hooks, resources, concurrency/reentrancy, GC, and failure cases to Slices
  2–11 in a CPython 3.12.11 oracle matrix.

## Proof

Compiler tests pin canonical-name validation, deterministic node/edge ordering,
duplicate-edge consolidation, and stable source spans. Runtime tests pin every
legal state transition, reject illegal transitions, retain cached modules and
namespaces through collection, and prove failed unpublished objects collect.
No broader import compatibility status changes in this ownership slice.

## Status

Complete. `CanonicalModuleName` owns validated absolute identities and the
ordered `ModuleGraph` owns nodes, canonical edges, conflicting-owner rejection,
and every edge's sorted source occurrences. The existing managed `ModuleObject`
now owns the validated `created -> initializing -> ready|failed` lifecycle.
Focused proof is green: three resolver ownership tests, the runtime lifecycle/
GC test, the existing public CPython 3.12.11 import differential, warning-denied
compiler/runtime Clippy, formatting, and diff checks. The supported import
surface is intentionally unchanged; Slice 2 is active.
