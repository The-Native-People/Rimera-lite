# Gate 4 Slice 20 — Literal, Capture, Wildcard, OR, and AS Patterns

## Goal

Implement the non-container core of structural pattern matching.

## Implementation

- Support literal and singleton patterns using Python equality/identity rules.
- Support capture and wildcard patterns without treating `_` as a binding.
- Support `pattern as name` with transactional binding.
- Support OR patterns left-to-right and require identical capture-name sets.
- Evaluate guards only after successful pattern binding.
- Preserve exceptions raised by value equality or guards.

## Public fixtures

- Every pattern form, nested OR/AS combinations, guard success/failure,
  side-effect ordering, duplicate/inconsistent capture diagnostics, and GC of
  tentative managed bindings.

## Completion evidence

- `gate4_match_core_matches_cpython_312` covers value/literal equality,
  `None`/boolean singleton identity, capture, `_` non-binding behavior, OR and
  nested OR/AS, identical-name OR captures, successful/false guards,
  commit-before-guard visibility, local captures, subject-once ordering, and
  equality/guard exception propagation against CPython 3.12.11.
- `gate4_match_tentative_bindings_survive_forced_gc` runs value comparisons and
  both false/successful guards under a 32 KiB managed-heap limit, proving failed
  patterns do not overwrite existing bindings while committed guard bindings
  and the subject survive collection pressure.
- `gate4_match_semantic_failures_are_stable_and_emit_no_artifact` proves
  duplicate capture, inconsistent OR-binding, and unreachable-alternative
  failures retain `RIM-SEMA-001`, source spans, and no emitted artifact.
- `gate4_owned_later_slice_forms_have_stable_capability_diagnostics` keeps
  sequence/mapping and class patterns explicitly owned by Slices 21/22 with
  `RIM-CAP-G4-21`/`RIM-CAP-G4-22` and no fallback artifact.
- Public match artifacts use the same native-only forbidden-symbol scan as the
  rest of the Gate 4 differential corpus.

## DONE BY CHATGPT
