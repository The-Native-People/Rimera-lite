# Gate 4 Slice 22 — Class Patterns and `__match_args__`

## Goal

Complete Python 3.12 class-pattern matching on the existing object model.

## Implementation

- Evaluate the class expression once and require a runtime type.
- Check `isinstance(subject, class)` through the generic subtype contract.
- Resolve positional fields through class `__match_args__`, validating tuple
  shape, string entries, count, and duplicate attribute selection.
- Resolve keyword subpatterns through ordinary attribute lookup and descriptor
  binding; missing attributes fail the pattern while other exceptions propagate.
- Apply patterns and bindings transactionally, including nested OR/AS,
  sequence, and mapping patterns.
- Respect user descriptors, inheritance, slots, and metaclass-provided class
  attributes without compiler special cases.

## Proof

- Differentials cover positional/keyword class patterns, inheritance,
  descriptors, invalid `__match_args__`, duplicate names, side effects,
  exceptions, and forced GC.

## Completion evidence

- Class patterns are owned directly in syntax/HIR and lower to the explicit
  `PatternClass` MIR operation; no `RIM-CAP-G4-22` source boundary remains.
- Lowering resolves the class expression exactly once, then recursively applies
  positional/keyword subpatterns through the same tentative capture table used
  by Slices 19–21, so a failed nested class pattern publishes no real binding.
- `rimera_pattern_class` requires a runtime type, checks the generic
  `isinstance` contract, resolves positional fields through descriptor-aware
  class/metaclass `__match_args__`, validates tuple/string/count/duplicate-name
  rules, and resolves keyword fields through ordinary descriptor-aware instance
  lookup. Missing attributes are structural mismatch; other exceptions remain
  managed Python exceptions.
- Cranelift registers non-empty NUL-separated keyword metadata and emits
  `(null, 0)` for keyword-free class patterns, avoiding any hidden dynamic
  parser/evaluator path.
- `gate4_class_pattern_roots_subject_and_class_and_evaluates_class_expression_once`
  proves the dotted class expression has one MIR evaluation, and that
  `PatternClass` roots both subject and resolved class with an exception
  successor.
- `gate4_class_patterns_match_cpython_312` matches CPython 3.12.11 for
  positional/keyword patterns, inheritance, descriptor binding, slots,
  builtin match-self behavior, metaclass-provided `__match_args__`, nested
  mapping/sequence patterns, failed-binding rollback, invalid `__match_args__`,
  excessive positionals, duplicate attribute selection, non-type class
  expressions, and descriptor exceptions.
- `gate4_class_patterns_survive_forced_gc` runs descriptor-driven class
  extraction nested inside mapping/sequence matching under a 32 KiB managed
  heap limit while callbacks allocate heavily, proving subject, class,
  extracted values, and tentative captures remain live across collection.
- Public class-pattern artifacts pass the native-only forbidden-symbol scan;
  no generated C, CPython ABI, bytecode interpreter, or alternate execution
  path is introduced.

## DONE BY CHATGPT
