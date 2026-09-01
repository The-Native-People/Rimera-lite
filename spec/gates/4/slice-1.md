# Gate 4 Slice 1 — Baseline and Syntax-Ownership Audit

## Goal

Establish an exact, tested baseline for every Python 3.12 AST form owned by
Gate 4 before changing representations.

## Implementation

- Inventory parser output for assignment targets, starred expressions,
  comprehensions, expanded calls, boolean operations, comparison chains,
  subscripts/slices, assertions, deletion, named expressions, annotations,
  formatted strings, and `match` patterns.
- Record the single Rimera syntax/HIR owner for each form and remove duplicate
  stage-local models.
- Add stable `RIM-CAP-*` diagnostics for forms whose later numbered slice is
  not active yet. Diagnostics must preserve source spans and emit no artifact.
- Pin the existing supported behavior so subsequent representation changes do
  not regress flat unpacking, calls, loops, exceptions, or attributes.
- Add an integration table test proving every owned AST form either compiles or
  maps to one specific Gate 4 slice diagnostic.

## Proof

- Parser/semantic tests cover all listed Python 3.12 AST node shapes.
- Public negative fixtures verify stable codes, spans, and no output artifact.
- Existing native pipeline tests remain green.

## Completion boundary

This slice owns the audit and diagnostics only. It does not earn implementation
status for syntax assigned to Slices 2–22.

## Completion evidence

- `syntax::tests::gate4_owned_syntax_has_stable_slice_diagnostics_and_spans`
  inventories the owned Python 3.12 statement/expression shapes and pins a
  stable `RIM-CAP-G4-*` owner plus non-empty source span for later slices.
- `gate4_owned_later_slice_forms_have_stable_capability_diagnostics` exercises
  the public build API, verifies the assigned per-slice code/span, and proves
  rejected later forms emit no executable artifact.
- Existing supported assignment, unpacking, calls, loops, exceptions,
  attributes, slicing, and boolean behavior remain covered by the full native
  workspace corpus rather than a parallel Gate 4 execution path.

## Proof

The completed Slice 1–4 boundary passes warning-denied clippy and the complete
workspace suite (193 tests, zero ignored), including 115 public native-pipeline
regressions. `scripts/verify_release.sh` remains green at 485,048 bytes.

## DONE BY CHATGPT
