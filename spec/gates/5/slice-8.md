# Gate 5 Slice 8 — Final Audit, Documentation, and Gate Closure

## Goal

Promote Gate 5 only after Slices 1–7 are complete through public native proof.

## Completion audit

- Search for Gate 5 capability diagnostics, ignored tests, placeholders, dead
  ABI exports, duplicate binders/cell models, parser-only forms, runtime-only
  helpers, host unwinding, and stale partial claims.
- Run the full function/signature/scope/exception differential corpus through
  the public build API, including forced-GC and heap-limit variants.
- Run formatting, warning-denied workspace clippy, workspace tests, doc tests,
  release build, `git diff --check`, release-size proof, and forbidden-symbol
  scans once at the gate boundary.
- Reconcile `TODO.md`, `GATES.MD`, `spec/compatibility.md`, `spec/abi-v1.md`,
  and `AGENTS.md` with exact test counts and remaining boundaries.

## Promotion rule

Only after the audit is green, mark all Gate 5 slices complete and promote
exceptions, function calling, and LEGB/scopes/closures to
`Implemented — conformance audit pending`. Do not imply completion of imports,
dynamic code, async, stdlib, PyPI, or general Python 3.12 conformance.

## Acceptance evidence — 2026-09-04

- Gate 5 focused public tests and the broader exception corpus are green against
  CPython 3.12.11, including the repaired injected-generator-resume liveness
  edge. Gate 6's six suspension/delegation public regressions remain green.
- Final workspace boundary: 304 tests pass with zero failures or ignored tests
  (3 ABI + 8 CLI + 42 compiler unit + 181 public native-pipeline + 70 runtime).
  `cargo fmt --check`, warning-denied workspace clippy, and workspace doc tests
  pass.
- `cargo build --release --workspace` passes. `scripts/verify_release.sh`
  produces a 503,288-byte native `hello`, below the 512 KiB ceiling, with the
  forbidden CPython/generated-C/`setjmp`/`longjmp` scan clean.
- Compatibility rows 1–3 and the trackers are promoted only at this final Gate 5
  boundary; Gate 7 then performs its own separate reflection promotion.

## DONE BY CHATGPT
