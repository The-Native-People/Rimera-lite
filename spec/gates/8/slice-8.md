# Gate 8 Slice 8 — Final Audit, Documentation, and Gate Closure

## Goal

Promote Gate 8 only after Slices 1–7 are complete through public native proof.

## Completion audit

- Search for Gate 8 diagnostics, ignored tests, placeholders, parser-only
  `with`, runtime-only helpers, dead ABI exports, duplicate cleanup/method-lookup
  paths, host unwinding, and stale not-started claims.
- Run the complete synchronous context-manager corpus through the public build
  API against CPython 3.12.11, including generators, forced GC, and heap limits.
- Run formatting, warning-denied workspace clippy, workspace tests, doc tests,
  release build, `git diff --check`, release-size proof, and forbidden-symbol
  scans once at the gate boundary.
- Reconcile `TODO.md`, `GATES.MD`, `spec/compatibility.md`, `spec/abi-v1.md`, and
  `AGENTS.md` with exact proof paths, counts, and remaining boundaries.

## Promotion rule

Only after the audit is green, mark every Gate 8 slice and Gate 8 complete and
promote synchronous context managers to
`Implemented — conformance audit pending`. Do not claim `async with`, stdlib
`contextlib`, imports/modules, dynamic compilation, async, or PyPI compatibility.
