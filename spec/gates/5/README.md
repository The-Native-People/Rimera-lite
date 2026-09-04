# Gate 5 — Close Functions, LEGB, and Structured Exceptions

Gate 5 is the conformance closure for Rimera's already-substantial native
function, scope, and exception implementation. It must extend the existing
function objects, cells, call binder, exception graph, traceback model, and
cleanup CFG. It may not introduce a second call path, scope model, unwinder, or
exception representation.

Gate 6 is closed. Gate 5 Slices 1–5 are complete and **Slice 6 is the sole
active compatibility slice**. Continue this ledger in numeric order; the Gate 6
prerequisite repairs already landed do not skip the remaining propagation,
composition, and final-audit slices.

Every slice must reach the complete native path:

```text
Python source -> owned syntax/HIR -> semantic analysis -> verified MIR
-> Cranelift -> documented Rust ABI/runtime -> precise GC
-> public CPython 3.12.11 differential
```

## Synchronized slice contract

- The slice size targets sustained GPT-5.6 Sol implementation work: each slice
  should close a meaningful behavior family, not become a chain of micro-slices
  or require a new prompt between compiler, runtime, and proof work.
- Work in numeric order and keep one Gate 5 slice active after the gate is
  promoted. A later slice may be researched but not implemented ahead of its
  dependencies.
- Each slice is deliberately substantial. It may be divided into coordinated
  compiler, runtime/ABI, and proof lanes, provided every file has one active
  owner and all lanes integrate before the slice checkbox changes.
- The compiler lane owns syntax/HIR/sema/MIR/verifier/Cranelift changes. The
  runtime lane owns managed objects, ABI operations, exception state, tracing,
  and reclamation. The proof lane owns public fixtures, CPython differentials,
  negative diagnostics, artifact scans, and documentation evidence.
- Lanes synchronize on stable HIR/MIR and ABI contracts before implementation
  fans out. A runtime helper without compiled source reachability, or compiler
  lowering without runtime/GC proof, is unfinished work in the same slice.
- A discovered prerequisite is added to the active slice and completed there.
  Do not create an alternate binder, cell, frame, traceback, or cleanup model.
- Use focused checks during a slice. At its integration boundary run formatting,
  warning-denied clippy, relevant workspace tests, doc tests, and
  `git diff --check` once.
- Do not update the compatibility status or promote Gate 5 after an individual
  slice. Slice 8 owns the single audited gate promotion.

## Progress ledger

- [x] Slice 1 — ownership, oracle, and gap matrix
- [x] Slice 2 — function identity, metadata, defaults, and decorators
- [x] Slice 3 — authoritative binding and activation isolation
- [x] Slice 4 — complete LEGB, cells, and scope interactions
- [x] Slice 5 — exception objects, hierarchy, normalization, and traceback API
- [ ] Slice 6 — propagation, chaining, groups, and cleanup completion
- [ ] Slice 7 — cross-feature composition, GC, and traceback fidelity
- [ ] Slice 8 — final audit, documentation, and gate closure

## Slice order

1. [Ownership, oracle, and gap matrix](slice-1.md)
2. [Function identity, metadata, defaults, and decorators](slice-2.md)
3. [Authoritative binding and activation isolation](slice-3.md)
4. [Complete LEGB, cells, and scope interactions](slice-4.md)
5. [Exception objects, hierarchy, normalization, and traceback API](slice-5.md)
6. [Propagation, chaining, groups, and cleanup completion](slice-6.md)
7. [Cross-feature composition, GC, and traceback fidelity](slice-7.md)
8. [Final audit, documentation, and gate closure](slice-8.md)

Gate 5 closes only when compatibility rows 1–3 can honestly move from
`Substantial partial` to `Implemented — conformance audit pending`. Reflection
surfaces beyond the metadata explicitly produced here remain Gate 7-owned.
