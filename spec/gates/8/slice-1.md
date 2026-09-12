# Gate 8 Slice 1 — Ownership, Syntax, Cleanup Contract, and Differential Matrix

## Goal

Define the complete synchronous `with` source and cleanup contract before
adding execution behavior.

## Integrated work

- Inventory single/multiple `with` items, parenthesized forms, supported target
  shapes, nesting, all completion reasons, generator suspension, and invalid or
  async forms in Python 3.12 syntax.
- Establish one syntax/HIR owner and extend MIR cleanup actions with an explicit
  context-exit plan carrying manager methods, entered values, exception state,
  and pending completion.
- Specify special-method lookup, descriptor binding, evaluation/entry/exit
  order, exception triples, suppression, replacement, and partial-entry rules.
- Define verifier invariants and exact roots for every enter/body/exit/cleanup
  edge before compiler/runtime lanes fan out.
- Build a CPython 3.12.11 matrix and assign every gap to Slices 2–7. `async
  with` and stdlib helpers retain later-gate diagnostics and emit no artifact.

## Completion proof

Syntax/HIR/MIR ownership and verifier tests are green, public negative fixtures
have stable spans/codes, and every synchronous `with` outcome has one planned
cleanup path and later slice.

## Status

Complete. Rimera-owned syntax and HIR preserve ordered items, assignment
targets, bodies, and source spans; `async with` remains the stable
`RIM-CAP-G8-01` no-artifact boundary. MIR owns captured special methods,
exception-state operations, and `ContextExit` cleanup actions. The lowering
verifier/root test proves target failures enter cleanup and captured exits stay
live across normal and exceptional call safepoints.
