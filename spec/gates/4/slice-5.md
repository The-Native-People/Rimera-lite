# Gate 4 Slice 5 — Destructuring in `for` and Comprehensions

## Goal

Use the recursive target model for loop binding and prepare the same contract
for comprehension clauses.

## Implementation

- Replace name-only loop binding with the Slice 2 target plan.
- Run unpacking on every iteration before the body.
- Preserve `break`, `continue`, `else`, nested-loop cleanup, and exception
  propagation when target binding fails.
- Ensure a failed target bind does not execute the body for that iteration.
- Expose a reusable HIR/MIR clause-target lowering interface for Slices 15–18.

## Public fixtures

- Exact, starred, and nested loop targets over native and user iterables.
- Binding failures inside `try`/`finally` and loops with `else`.
- Forced collection between iteration and body entry.

## Completion criteria

Ordinary `for` destructuring is fully executable; comprehension execution is
left to Slices 15–18 using this exact target contract.

## Completion evidence

- Ordinary and class-body `for` loops now consume the same resolved recursive
  HIR target tree used by assignment. Exact, starred, nested, attribute, and
  item leaves therefore reuse the established target write/unpack semantics.
- Binding occurs after `IteratorNext` succeeds and before the body. A failed
  target bind takes the active exception successor, so that iteration's body is
  skipped while existing `break`, `continue`, loop `else`, nested cleanup, and
  `try`/`finally` behavior remain unchanged.
- `write_clause_target` is the reusable clause-target lowering entry point for
  the later comprehension slices; class-suite loops use the corresponding
  namespace-aware recursive target writer rather than a name-only side path.
- `gate4_for_destructuring_matches_cpython_312` compares exact, starred, nested,
  user-iterator, class-suite, failure, `else`, `break`, `continue`, and
  `finally` behavior byte-for-byte with CPython 3.12.11. The former
  `gate4_cap_for_unpack.py` denial is also compiled as a positive promotion
  smoke.
- `gate4_loop_destructuring_roots_yielded_item_and_iterator_at_unpack_safepoint`
  proves the yielded managed item and loop iterator are shadow-rooted at the
  fallible target `Unpack` safepoint and that the bind has an explicit
  exception successor. Slice 4's forced low-heap unpack regression provides the
  runtime collection half of that iteration-to-body GC boundary.

## Proof

The final Slice 5–7 boundary passes 200/200 workspace tests with zero ignored,
warning-denied clippy, `git diff --check`, and the release verifier at 485,144
bytes.

## DONE BY CHATGPT
