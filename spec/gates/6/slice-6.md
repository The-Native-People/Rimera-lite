# Gate 6 Slice 6 — Suspension Through Handlers and Cleanup Regions

## Goal

Preserve Python completion and exception semantics when suspension occurs inside
or across structured exception and cleanup regions.

## Integrated work

- Persist compiler-planned pending return/break/continue/exception completion,
  active handler state, cleanup cursor, and all live values across suspension.
- Support yields in `try`, handlers, `else`, and `finally`, including nested
  regions, loops, reraises, and cleanup transfers that replace pending work.
- Ensure `throw` enters at the exact suspension point and `close` executes all
  required `finally` cleanup in inner-to-outer order.
- Preserve traceback frames and handled exceptions while a generator is
  suspended, restore the caller's handled state around resume, and release
  saved graphs after completion.
- Verify every suspension and cleanup edge plus exact roots; no host unwinding,
  runtime MIR evaluation, or generator-specific cleanup interpreter is allowed.

## Completion proof

Public differentials pin yield/throw/close behavior, side-effect order, final
exception identity, traceback frames, loop transfers, nested cleanup, forced
GC, and abandonment/collection cases.

## Completion evidence — 2026-09-03

- The generator frame now traces its own pending `raised` exception in addition to saved handled-exception state, preventing exceptions pending through `finally` suspension from leaking into the caller.
- Compiler-planned cleanup CFG preserves pending exception and return completions across yields; `throw`/`close` re-enter the exact suspension successor and execute nested cleanup in compiled control flow.
- `gate6_cleanup_suspension_matches_cpython_312_under_gc_pressure` proves pending exceptions, handler/finally yields, close cleanup, and return-through-finally behavior under a constrained heap.

## DONE BY CHATGPT
