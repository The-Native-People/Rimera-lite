# Gate 3 Slice 11 — `round()` Float Parity

## Goal
Replace the current scale-and-round approximation with behavior matching CPython's correctly rounded float `round()` semantics.

## Completed implementation
- Fix classic decimal cases such as `round(2.675, 2) == 2.67`.
- Match ties-to-even behavior after binary floating representation is accounted for.
- Handle positive and negative `ndigits` correctly.
- Handle very large positive/negative `ndigits` without unnecessary overflow/underflow.
- Preserve signed zero where CPython does.
- Handle huge finite magnitudes.
- Handle subnormal values.
- `round(float('inf'))` without `ndigits` must raise `OverflowError`.
- `round(float('nan'))` without `ndigits` must raise `ValueError`.
- Recheck behavior with explicit `ndigits` for infinities/NaNs.
- Preserve integer rounding behavior, including negative decimal positions.
- Preserve custom `__round__` dispatch and keyword binding rules.
- Use structured exceptions rather than generic call errors.

## Completion criteria
Representative decimal, halfway, large-magnitude, signed-zero, NaN, and infinity cases match CPython 3.12.11.

## Completion evidence
- Native float rounding now works from the exact binary64 rational value, performs decimal ties-to-even rounding with arbitrary-precision integers, and converts the rounded decimal back to the correctly rounded binary64 result instead of using scale-and-round floating arithmetic.
- The implementation covers positive/negative `ndigits`, CPython's large-`ndigits` cutoffs, signed zero, subnormals, huge finite magnitudes, and overflow when the rounded decimal result cannot fit in a finite float.
- `round(x)` for finite floats produces arbitrary-size native integers, so values such as `1e20` are not limited by an `i128` cast.
- Non-`ndigits` infinity and NaN failures use structured `OverflowError` and `ValueError`; explicit `ndigits` preserves the non-finite float.
- Existing integer rounding and custom `__round__` dispatch remain on the generic call path.
- `tests/fixtures/basic/gate3_round_divmod.py` is compared byte-for-byte with `/opt/homebrew/bin/python3.12` by `gate3_round_and_float_divmod_semantics_match_cpython_312`.

## DONE BY CHATGPT
