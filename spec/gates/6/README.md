# Gate 6 — Complete Native Synchronous Generators

Gate 6 extends the one native generator implementation already exercised by
Gate 4 generator expressions. It owns general source `yield`, `send`, `throw`,
`close`, `yield from`, suspension-aware cleanup, and complete synchronous
generator lifecycle behavior. It must not add a generator-specific interpreter,
duplicate object model, alternate call binder, or second resume ABI.

Every slice must reach the complete native path:

```text
Python source -> owned syntax/HIR -> semantic analysis -> verified MIR
-> Cranelift resume state machine -> documented Rust ABI/runtime
-> precise suspended-state GC -> public CPython 3.12.11 differential
```

## Synchronized slice contract

- The slice size targets sustained GPT-5.6 Sol implementation work: each slice
  should close a meaningful behavior family, not become a chain of micro-slices
  or require a new prompt between compiler, runtime, and proof work.
- Work in numeric order and keep exactly one Gate 6 slice active.
- Each slice is substantial and may run coordinated compiler, runtime/ABI, and
  proof lanes concurrently. Give each file one active owner; synchronize first
  on HIR/MIR state transitions and ABI operation/outcome contracts.
- The compiler lane owns generator classification, HIR/MIR, verifier/liveness,
  resume dispatch, and Cranelift. The runtime lane owns generator objects,
  methods, exception injection, delegation state, tracing, and reclamation. The
  proof lane owns source fixtures, CPython differentials, negative diagnostics,
  heap-limit/GC cases, artifact scans, and evidence updates.
- A slice integrates only when compiled source reaches the runtime behavior and
  its public proof passes. No lane may claim independent slice completion.
- A concrete missing function/cell/exception prerequisite is repaired inside
  the active slice using the existing Gate 5-owned model. Record the dependency,
  add regression proof, and continue Gate 6; do not broadly promote Gate 5.
- Use focused checks during development. At each slice integration boundary run
  formatting, warning-denied clippy, relevant workspace tests, doc tests, and
  `git diff --check` once.
- Do not update compatibility status after an individual slice. Slice 9 owns
  the single audited Gate 6 promotion.

## Progress ledger

- [ ] Slice 1 — ownership, ABI baseline, and differential matrix
- [ ] Slice 2 — source generator classification and lazy construction
- [ ] Slice 3 — yield/resume CFG, values, and persistent liveness
- [ ] Slice 4 — generator protocol surface, return values, and exhaustion
- [ ] Slice 5 — exception injection, `throw`, `close`, and PEP 479
- [ ] Slice 6 — suspension through handlers and cleanup regions
- [ ] Slice 7 — complete `yield from` delegation state machine
- [ ] Slice 8 — composition, GC, failure atomicity, and artifact proof
- [ ] Slice 9 — final audit, documentation, and gate closure

## Slice order

1. [Ownership, ABI baseline, and differential matrix](slice-1.md)
2. [Source generator classification and lazy construction](slice-2.md)
3. [Yield/resume CFG, values, and persistent liveness](slice-3.md)
4. [Generator protocol surface, return values, and exhaustion](slice-4.md)
5. [Exception injection, `throw`, `close`, and PEP 479](slice-5.md)
6. [Suspension through handlers and cleanup regions](slice-6.md)
7. [Complete `yield from` delegation state machine](slice-7.md)
8. [Composition, GC, failure atomicity, and artifact proof](slice-8.md)
9. [Final audit, documentation, and gate closure](slice-9.md)

Gate 6 closes only when the generator compatibility row can move from
`Foundation only` to `Implemented — conformance audit pending`. Async generators
and async iteration remain later async-gate work.
