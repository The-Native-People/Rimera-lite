# Gate 3 Slice 1 — Finish Half-Written Set Update Methods

## Goal
Restore the repository to a compiling state and complete the mutable set update-method surface that was started but not wired through the runtime.

## Remaining implementation
- Wire `BuiltinFunctionKind::SetIntersectionUpdate` into builtin-call dispatch.
- Wire `BuiltinFunctionKind::SetDifferenceUpdate` into builtin-call dispatch.
- Wire `BuiltinFunctionKind::SetSymmetricDifferenceUpdate` into builtin-call dispatch.
- Expose `set.intersection_update(...)` through `attribute_get`.
- Expose `set.difference_update(...)` through `attribute_get`.
- Expose `set.symmetric_difference_update(...)` through `attribute_get`.
- Implement all three mutation bodies using the existing generic set/hash/equality machinery.
- Preserve the identity of the receiving mutable set.
- Return `None` exactly as CPython does.
- Support iterable operands using the established iterator protocol.
- Propagate hash/equality/iteration failures without leaving the target set in an invalid internal state.

## Completion criteria
- `cargo check --workspace` compiles past the currently unmatched enum variants.
- All three methods mutate the receiver rather than replacing it.
- Immutable `frozenset` does not expose these methods.

## Out of scope
- Comprehension syntax and other Gate 4 syntax work.

## DONE BY CHATGPT
