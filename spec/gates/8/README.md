# Gate 8 — Complete Synchronous Context Managers

Gate 8 implements `with` as compiler-planned cleanup over ordinary special
method lookup and generic native calls. It must not lower context managers to a
runtime shortcut, host-language unwinding, generated C, or a framework-specific
path.

Gate 8 remains queued until Gates 5–7 close. Its generator-interaction slices
consume Gate 6 lifecycle semantics, and its exception/inspection behavior
consumes the final Gate 5 and Gate 7 contracts.

Every slice must reach the complete native path:

```text
Python source -> owned syntax/HIR -> semantic analysis -> verified MIR
-> Cranelift cleanup CFG -> documented Rust ABI/runtime -> precise GC
-> public CPython 3.12.11 differential
```

## Synchronized slice contract

- The slice size targets sustained GPT-5.6 Sol implementation work: each slice
  closes a meaningful context-management family without requiring another
  prompt between compiler, runtime, cleanup, and proof work.
- Work in numeric order and keep exactly one Gate 8 slice active after the gate
  is promoted.
- A slice may use coordinated compiler, runtime/ABI, and proof lanes. Give each
  file one active owner and synchronize first on cleanup actions, completion
  payloads, special-method lookup, call ordering, and root ownership.
- The compiler lane owns syntax/HIR/sema/MIR/verifier/Cranelift cleanup paths.
  The runtime lane owns generic method binding/calls, exception triples, tracing,
  and reclamation. The proof lane owns public fixtures, CPython differentials,
  negative diagnostics, heap-limit/GC cases, artifact scans, and evidence.
- A slice integrates only when every normal and exceptional source path reaches
  the existing runtime protocols with public proof. A helper or hand-built MIR
  test alone does not complete context-manager behavior.
- A discovered prerequisite is completed inside the active slice through its
  existing owner. Do not create a second descriptor lookup, call binder,
  exception triple, generator cleanup, or completion model.
- Use focused checks during development. At a slice boundary run formatting,
  warning-denied clippy, relevant workspace tests, doc tests, and
  `git diff --check` once.
- Do not promote compatibility rows after individual slices. Slice 8 owns the
  single audited Gate 8 promotion.

## Progress ledger

- [ ] Slice 1 — ownership, syntax, cleanup contract, and differential matrix
- [ ] Slice 2 — single-manager lookup, enter, body, and normal exit
- [ ] Slice 3 — targets, multiple managers, partial entry, and ordering
- [ ] Slice 4 — return, break, continue, and nested cleanup composition
- [ ] Slice 5 — exception triples, suppression, replacement, and chaining
- [ ] Slice 6 — suspension, generator `throw`/`close`, and delegated cleanup
- [ ] Slice 7 — composition, GC, failure atomicity, and artifact proof
- [ ] Slice 8 — final audit, documentation, and gate closure

## Slice order

1. [Ownership, syntax, cleanup contract, and differential matrix](slice-1.md)
2. [Single-manager lookup, enter, body, and normal exit](slice-2.md)
3. [Targets, multiple managers, partial entry, and ordering](slice-3.md)
4. [Return, break, continue, and nested cleanup composition](slice-4.md)
5. [Exception triples, suppression, replacement, and chaining](slice-5.md)
6. [Suspension, generator `throw`/`close`, and delegated cleanup](slice-6.md)
7. [Composition, GC, failure atomicity, and artifact proof](slice-7.md)
8. [Final audit, documentation, and gate closure](slice-8.md)

Gate 8 closes only when synchronous context managers can honestly move to
`Implemented — conformance audit pending`. `async with`, async context-manager
protocols, `contextlib`, and other stdlib helpers remain later-gate work.
