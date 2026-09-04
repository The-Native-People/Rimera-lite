# Gate 5 Slice 1 — Ownership, Oracle, and Gap Matrix

## Goal

Establish the exact delta between the proven native function/scope/exception
kernel and the Gate 5 contract before changing representations.

## Integrated work

- Inventory Python 3.12 function forms, parameter shapes, decorators,
  annotations, scope declarations, cell transitions, raise forms, handlers,
  exception groups, and cleanup transfers.
- Map every behavior to its single syntax, HIR, semantic, MIR, ABI, runtime,
  and public-test owner. Remove or consolidate duplicate stage-local contracts.
- Build a CPython 3.12.11 differential matrix covering success, error type,
  message boundary, traceback order/position, side effects, and cleanup order.
- Classify each gap into Slices 2–7. Later-gate reflection, imports, dynamic
  code, and async behavior must retain stable capability diagnostics and emit
  no artifact.
- Pin the existing supported behavior so closure work cannot regress generator
  expressions, class bodies, comprehensions, descriptors, or structured
  exception cleanup.

## Ownership and gap matrix

| Behavior family | Single current owner | Proven/current boundary | Remaining owner |
|---|---|---|---|
| `def`/lambda syntax, decorators, annotations, `/`, `*args`, keyword-only, `**kwargs` | `syntax::convert_statement`, `convert_expression`, and `convert_parameters` | Owned syntax/HIR preserves every supported parameter kind, default, annotation, and decorator expression. | Slice 2 completes Python-visible construction metadata; Slice 3 audits the complete binding matrix. |
| Definition-time decorator/default/annotation evaluation and publication | `lower::Lowerer` using the single `MakeFunction` MIR path plus ordinary `Call`/`AttributeSet` | Decorator expressions are evaluated top-to-bottom, defaults/annotations once, decorators applied bottom-up, and the name is stored only after successful application. | Slice 2. |
| Function identity and live metadata | `runtime::FunctionObject` reached only by `rimera_function_new` and ordinary attribute ABI operations | `__name__`, `__qualname__`, `__annotations__`, positional defaults, keyword defaults, aliases, lambdas, methods, and nested qualified names share one managed object. Raw code addresses are never Python-visible. | Slice 2; broader signature/code/frame reflection remains Gate 7. |
| Argument expansion, binding errors, recursion/reentrancy, activation-local state | `runtime::call::bind` / `rimera_call` and Gate 4 prepared-call accumulator | One authoritative binder already handles the supported signature/call forms. | Slice 3 performs the exhaustive parameter/error/reentrancy/root audit. |
| Local/global/free/cell classification; `global`/`nonlocal`; deletion/rebinding; class/comprehension/walrus interactions; function-local class definitions | `sema::RawScope`/`Analyzer` plus MIR cell/global/class-name operations | The established kernel is public-differential for current supported scope forms; methods keep ordinary class locals out of closures. Function-local class definitions remain rejected before MIR. | Slice 4. |
| Exception values, hierarchy, user subclasses, raise normalization, `args`, traceback mutation API | managed `ExceptionObject`/`TracebackObject` and `RimeraContext` raise helpers | Current structured exceptions are managed and traced, but the complete Python-visible value/normalization surface is not claimed. | Slice 5. |
| Bare reraising, causes/context/suppression, exception groups/`except*`, handler cleanup, return/break/continue through cleanup | explicit MIR exception edges and `Lowerer` cleanup actions | Existing public differentials pin the substantial supported kernel without host unwinding. | Slice 6 performs the closure audit and completes remaining propagation/cleanup edge cases. |
| Cross-feature traceback fidelity, callback reentrancy, forced GC, heap-limit failure atomicity, forbidden-symbol audit | MIR safepoint plan + runtime tracing GC + public build API | Gate 4 composition and the existing function/exception corpus stay native-only and differential. | Slice 7. |
| Source `yield`/`send`/`throw`/`close`/`yield from` | existing generator MIR/ABI extension selected by the active tracker | Generator expressions are already native; general source generators are intentionally outside Gate 5. | Gate 6. |
| PEP 695/function reflection, signatures, `__code__`, frame/local inspection | no alternate runtime path; only stable internal native metadata is retained | Type-parameter syntax remains a no-artifact capability diagnostic; no raw machine pointer is exposed. | Gate 7. |
| Imports/modules, async syntax/protocols, and dynamic `compile`/`eval`/`exec` | no fallback implementation | Public negative boundaries remain capability failures with no artifact. | Explicit queued post-core import, async, and dynamic-compilation architectures in `TODO.md`/`GATES.MD`. |

## Completion proof

- `syntax::tests::gate5_function_surface_preserves_decorators_annotations_and_parameter_kinds`, `sema::tests::gate5_function_analysis_preserves_decorators_defaults_annotations_and_scope_ownership`, and `lower::tests::gate5_function_mir_uses_one_make_function_path_for_metadata_decorators_and_closures` pin the changed source/HIR/MIR ownership surface.
- `gate5_slice1_existing_function_scope_exception_kernel_remains_native_and_differential` reruns functions, lambdas, scopes, structured exceptions, cleanup transfers, and Gate 4 composition through the public build API against CPython 3.12.11.
- `gate5_slice1_later_gate_boundaries_are_stable_and_emit_no_artifact` pins async, imports, and PEP 695 as stable no-artifact boundaries.
- Every unresolved Gate 5 item above names exactly one later Slice 3–7 owner; generator/reflection/import/async/dynamic-code behavior is assigned to its explicit architectural gate rather than a generic support bucket.

## Acceptance evidence — 2026-09-02

- Oracle: `/opt/homebrew/bin/python3.12 --version` -> `Python 3.12.11`.
- Focused ownership proof: `cargo test -p rimera-compiler --lib gate5_function -- --nocapture` -> 3 passed, 0 failed.
- Public Slice 1 proof: both `gate5_slice1_*` tests pass, including CPython differentials, stable diagnostic spans/codes, native-only artifact assertions, and no-artifact later-gate failures.
- Integration boundary: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `cargo test --doc --workspace`, and `git diff --check` all pass. The workspace suite totals 257 tests: 3 ABI + 8 CLI + 40 compiler unit + 149 public native-pipeline + 57 runtime; 0 failed and 0 ignored.
- Gate 5 is not promoted and `TODO.md`/`spec/compatibility.md` remain unchanged; Gate 6 stays the sole active tracker as required until Slice 8 performs the single Gate 5 promotion.

## DONE BY CHATGPT
