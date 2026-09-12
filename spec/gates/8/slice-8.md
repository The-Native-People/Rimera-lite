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

## Completion evidence

- The final corpus passes 317 tests with zero failures and zero ignored tests:
  3 ABI, 8 CLI, 44 compiler-unit, 190 public native-pipeline, and 72 runtime.
- Warning-denied workspace clippy, doc tests, formatting, diff hygiene, release
  build, release execution, the 512 KiB size ceiling, and forbidden-symbol scans
  pass. The audited macOS arm64 release `hello` is 503,544 bytes.
- Gate 8 is closed; Gate 9 Slice 1 has since closed and Slice 2 is active.
