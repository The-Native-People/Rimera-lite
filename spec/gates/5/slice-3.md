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

## Completion proof

Public differentials exercise the complete supported signature matrix,
evaluation logs, failures, recursion, reentrancy, forced collection, and
traceback frame ordering through the public build API.
