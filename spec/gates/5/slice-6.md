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

## Implemented closure

- The existing explicit MIR exception and cleanup CFG remains authoritative for
  reraising, causes/context/suppression, `except*`, handler cleanup, and
  return/break/continue/exception completion.
- Generator persistence now includes values live on both the normal resume edge
  and the injected-exception edge of each `Yield`. A caught exception copied to
  another local therefore remains the same managed value when `throw()` enters a
  suspended handler; no second generator or exception state model was added.
- The existing exception-group split/combine path, traceback attachment rules,
  and Gate 6 cleanup suspension remain unchanged and differentially proven.

## Acceptance evidence — 2026-09-04

- `gate5_slice6_handler_capture_survives_generator_suspension_and_cleanup`
  reproduces the injected-resume local-liveness regression under a 96,000-byte
  managed heap and now matches CPython 3.12.11.
- `gate5_slice6_propagation_chaining_groups_and_cleanup_match_cpython_312` runs
  `exceptions.py`, `exception_control_flow.py`, `exception_groups.py`,
  `gate6_throw_close.py`, and `gate6_cleanup_suspend.py` under a 128,000-byte
  heap limit with exact public differential comparison.
- `mir::tests::generator_yield_verification_and_persistent_liveness_are_explicit`
  pins persistent values required only by an injected-exception successor.

## DONE BY CHATGPT
