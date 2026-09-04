# Gate 7 Slice 3 — Identity, Type Relations, and Reflective Attribute Helpers

## Goal

Close core reflective builtins through the established type, attribute, hash,
formatting, and generic-call protocols.

## Integrated work

- Audit and complete `type`, `isinstance`, `issubclass`, and nested class-info
  behavior for builtin/user classes, metaclasses, multiple inheritance, and
  supported protocol hooks.
- Complete `getattr`, `setattr`, `delattr`, `hasattr`, and `callable` with exact
  descriptor/custom-hook precedence and exception preservation.
- Complete Gate 7-owned behavior for `repr`, `ascii`, `format`, `hash`, and `id`
  through normal dunder dispatch, validation, stable identity tokens, and no raw
  address exposure.
- Ensure aliasing/rebinding of builtin names does not create compiler intrinsics
  or bypass the generic call path.
- Preserve exact roots during reflective callbacks and failure replacement;
  cache/version effects are coordinated with Slice 9.

## Completion proof

Public CPython differentials cover metaclass hooks, descriptors, callback
mutation, invalid return types, exception identity, aliases, IDs, cycles, and
forced collection. The integrated proof is
`gate7_slice3_identity_type_relations_and_attribute_reflection_match_cpython_312`
over `tests/fixtures/basic/gate7_identity_attribute_reflection.py`; the shared
fixture harness also performs the native forbidden-symbol artifact scan.

## DONE BY CHATGPT
