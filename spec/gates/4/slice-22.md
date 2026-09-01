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
