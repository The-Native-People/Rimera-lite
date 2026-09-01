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
