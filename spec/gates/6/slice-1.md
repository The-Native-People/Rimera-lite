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

## Completion evidence — 2026-09-03

- The existing managed `GeneratorObject`, `RNativeGeneratorResume`, persistent-slot ABI, and Cranelift resume dispatcher remain the sole synchronous-generator model; source generators and Gate 4 generator expressions share them.
- CPython `/opt/homebrew/bin/python3.12` reports 3.12.11 and is the public differential oracle.
- Final Gate 6 acceptance is green: 269 workspace tests passed with 0 failed/ignored, warning-denied clippy and doc tests pass, and the release verifier emits a 485,176-byte `hello` with a clean forbidden-symbol scan.

## DONE BY CHATGPT
