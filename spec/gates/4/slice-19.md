# Gate 4 Slice 19 — Pattern-Matching Control-Flow Foundation

## Goal

Add the transactional CFG and binding model required by Python 3.12 `match`.

## Implementation

- Evaluate the subject exactly once and root it across all cases.
- Represent ordered cases, patterns, optional guards, and bodies in syntax/HIR.
- Compile each pattern to success/failure CFG with tentative bindings.
- Commit bindings before a successful guard/body; discard tentative bindings
  when a pattern fails. Preserve CPython-observable guard behavior.
- Continue to the next case on pattern or guard failure; execute at most one
  body.
- Diagnose duplicate captures, unreachable alternatives, and inconsistent OR
  bindings during semantics.
- Add MIR verifier coverage for binding sets, successor completeness, and roots.

## Proof

Hand-built MIR is only foundation proof. Slice 20 must provide the first public
native `match` fixture before this foundation is considered exercised.
