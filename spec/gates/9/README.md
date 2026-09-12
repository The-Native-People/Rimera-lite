# Gate 9 — Imports, Modules, Packages, and `sys.modules`

Gate 9 replaces the narrow Gate 7 registered-module shell with one owned,
deterministic module resolver and runtime state machine. Source resolution,
native object emission, initialization order, caching, and Python-visible
import state must agree; syntax lowering or a runtime-only registry does not
complete a slice.

Every executable slice reaches source/HIR/sema/verified MIR/Cranelift, the
documented Rust ABI/runtime, precise GC ownership, and public CPython 3.12.11
differential proof. Builds resolve only project inputs and pinned manifests;
they never inspect an ambient virtual environment or fetch unpinned network
state.

## Synchronized slice contract

- Execute slices in numeric order. After Slice 1, implement and review two
  consecutive slices per work batch, but check off each slice only when its own
  evidence passes. The earlier slice remains the sole active compatibility
  slice until that evidence is green.
- Extend the existing `ModuleObject`, context-owned module cache, and
  `ImportName` path. A second registry, loader, namespace, or interpreted module
  executor is forbidden.
- Resolver work owns canonical names, paths, graph edges, package roots,
  manifests, capabilities, and diagnostics. It has no runtime effects.
- Compiler work owns import bindings, per-module MIR, native initializers,
  relocations, and source spans. Runtime work owns module identity/state,
  namespaces, initialization transactions, Python-visible caches, and GC.
- Cycles expose the one partially initialized module object. Failed first
  imports roll back the authoritative cache without leaving a second identity.
- Focused checks are used during a slice. The full workspace, warning-denied
  Clippy, formatting, release-size, reproducibility, and forbidden-symbol
  boundary runs once in Slice 12.

## Progress ledger

- [x] Slice 1 — ownership, canonical identity, state machine, and oracle matrix
- [x] Slice 2 — deterministic project discovery, graph edges, and diagnostics
- [x] Slice 3 — per-module MIR/object emission, linking, and native initialization
- [x] Slice 4 — cached identity, initialization cycles, and failure rollback
- [x] Slice 5 — plain and dotted `import`, aliases, and binding order
- [x] Slice 6 — `from` imports, `__all__`, star imports, and missing-name behavior
- [x] Slice 7 — regular packages, `__init__`, relative imports, and metadata
- [x] Slice 8 — namespace packages, search roots, and parent publication
- [x] Slice 9 — authoritative `sys.modules`, `__import__`, and cache mutation
- [x] Slice 10 — import hooks, reload, invalidation, and reentrant loading
- [x] Slice 11 — locked dependencies, package data, capabilities, and cache manifests
- [x] Slice 12 — cross-feature composition, reentrancy, and constrained-heap stress
- [x] Slice 13 — reproducibility, final audit, documentation, and gate closure

## Slice order

1. [Ownership, canonical identity, state machine, and oracle matrix](slice-1.md)
2. [Deterministic project discovery, graph edges, and diagnostics](slice-2.md)
3. [Per-module MIR/object emission, linking, and native initialization](slice-3.md)
4. [Cached identity, initialization cycles, and failure rollback](slice-4.md)
5. [Plain and dotted `import`, aliases, and binding order](slice-5.md)
6. [`from` imports, `__all__`, star imports, and missing-name behavior](slice-6.md)
7. [Regular packages, `__init__`, relative imports, and metadata](slice-7.md)
8. [Namespace packages, search roots, and parent publication](slice-8.md)
9. [Authoritative `sys.modules`, `__import__`, and cache mutation](slice-9.md)
10. [Import hooks, reload, invalidation, and reentrant loading](slice-10.md)
11. [Locked dependencies, package data, capabilities, and cache manifests](slice-11.md)
12. [Cross-feature composition, reentrancy, and constrained-heap stress](slice-12.md)
13. [Reproducibility, final audit, documentation, and gate closure](slice-13.md)

Gate 9 does not implement the standard library, wheel/native-extension ABI, or
arbitrary PyPI compatibility. Those later gates consume this module machinery.

All thirteen Gate 9 slices are complete. Imports/modules/packages are promoted to
`Implemented — conformance audit pending` within Rimera's deterministic compiled
module roots and lock/manifest contract. The next active compatibility slice is
Gate 10 Slice 1.
