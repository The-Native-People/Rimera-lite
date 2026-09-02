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
`Implemented — conformance audit pending`. Do not imply completion of Gate 7
reflection, imports/modules, dynamic code, async, stdlib, or PyPI compatibility.
