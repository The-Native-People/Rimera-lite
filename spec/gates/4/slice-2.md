# Gate 4 Slice 2 — Recursive Assignment-Target Model

## Goal

Create one recursive assignment-target representation used by assignment,
loop targets, comprehensions, deletion, and named expressions.

## Implementation

- Add an owned target tree covering name, attribute, item, tuple/list nesting,
  and starred children with source spans.
- Resolve binding targets in semantics without treating them as value reads.
- Preserve evaluation rules for attribute receivers and item receiver/index.
- Add HIR target validation: at most one star per sequence level, no illegal
  expression targets, and context-specific restrictions.
- Add MIR target-write planning with explicit fallible stores and exception
  successors. Do not add a runtime AST walker.
- Verify uses, definitions, cleanup edges, and safepoint roots for receiver,
  index, source iterable, and partially unpacked values.

## Proof

- HIR tests pin recursive trees and precise invalid-target diagnostics.
- MIR tests pin single receiver/index evaluation and exception edges.
- Existing simple assignment and flat unpacking public fixtures remain green.

## Completion evidence

- Syntax and HIR now share one recursive target tree for names, attributes,
  items, tuple/list sequences, and starred children; every node retains its
  source span and HIR name leaves carry resolved binding ownership.
- Scope scanning treats target bindings as writes while still treating
  attribute receivers and item receiver/index expressions as ordinary reads.
- Semantic validation owns target-tree legality and rejects multiple direct
  stars at one sequence level before MIR with `RIM-CAP-G4-02`.
- MIR uses one recursive target-write planner. Plain assignment evaluates its
  RHS once before target expressions; attribute/item writes evaluate receiver
  and index once and preserve fallible exception edges. Augmented assignment
  and deletion reuse the same target ownership without a runtime AST walker.
- `lower::tests::gate4_target_write_planner_evaluates_rhs_receiver_and_index_once_with_exception_edge`
  pins the MIR ordering and exception successor. `gate4_recursive_target_baseline_matches_cpython_312`
  proves chained assignment and the pre-existing flat/trailing-star subset
  byte-for-byte against CPython 3.12.11 through a native-only artifact.

## Proof

Compiler target/sema/MIR tests, the public target baseline, and all pre-Gate-4
native regressions pass as part of the 193-test workspace boundary.

## DONE BY CHATGPT
