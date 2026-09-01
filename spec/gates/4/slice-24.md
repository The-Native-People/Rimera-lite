# Gate 4 Slice 24 — Audit, Documentation, and Gate Closure

## Goal

Close Gate 4 only after Slices 1–23 are end-to-end complete and verified.

## Completion audit

- Audit every Python 3.12 AST form named by this directory. Each included form
  must execute through syntax/HIR/MIR/Cranelift/runtime/GC/public proof.
- Search for Gate 4 capability diagnostics, ignored tests, placeholder panics,
  dead ABI exports, parser-only nodes, runtime-only helpers, and alternate
  execution paths. Resolve every hit or assign it to a genuinely later gate
  with an exact reason.
- Confirm generator expressions execute natively; a Gate 6 label is not enough.
- Confirm no runtime AST evaluator, bytecode, generated C, or CPython embedding
  was introduced.

## Required verification

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo test --doc --workspace
cargo build --release
git diff --check
```

- Compile/run the Gate 4 corpus through the real public build API.
- Compare representative stdout, stderr, and exit status with
  `/opt/homebrew/bin/python3.12` version 3.12.11.
- Scan final artifacts for CPython, generated-C, `setjmp`, and `longjmp`.
- Verify heap-limit regressions and the documented release-size budget.

## Tracking

Only after all checks pass:

- Mark Gate 4 complete in `TODO.md`.
- Update `spec/compatibility.md`, `spec/abi-v1.md`, and `AGENTS.md` with exact
  proof paths and remaining conformance-only gaps.
- Mark comprehensions/unpacking and included synchronous syntax as
  `Implemented — conformance audit pending`.
- Make native generators the sole next active gate, limited to source `yield`,
  `send`, `throw`, `close`, and `yield from` behavior not already required by
  executable generator expressions.
- Do not claim general Python 3.12, stdlib, module, async, or PyPI compatibility.
