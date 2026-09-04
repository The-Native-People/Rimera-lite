# Gate 9 — Imports, Modules, Packages, and `sys.modules`

Gate 9 replaces the narrow registered-module shell with one owned resolver and
runtime module system. It begins only after Gates 5, 7, and 8 close. Source
resolution, initialization order, caching, and Python-visible import state must
agree; a module cannot be considered implemented from syntax lowering alone.

Every slice reaches source/HIR/sema/verified MIR/Cranelift, the documented Rust
runtime contract, GC ownership, and public CPython 3.12.11 differential proof.
Resolution must be deterministic from project metadata and a lockfile; builds
must not consult an ambient virtual environment or unpinned network state.

## Progress ledger

- [ ] Slice 1 — ownership, module graph, lock/manifest contract, and oracle matrix
- [ ] Slice 2 — source modules, cached identity, initialization, and cycles
- [ ] Slice 3 — `import`, `from`, aliases, star imports, and binding order
- [ ] Slice 4 — packages, `__init__`, relative imports, and namespace metadata
- [ ] Slice 5 — `sys.modules`, `__import__`, import hooks, reload, and failure rollback
- [ ] Slice 6 — package resources, deterministic object caching, and capabilities
- [ ] Slice 7 — composition, concurrent/reentrant import state, GC, and atomicity
- [ ] Slice 8 — final audit, documentation, and gate closure

## Slice acceptance

1. Define one module identity and state machine (`created`, `initializing`,
   `ready`, `failed`), canonical names, package roots, lockfile inputs, stable
   diagnostics, and the differential corpus.
2. Compile each reachable Python module to its own object, execute it once,
   expose partially initialized modules during cycles, and remove failed first
   imports according to Python-visible cache behavior.
3. Implement all synchronous import statement forms with exact evaluation,
   binding, `__all__`, missing-name, and star-import rules.
4. Implement regular and namespace packages, relative-level resolution, and
   coherent `__name__`, `__package__`, `__path__`, `__file__`, `__spec__`, and
   loader metadata without exposing compiler internals.
5. Publish the authoritative module cache through `sys.modules`; implement
   ordinary `__import__`, supported hooks, invalidation, and reload semantics
   on the same state machine.
6. Include package data and native module declarations in reproducible
   manifests and cache keys; capability denial happens before artifact output.
7. Stress cycles, callbacks, failures, cache mutation, constrained heaps, and
   multi-module exception tracebacks without duplicate module objects.
8. Audit every import form and module attribute, run the full verification
   boundary, and promote imports/modules only from public proof.

Gate 9 does not claim the standard library or arbitrary PyPI packages; it
provides the module machinery those later gates consume.
