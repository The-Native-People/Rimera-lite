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
