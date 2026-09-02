# Gate 4 Slice 7 — Expanded Calls with `*` and `**`

## Goal

Complete call-site positional and keyword expansion through the authoritative
runtime binder.

## Implementation

- Preserve source-order call parts instead of splitting all positionals and
  keywords into independent evaluation lists.
- Evaluate the callable and every argument expression exactly once,
  left-to-right.
- Expand each `*iterable` through generic iteration.
- Expand each `**mapping` through the generic mapping protocol and require
  string keys.
- Detect duplicate keywords across explicit and expanded arguments at the same
  point CPython does, before entering the callee.
- Feed the resulting descriptor to the existing binder; do not implement a
  second argument-binding algorithm.
- Root callable, accumulated arguments, names, and mapping values across every
  allocation/callback.

## Public fixtures

- Interleaved expansions with visible side effects.
- User iterables/mappings, duplicate keys, non-string keys, and expansion
  failures.
- Full parameter kinds, bound methods, callable instances, and class calls.

## Completion evidence

- Syntax/HIR keep a source-ordered `CallPart` stream for positional, starred,
  explicit keyword, and keyword-unpack arguments. The callable is evaluated
  once; each part is evaluated and expanded before the next source expression.
- Expanded calls allocate a traced internal call-argument accumulator through
  `rimera_call_arguments_new`, append/expand parts through
  `rimera_call_argument_add`, and finish with `rimera_call_prepared`. Final
  parameter binding remains exclusively in the pre-existing authoritative
  `call::invoke`/`rimera_call` binder.
- `*iterable` uses generic iteration; `**mapping` uses `keys()` plus generic item
  access and requires string keys. Duplicate explicit/expanded keywords are
  rejected before callee entry at CPython's callback point. Non-string mapping
  keys intentionally fetch their mapping value before the final `TypeError`,
  matching CPython 3.12 ordering.
- The accumulator traces its callable and every accumulated value. Its
  `managed_size()` charges retained positional/keyword vector capacity and
  keyword-string capacity; mutating ABI returns refresh managed bytes before
  heap-limit enforcement.
- `gate4_expanded_calls_match_cpython_312` is byte-for-byte green for
  interleaved side effects, user iterables/mappings, duplicate and non-string
  keys, expansion failures, positional-only/keyword-only/varargs/varkw binding,
  bound methods, callable instances, and class calls. The former Slice 7
  capability fixture is a positive smoke.
- Runtime proofs `gate4_prepared_call_arguments_trace_callable_and_accumulated_values`
  and `gate4_prepared_call_argument_growth_refreshes_managed_bytes_and_heap_limit`
  establish traced lifetime plus deterministic managed-size/heap-limit behavior.

## DONE BY CHATGPT
