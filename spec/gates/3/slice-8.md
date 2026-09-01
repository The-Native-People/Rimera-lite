# Gate 3 Slice 8 — Builtin-Storage Subclass Construction

## Goal
Make user subclasses of Gate 3 builtin storage layouts initialize through semantics equivalent to the underlying builtin constructor rather than through a narrower parallel path.

## Remaining implementation
- Audit `initialize_instance_storage` for list, tuple, dict, set, float, complex, bytes, bytearray, and frozenset layouts.
- Float subclasses must honor the same `__float__`/`__index__` conversion behavior as `float()`.
- Complex subclasses must support the constructor's valid argument forms, including real/imag semantics where the class call allows them.
- Bytes/bytearray subclasses must support proper bytes-like, iterable, integer-size, and string+encoding/errors construction instead of only `byte_values()`.
- Dict subclasses must use the permanent generic-key dictionary representation; do not silently fall back to the old string-key-only dictionary storage.
- Dict subclass construction must accept the same mapping/iterable-pair forms and keyword updates as the supported builtin `dict()` core.
- Set/frozenset subclasses must use generic hashing/equality and normal iterable construction.
- Recheck tuple/frozenset identity optimizations so they do not incorrectly apply to subclasses.
- Preserve class identity while delegating storage initialization to the builtin semantics.
- Root every intermediate iterator/value/storage replacement across callbacks and allocation.
- Ensure subclass `__init__` continues through the generic call path after storage creation.

## Completion criteria
Supported builtin-storage subclasses do not have a weaker key/conversion model than their builtin base and remain traced/collectible under GC.

## Evidence
`gate3_constructor_and_builtin_subclass_semantics_match_cpython_312` proves shared builtin constructor semantics, permanent generic-key dict storage, mutable-vs-immutable custom `__init__` behavior, native mutable-base `super().__init__`, inherited storage-backed methods, and the GC-safe payload handoff that roots fresh builtin storage before the wrapper instance allocation.

## DONE BY CHATGPT
