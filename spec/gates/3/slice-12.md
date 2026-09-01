# Gate 3 Slice 12 — Native Float `divmod()` Parity

## Goal
Make builtin `divmod()` produce CPython-consistent quotient/remainder pairs for native numeric operands while preserving custom protocol dispatch.

## Completed implementation
- Keep custom `__divmod__` / `__rdivmod__` dispatch for non-native objects.
- Replace the current native float implementation that separately computes floor division and modulo.
- Implement a dedicated float `divmod` algorithm so quotient and remainder are derived together.
- Match Python's sign rules for remainder and quotient.
- Preserve signed zero behavior.
- Recheck very large/small finite floats and floating precision near integral boundaries.
- Raise `ZeroDivisionError` for zero divisors with CPython-shaped behavior.
- Preserve integer `divmod` exactness for arbitrary-size integers.
- Preserve mixed int/float behavior.
- Keep `NotImplemented` fallback and reflected protocol ordering correct for custom classes.

## Completion criteria
Native integer, float, and mixed numeric `divmod()` pairs match CPython and custom dunder dispatch remains intact.

## Completion evidence
- Native float and mixed int/float `divmod()` now derive quotient and remainder together using CPython's coordinated `fmod`-style correction algorithm rather than independent floor-division and modulo operations.
- Quotient/remainder sign correction, signed zero, infinities, NaNs, large/small finite operands, and precision near integral boundaries match the covered CPython 3.12.11 behavior.
- Zero float divisors raise structured `ZeroDivisionError`, and integer-to-float overflow in mixed numeric calls raises structured `OverflowError` instead of silently producing infinity.
- Pure integer `divmod()` retains arbitrary-precision exactness.
- Custom `__divmod__` / `__rdivmod__`, `NotImplemented` fallback, and strict-subclass reflected priority remain unchanged and are included in the differential proof.
- `tests/fixtures/basic/gate3_round_divmod.py` is compared byte-for-byte with `/opt/homebrew/bin/python3.12` by `gate3_round_and_float_divmod_semantics_match_cpython_312`.

## DONE BY CHATGPT
