# Gate 3 Slice 21 — Small Python-Visible Builtin Type Surfaces

## Goal
Close the remaining obvious Python-visible attributes/methods of Gate 3 builtin values without expanding Gate 3 into a full stdlib/method-table project.

## Completed Gate 3 surface
### Complex
- `.real` and `.imag` return managed float values through ordinary attribute lookup.
- `.conjugate()` is a first-class bound managed builtin method and preserves signed imaginary behavior through the ordinary call path.

### Float
- `.real` returns the original float object, matching CPython identity behavior.
- `.imag` returns managed `0.0`.
- `.conjugate()` returns the original float object.
- `.is_integer()` handles finite values, infinities, and NaNs with CPython behavior.
- `.as_integer_ratio()` reconstructs the exact binary ratio, including subnormals and signed values, and raises structured `OverflowError`/`ValueError` for infinity/NaN.
- `.hex()` emits CPython-compatible normal, subnormal, signed-zero, infinity, and NaN text.
- `float.fromhex()` is intentionally not part of Gate 3. Class-level builtin method descriptors and the broader reflective builtin method-table audit are owned by Gate 7; Gate 3 does not claim this class method.

### Already completed by their owning slices
- Range `.start`/`.stop`/`.step`/`.count()`/`.index()` remain owned and proven by Slice 14.
- Slice `.start`/`.stop`/`.step`/`.indices()` remain owned and proven by Slice 15.
- Dictionary-view `.mapping` remains owned and proven by Slice 17.

## Scope rule
Do **not** turn this slice into implementing every convenience method on `str`, `bytes`, `bytearray`, list, dict, or set. Gate 3's authoritative contract is construction, tracing, managed-size accounting, truth, equality, hash policy, repr/str, conversion, applicable iteration, mutation rules, and the synchronous builtin namespace. Only small methods/attributes necessary to make an explicitly named Gate 3 builtin family feel incomplete should land here.

## Completion criteria
Every small surface included in the final Gate 3 compatibility claim is actually available through normal managed attribute lookup, while anything intentionally omitted has a clear later owner.

## Proof
- `ffi::tests::gate3_float_and_complex_small_surfaces_use_managed_attributes` proves the new values and methods use the managed attribute/bound-call path rather than compiler intrinsics.
- `gate3_small_builtin_type_surfaces_match_cpython_312` differentially proves float/complex attributes, object identity where applicable, exact integer ratios, float hex text, typed ratio failures, and complex conjugation against CPython 3.12.11.
- Existing Slice 14, 15, and 17 public differentials continue to prove the range, slice, and dictionary-view surfaces without duplicating implementations.

## DONE BY CHATGPT
