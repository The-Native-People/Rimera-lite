# Gate 6 Slice 9 — Final Audit, Documentation, and Gate Closure

## Goal

Promote Gate 6 only after Slices 1–8 are complete through public native proof.

## Completion audit

- Search for Gate 6 diagnostics, ignored tests, placeholders, parser-only
  `yield`, runtime-only helpers, dead ABI exports, duplicate generator models,
  interpreted resume/cleanup paths, and stale foundation-only claims.
- Run the complete synchronous generator corpus through the public build API,
  including forced-GC and heap-limit variants against CPython 3.12.11.
- Run formatting, warning-denied workspace clippy, workspace tests, doc tests,
  release build, `git diff --check`, release-size proof, and forbidden-symbol
  scans once at the gate boundary.
- Reconcile `TODO.md`, `GATES.MD`, `spec/compatibility.md`, `spec/abi-v1.md`,
  and `AGENTS.md` with exact proof paths, counts, and remaining boundaries.

## Promotion rule

Only after the audit is green, mark all Gate 6 slices and Gate 6 complete and
promote generators from `Foundation only` to
`Implemented — conformance audit pending`. Do not claim async generators,
async iteration, Gate 5's broader closure audit, reflection, imports/modules,
stdlib, or PyPI compatibility.
