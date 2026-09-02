# Gate 4 Slice 11 — General Augmented Assignment, Deletion, and Assertions

## Goal

Close the remaining statement-target forms using existing protocol and cleanup
machinery.

## Implementation

- Complete name, attribute, and item augmented assignment for every supported
  in-place operator, evaluating receiver and index once.
- Generalize `del` to recursive target lists, attributes, and items while
  retaining Python left-to-right effects.
- Implement `assert test` and `assert test, message` through generic truth,
  lazy message evaluation, and a managed `AssertionError`.
- Preserve exception propagation and cleanup in loops, functions, class bodies,
  and `try`/`finally`.
- Reject illegal deletion and augmented targets during semantics.

## Proof

- Side-effect fixtures pin one-time target evaluation.
- Differential failures pin exception type/message and traceback position.
- MIR tests cover in-place result stores and delete/assert exception edges.

## Completion evidence

- Normal and class-body augmented assignment now share target-specific lowering
  for names, attributes, and items and always feed the existing generic
  `InPlace` protocol result back through the corresponding store. Receiver and
  item index expressions are evaluated once before the RHS/store sequence.
- Deletion accepts recursive target lists and executes their leaves left-to-right
  through namespace/global, attribute, and item deletion machinery; starred
  deletion remains rejected by semantic validation.
- `assert test` and `assert test, message` are owned source syntax. The test is
  truth-tested through the generic branch protocol; only the failure block loads
  `AssertionError` and evaluates the optional message, then raises the managed
  exception through the normal exception/cleanup path. `AssertionError` is a
  lazy builtin exception type like the existing runtime exception family.
- `gate4_augmented_deletion_and_assertions_match_cpython_312` is byte-for-byte
  green for one-time attribute/item target evaluation, recursive deletions,
  lazy assertion messages, user truth callbacks, nested `finally`, and class
  body name/attribute/item forms. The old assert capability fixture is now a
  positive smoke.
- `gate4_assert_failure_pins_cpython_type_message_and_frame_positions` verifies
  the uncaught failure has CPython's module/function frame line positions and
  `AssertionError: boom`; Rimera intentionally retains its existing compact
  traceback rendering rather than adopting CPython's source/caret presentation
  in this slice.
- `gate4_slice11_mir_pins_inplace_store_delete_and_lazy_assert_failure` pins the
  in-place result store, delete and inplace exception successors, dedicated lazy
  assertion-failure block, and assertion truth-branch roots.

## Final Slice 8-11 boundary

`cargo fmt --check`, warning-denied workspace clippy, `git diff --check`, and
all 212 workspace tests pass with zero ignored. Release verification passes at
485,144 bytes, below the 512 KiB gate budget.

## DONE BY CHATGPT
