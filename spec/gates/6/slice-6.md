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
