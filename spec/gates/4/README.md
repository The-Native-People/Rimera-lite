# Gate 4 — Unpacking, Comprehensions, Expanded Calls, and Synchronous Syntax

Gate 4 is complete only when every numbered slice in this directory is
complete in order. A slice is complete only when its behavior reaches:

```text
Python source -> owned syntax/HIR -> verified MIR -> Cranelift
-> documented Rust ABI/runtime -> precise GC -> public native differential
```

Runtime-only helpers, parser acceptance, capability diagnostics, ignored tests,
and hand-built MIR do not complete a slice. Gate 3 must be green before Gate 4
implementation begins. Existing object, iterator, call-binding, exception, and
cleanup machinery must be extended rather than duplicated.

## Per-slice execution contract

- Work in numeric order and keep exactly one slice active.
- Inspect the current worktree before editing; preserve passing work from prior
  slices and repair regressions rather than weakening tests.
- A newly discovered prerequisite is part of the active slice. Record the
  deviation in that slice, implement it, and continue.
- Add focused syntax/sema, MIR/verifier, runtime/GC, and public-native tests as
  applicable. Public behavior is compared with CPython 3.12.11.
- Every fallible operation has an exception successor and every allocation or
  invocation has an exact safepoint root plan.
- Unsupported later-architecture syntax produces a stable source diagnostic
  and no artifact; included Gate 4 syntax may not remain capability-denied.
- Run focused checks while developing. At a slice boundary run formatting,
  warning-denied clippy, workspace tests, documentation tests, and
  `git diff --check` once.
- Do not change `TODO.md` or the compatibility status after an individual
  slice. Slice 24 performs the single audited Gate 4 promotion.

## Progress ledger

- [x] Slice 1 — baseline and syntax ownership
- [x] Slice 2 — recursive targets
- [x] Slice 3 — nested exact unpacking
- [x] Slice 4 — general starred unpacking
- [x] Slice 5 — loop/comprehension destructuring
- [x] Slice 6 — dictionary unpacking
- [x] Slice 7 — expanded calls
- [x] Slice 8 — boolean short-circuit
- [x] Slice 9 — chained comparisons
- [x] Slice 10 — slicing and extended subscripts
- [x] Slice 11 — augmented assignment, deletion, assertions
- [x] Slice 12 — assignment expressions
- [x] Slice 13 — annotations
- [x] Slice 14 — f-strings
- [x] Slice 15 — comprehension scope/CFG
- [x] Slice 16 — list comprehensions
- [x] Slice 17 — set/dictionary comprehensions
- [x] Slice 18 — generator expressions
- [x] Slice 19 — pattern CFG/bindings
- [x] Slice 20 — core patterns
- [x] Slice 21 — sequence/mapping patterns
- [x] Slice 22 — class patterns
- [x] Slice 23 — composition/GC/failure proof
- [x] Slice 24 — final audit and tracker closure

## Slice order

1. Baseline and syntax-ownership audit
2. Recursive assignment-target model
3. Nested exact unpacking
4. General starred unpacking
5. Destructuring in `for` and comprehensions
6. Mapping displays and dictionary unpacking
7. Expanded calls with `*` and `**`
8. Boolean short-circuit expressions
9. Chained comparisons
10. Complete slicing and extended subscripts
11. General augmented assignment, deletion, and assertions
12. Assignment expressions (`:=`)
13. Function, variable, and class annotations
14. F-strings and formatting conversions
15. Comprehension scope and lowering foundation
16. List comprehensions
17. Set and dictionary comprehensions
18. Generator expressions and suspension handoff
19. Pattern-matching control-flow foundation
20. Literal, capture, wildcard, OR, and AS patterns
21. Sequence and mapping patterns
22. Class patterns and `__match_args__`
23. Cross-feature ordering, GC, and failure atomicity
24. Gate audit, documentation, and closure

No slice may move ahead by weakening a prior slice or by assigning included
syntax to a later gate. Generator expressions are the sole planned dependency
on the Gate 6 suspension engine; Slice 18 must provide executable native proof,
not only syntax ownership, before Gate 4 closes.
