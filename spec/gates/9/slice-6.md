# Gate 9 Slice 6 — `from` Imports, `__all__`, Star Imports, and Missing Names

## Goal

Implement synchronous `from` import semantics without compiler-side namespace
shortcuts.

## Integrated work

- Add absolute and parenthesized `from module import name [as alias]` forms,
  including submodule fallback where Python requires it.
- Implement star import from validated iterable `__all__`, or public namespace
  names when `__all__` is absent, preserving iteration/order/failure behavior.
- Restrict star import to legal scopes and route writes through the current
  module or prepared class namespace owner.
- Match missing module/name, malformed `__all__`, callback failure, and partial
  binding behavior with stable source spans.

## Completion proof

Public differentials cover aliases, parenthesized lists, submodule fallback,
dynamic `__all__`, underscore filtering, invalid names, callbacks, partial
publication, and constrained heaps.

## Status

Implemented. Owned syntax/HIR/MIR operations lower named and star imports to
the documented runtime ABI. Public native proof is
`from_imports_star_all_submodule_fallback_and_partial_binding_match_cpython`,
with the invalid sequence protocol and illegal-scope boundaries covered by
`star_import_rejects_iterator_only_all_like_cpython` and
`star_import_is_rejected_outside_module_scope_before_output`. Incremental
publication and missing/non-string failure types are pinned by
`star_import_preserves_partial_bindings_and_failure_types`.
