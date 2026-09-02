# Gate 6 Slice 1 — Ownership, ABI Baseline, and Differential Matrix

## Goal

Freeze the proven generator-expression foundation and define the exact source,
state-machine, ABI, runtime, and proof delta for general generators.

## Integrated work

- Inventory Python 3.12 `yield`, yield expressions, `yield from`, generator
  methods, return values, exception injection, close, cleanup, and invalid
  placement forms.
- Audit the existing generator object, resume operation/outcome enums, state and
  slot accessors, delegate/handled/terminal fields, MIR `Yield`, liveness, and
  Cranelift resume dispatcher. Remove stale or duplicate contracts.
- Define stable semantics for resume input, injected exceptions, state IDs,
  terminal completion, persistent slots, delegate ownership, and saved handled
  state before implementation lanes fan out.
- Build a CPython 3.12.11 differential matrix and assign every gap to Slices
  2–8. Async generators/await retain later-gate diagnostics and emit no artifact.
- Pin Gate 4 generator-expression laziness, closure capture, exception behavior,
  repeated exhaustion, and forced-GC proof.

## Completion proof

ABI layout tests, MIR/verifier baseline tests, public generator-expression
regressions, and invalid-source diagnostics are green. The matrix names one
owner and one slice for every unresolved synchronous generator behavior.
