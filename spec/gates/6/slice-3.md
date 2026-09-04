# Gate 6 Slice 3 — Yield/Resume CFG, Values, and Persistent Liveness

## Goal

Compile multiple suspension points and yield expressions into a verified native
resume state machine with exact persistent state.

## Integrated work

- Lower source `yield value` and bare `yield` to explicit MIR suspension
  terminators with stable continuation states and resume-input values.
- Support multiple yields in branches, loops, nested expressions, and ordinary
  function control flow without interpreting MIR at runtime.
- Extend verifier checks for generator-only use, state uniqueness, continuation
  validity, definition dominance after resume, slot consistency, and terminal
  paths.
- Compute exact values live across each suspension, save them before publishing
  the next state, restore them on resume, and clear obsolete slots promptly.
- Emit a Cranelift resume dispatcher and compiled continuation blocks using only
  the documented opaque generator ABI.

## Completion proof

MIR/verifier negative tests, generated-code tests, public multi-yield/send-value
fixtures, closure mutation, loop control, forced collection, and slot-liveness
assertions match CPython 3.12.11.

## Completion evidence — 2026-09-03

- MIR `Yield` records yielded value, optional resume-value SSA destination, normal continuation, injected-exception successor, and optional delegate; verifier checks generator ownership, targets, parameters, and delegate values.
- Generated Cranelift saves only liveness-selected persistent values before suspension and restores them before the compiler-selected continuation; `x = yield y` receives the exact `send(x)` value.
- `mir::tests::generator_yield_verification_and_persistent_liveness_are_explicit` and the constrained-heap source-generator differential are green.

## DONE BY CHATGPT
