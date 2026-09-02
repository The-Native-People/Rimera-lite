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

## Completion evidence

- `lower::tests::gate4_match_cfg_commits_only_on_success_and_roots_subject_across_callbacks`
  proves the subject is evaluated once, equality callbacks root it, failed
  pattern edges publish no capture, successful capture stores precede guards,
  and guard callbacks retain the subject for later cases.
- `sema::tests::match_capture_sets_reject_duplicates_inconsistent_or_and_unreachable_alternatives`
  proves duplicate captures, inconsistent OR binding sets, and unreachable
  irrefutable OR alternatives fail semantically with source spans.
- Ordinary MIR verification/safepoint planning validates every generated match
  successor and callback root set; the public Slice 20 fixtures below exercise
  the same CFG through Cranelift rather than hand-built MIR.

## DONE BY CHATGPT
