# Gate 5 Slice 4 — Complete LEGB, Cells, and Scope Interactions

## Goal

Close compile-time binding and runtime cell behavior across all synchronous
scope kinds already owned by Rimera.

## Integrated work

- Complete local/global/free/cell classification for modules, functions,
  lambdas, comprehensions, generator expressions, class bodies, annotations,
  pattern bindings, and nested declarations.
- Enforce Python declaration conflicts and unbound-local/free-variable errors
  with stable spans and CPython-compatible observable behavior.
- Complete `global`, `nonlocal`, deletion, rebinding, shadowing, and shared-cell
  mutation across sibling closures and recursive activations.
- Preserve the class-namespace lookup rules, `__class__` cells, comprehension
  isolation, walrus binding rules, and the rule that methods do not close over
  ordinary class locals.
- Prove cell/function/class/generator cycles are traced and reclaimed without
  retaining dead activation state.

## Implemented contract

- Class suites now receive their own semantic scope plan instead of borrowing the enclosing module/function plan. `ClassName` represents namespace-first/global-builtin fallback and `ClassFree` represents namespace-first/enclosing-cell fallback; explicit class `global` bypasses the namespace and explicit class `nonlocal` reads/writes/clears the enclosing cell.
- `rimera_class_free_get` is the documented native ABI operation for class free-variable reads. Class assignments, deletes, augmented assignments, nested `def`/`class`, and `except ... as` bindings use the resolved binding kind instead of unconditional class-namespace mutation.
- Function-local class definitions are supported through the existing hidden native class-body path. Methods still do not capture ordinary class locals; the one synthetic class-child binding is `__class__`, shared by explicit `__class__` references and zero-argument `super()` through the existing `__classcell__` path.
- Declaration diagnostics for global/nonlocal conflicts, parameter/declaration conflicts, and missing nonlocal bindings retain the declaration statement span and stop before artifact emission.

## Completion proof

- `gate5_slice4_scope_matrix_matches_cpython_312_under_gc_pressure` runs at a 96,000-byte heap limit and matches CPython 3.12.11 across function-local classes, class global/nonlocal, namespace/enclosing shadowing, explicit `__class__`, `super()`, class/nested comprehensions, walrus ownership, sibling-cell mutation, deletion/rebinding, annotation-created locals, recursive activations, match bindings, and generator-expression closure reads.
- `gate5_slice4_declaration_conflicts_have_stable_spans_and_emit_no_artifact` proves three declaration-conflict families return `RIM-SEMA-001`, carry non-empty source spans, and create no executable artifact.

## Acceptance evidence — 2026-09-02

- Oracle: `/opt/homebrew/bin/python3.12 --version` -> `Python 3.12.11`.
- Focused proof: the 96,000-byte scope matrix and all declaration-conflict/no-artifact cases pass; the consolidated Gate 5 public run reports 8 passed, 0 failed.
- `spec/abi-v1.md` documents `rimera_class_free_get` and the distinct class-name/global/nonlocal/free-cell resolution contracts.
- Integration boundary: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `cargo test --doc --workspace`, and `git diff --check` all pass.
- Full workspace test total: 262 passed, 0 failed, 0 ignored (3 ABI + 8 CLI + 40 compiler unit + 153 public native-pipeline + 58 runtime).
- Gate 5 is not promoted; compatibility/TODO promotion remains Slice 8-owned and Gate 6 remains the sole active implementation tracker.

## DONE BY CHATGPT
