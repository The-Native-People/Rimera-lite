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
