# Gate 3 Slice 16 — Memoryview Remaining Parity

## Goal
Close the remaining Python-visible `memoryview` gaps after the main Gate 3 memoryview implementation.

## Completed implementation
### Cast formats
- Add native single-character formats accepted by CPython 3.12 but currently missing, including `n`, `N`, and `P` where platform-native sizes permit them.
- Accept the optional native `@` prefix for supported single-character formats.
- Keep rejecting unsupported multi-character/non-native cast formats.
- Ensure scalar get/set, iteration, `tolist()`, hashing, and equality understand every format that `cast()` advertises.

### Equality
- Stop requiring identical format strings for equality.
- Compare logical element values and shapes as CPython does for compatible memoryview formats.
- Example: equal logical values in `B` and `b` views should compare equal where CPython says they do.
- Preserve `False`/`NotImplemented` behavior for unsupported format comparisons according to CPython.

### Hashing
- Only readonly views with hashable underlying exporters and allowed byte formats may hash.
- Follow the exporter chain, not merely the immediate exporter. A readonly view layered over another view whose ultimate exporter is a `bytearray` must remain unhashable.
- Preserve the rule that the memoryview hash agrees with the equivalent bytes hash for supported readonly byte-format views.

### Conversion
- `bytes(memoryview)` and `bytearray(memoryview)` must copy raw logical buffer bytes, not iterate typed scalar values.
- Recheck non-contiguous and sliced views when converting to bytes.

### Buffer protocol boundary
- Audit Python 3.12's Python-level buffer protocol (`__buffer__` / release behavior).
- If full user-defined buffer-provider support is required by Gate 3's permanent `memoryview` claim, implement it through the managed object/runtime protocol.
- Otherwise explicitly assign that behavior to a later gate and document the boundary; do not silently claim full buffer-provider compatibility.

## Existing CPython limitations to preserve
- Multi-dimensional integer subviews are intentionally not implemented by CPython in several cases.
- Multi-dimensional slice assignment is restricted in CPython.
- Do not "fix" CPython's own `NotImplementedError` boundaries into incompatible Rimera behavior.

## Completion criteria
Cast/equality/hash/conversion behavior for the supported memoryview core matches CPython 3.12.11, and any intentionally deferred Python-level buffer-provider feature has an explicit owner.

## Completion evidence
- A single native-format parser now owns cast format validation and accepts the supported one-character formats with an optional `@` prefix.
- Native `n`, `N`, and `P` are implemented, and native-width `l`/`L` now use the platform C `long` width instead of an incorrect fixed 4-byte assumption. On the supported 64-bit macOS target these formats are 8 bytes, matching CPython 3.12.11.
- Scalar get/set, iteration, and `tolist()` understand every format advertised by `cast()`, including signed/unsigned native integers, pointers, floats, and prefixed forms.
- Memoryview equality compares shape plus logical element values rather than raw bytes plus identical format strings. Cross-format numeric equality such as `B == b` for positive values and `i == f` for equal numeric elements follows CPython behavior.
- Readonly byte-format hashing follows the ultimate exporter chain and rejects views whose underlying exporter is a `bytearray`, including nested readonly views. Supported readonly `B`/`b`/`c` and `@` byte formats hash identically to their logical bytes.
- `bytes(memoryview)` and `bytearray(memoryview)` continue to copy raw logical bytes; non-contiguous sliced views are materialized in logical C order rather than by typed scalar iteration.
- Existing CPython multidimensional subview/slice-assignment `NotImplemented` boundaries remain unchanged.

## Explicit Python-level buffer-provider boundary
Python 3.12 PEP 688 user-defined providers (`__buffer__` / `__release_buffer__`) are not part of the Gate 3 builtin-exporter claim. Gate 3 permanently covers native `bytes`, `bytearray`, and `memoryview` exporters. User-defined Python-level buffer-provider dispatch is explicitly owned by Gate 7's managed reflection/object-protocol expansion, where those special methods can be exposed and audited together with the remaining reflective protocol surface.

## Proof
`gate3_slice_and_memoryview_remaining_semantics_match_cpython_312` covers native-width casts, `@` formats, scalar get/set/tolist, cross-format equality, exporter-chain hashing, and raw non-contiguous conversions against `/opt/homebrew/bin/python3.12`. The earlier `gate3_memoryview_surface_matches_cpython_312` regression remains green.

## DONE BY CHATGPT
