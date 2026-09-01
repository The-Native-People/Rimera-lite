# Gate 3 Slice 23 — Final Acceptance and Release Checks

## Goal
Prove Gate 3 is actually complete at the repository/release level after all implementation slices are finished.

## Required commands
Run from the repository root and require a clean pass:

```text
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo test --doc --workspace
git diff --check
cargo build --release
```

## Required acceptance checks
- Confirm `/opt/homebrew/bin/python3.12` is exactly CPython 3.12.11 before relying on differential output.
- Run the public synchronous Gate 3 native corpus through the real build API.
- Run representative CPython stdout/stderr/exit-code differentials for the completed Gate 3 surface.
- Confirm unsupported later-gate syntax still fails before producing a native artifact.
- Run the existing GC/lifetime/low-heap proofs with the completed builtin families.
- Verify the release hello executable is `<= 512 KiB` as required by `GATES.MD`.
- Scan release/native artifacts for forbidden CPython/runtime embedding symbols.
- Confirm no generated C pipeline has reappeared.
- Confirm no `setjmp`/`longjmp`-based Python exception machinery.
- Confirm no ignored tests used to hide Gate 3 failures.
- Search for `todo!()` / `unimplemented!()` / placeholder panics in Gate 3-owned execution paths.
- Audit dead ABI exports and ensure documented ABI operations are still used/reachable as intended.
- Confirm builtin types/functions remain lazy enough to preserve heap-limit and release-size behavior.

## Repository state issue to resolve
A prior audit observed `git status --short` reporting the entire repository as untracked. Before using diff-based acceptance or `task.finish`, determine why Git does not see a tracked baseline and restore/confirm the intended repository state. Do not claim `git diff --check` or change counts are meaningful until this is resolved.

## Completion criteria
Every required command and acceptance check is green on the final Gate 3 implementation. Any failure returns work to the owning earlier slice rather than being waived here.

## Completed implementation

- `/opt/homebrew/bin/python3.12 --version` reports exactly `Python 3.12.11`; the
  Gate 3 public differentials therefore use the required oracle.
- The repository's Git issue was an unborn `main` branch with no index or HEAD
  commit, not damaged source. The immutable VibePlus cycle baseline tree
  `50890fb0b40d180164901433e9de0f404f85d05e` contains 224 repository files and
  was restored into the Git index with `git read-tree` only. No working file was
  replaced and no commit was created; `git diff`/`git diff --check` now compare
  the working tree against the captured pre-cycle baseline meaningfully.
- Release reachability now declares only ABI functions required by verified MIR.
  A semantically proven unshadowed builtin `print("literal")` lowers to the
  runtime-owned `rimera_print_literal` path; rebound, aliased, local/free,
  keyword, and nonliteral print calls retain the ordinary Python call/print
  paths. Release linking dead-strips and fully strips the final custom-linked
  Mach-O.
- `scripts/verify_release.sh` passes end to end: the release executable prints
  exactly `hello`, is **485,064 bytes** (`<= 524,288`), and its symbol scan finds
  no forbidden `rv_`, `Py_`, `PyObject`, `setjmp`, or `longjmp` symbols.
- `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`,
  `cargo test --workspace`, `cargo test --doc --workspace`, `git diff --check`,
  and `cargo build --release` all pass on the final implementation. The workspace
  suite passes 3 ABI + 8 CLI + 12 compiler-unit + 111 public native-pipeline +
  46 runtime tests = **180/180**, with **0 ignored**; the iterative deep-graph GC
  test, public low-heap regressions, and unsupported-no-artifact tests are green.
- Static acceptance finds no generated `.c`/`.h` pipeline, no production
  `setjmp`/`longjmp`, no `todo!()`/`unimplemented!()` placeholders, and no Gate
  3 execution-path placeholder panics. The visible `panic!` sites are test-only
  under `#[cfg(test)]`.
- The ABI export audit distinguishes generated-code reachability from deliberate
  runtime helpers. Gate 3 managed helpers remain documented; memoryview release
  is runtime lifecycle support; structured-exception construction is runtime
  scaffolding; generator creation/resume exports are explicitly retained for
  Gate 6 and do not count as source-level generator completion.
- Lazy builtin/type startup remains proven within the public 8 KiB heap budget,
  while the final release artifact remains below the Gate 3 size ceiling.

## DONE BY CHATGPT
