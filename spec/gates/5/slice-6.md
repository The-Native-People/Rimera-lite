# Gate 5 Slice 6 — Propagation, Chaining, Groups, and Cleanup Completion

## Goal

Close exception control flow through the existing explicit MIR edges and
compiler-planned cleanup regions.

## Integrated work

- Complete bare reraising, explicit causes, implicit context, suppression,
  handler-target cleanup, and replacement exceptions raised while handling.
- Complete exception-group construction, recursive matching/splitting,
  `except*` execution, subgroup derivation, reraising, and merge ordering.
- Preserve return, break, continue, normal fallthrough, and exceptions through
  nested `try`/`except`/`else`/`finally` regions; cleanup-issued transfers
  replace pending completion exactly once.
- Verify call and cleanup exception successors, handled-state save/restore,
  traceback attachment points, and roots for every pending completion payload.
- Cover class bodies, comprehensions, callbacks, descriptors, iterators, and
  recursive native calls without host unwinding, `setjmp`, or `longjmp`.

## Completion proof

Public CPython differentials pin handler order, side effects, final exception
identity, cause/context graphs, traceback shape, group behavior, and every
cleanup transfer under forced collection.
