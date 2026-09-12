# Gate 5 Slice 3 — Authoritative Binding and Activation Isolation

## Goal

Close function-call behavior through the existing binder and prove independent
native activations under recursion, reentrancy, and failure.

## Integrated work

- Audit every Python 3.12 parameter kind and the supported positional,
  keyword, `*iterable`, and `**mapping` call shapes against the one native
  binder.
- Preserve left-to-right evaluation, duplicate detection, non-string keyword
  errors, positional-only names in `**kwargs`, and CPython-shaped binding
  failures before callee entry.
- Prove defaults, keyword defaults, variadic containers, callable objects,
  bound methods, class calls, decorated callables, and recursive calls use the
  same binding contract.
- Prove activation-local values, roots, handled-exception state, and tracebacks
  remain independent under direct recursion, mutual recursion, callback
  reentrancy, and failures during argument expansion.
- Verify every fallible expansion/binding/call edge and exact safepoint root set.

## Implemented contract

- The existing `call::bind`/`rimera_call` path remains authoritative for every supported parameter kind and direct/prepared invocation; no second binder or activation representation was added.
- Expanded positional/keyword arguments keep their Gate 4 prepared-call evaluation order and enter the binder only after expansion succeeds. Callable instances, bound methods, classes, decorated functions, recursion, and mutual recursion all converge on the same generic `call::invoke` path.
- Bare `raise` is no longer rejected by lexical sema state. It reaches the runtime handled-exception stack, which correctly supports reraising from a called native function while an outer handler is active and reports the runtime error only when no handled exception exists.
- Native activation roots remain call-local; recursion and callback reentrancy cannot alias argument/bound-local storage from another activation.

## Completion proof

- `gate5_slice3_authoritative_binding_and_activation_isolation_match_cpython_312` runs under a 65,536-byte managed heap limit and matches CPython 3.12.11 for positional-only/default/keyword-only/variadic binding, prepared `*`/`**` order, callable instances, bound methods, class construction, decorators, recursion, mutual recursion, callback reentrancy, failed expansion before callee entry, binding failures, and handled-exception reraising across calls.
- The consolidated public `gate5_slice*` run keeps the earlier function/default/decorator behavior green while exercising Slice 3 through the public build API and native-only artifact assertions.

## Acceptance evidence — 2026-09-02

- Oracle: `/opt/homebrew/bin/python3.12 --version` -> `Python 3.12.11`.
- Focused Gate 5 public run: 8 passed, 0 failed, including the Slice 3 binder/activation differential.
- Integration boundary: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `cargo test --doc --workspace`, and `git diff --check` all pass.
- Full workspace test total: 262 passed, 0 failed, 0 ignored (3 ABI + 8 CLI + 40 compiler unit + 153 public native-pipeline + 58 runtime).
- At this Slice 3 boundary promotion remained Slice 8-owned; the later Slice 8
  audit closed Gate 5. Gates 7, 8, and Gate 9 Slice 1 have also closed; Slice 2 is the
  current active implementation tracker.

## DONE BY CHATGPT
