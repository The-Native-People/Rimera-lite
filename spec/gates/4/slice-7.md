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
