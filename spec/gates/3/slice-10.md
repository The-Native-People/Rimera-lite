# Gate 3 Slice 10 — Complete `format()` Semantics

## Goal
Finish the native formatter so supported core objects follow CPython 3.12 formatting rules instead of only passing basic happy-path cases.

## Completed implementation
- Add complex-number formatting.
- Replace approximate float formatting/rounding with CPython-compatible output.
- Match default float `g` behavior and scientific/fixed switching.
- Match exponent spelling and zero padding.
- Finish alternate-form `#` behavior.
- Finish grouping behavior for `,` and `_` across supported numeric types.
- Implement `z` negative-zero coercion where Python allows it.
- Reject `z` for integer formats and other invalid contexts.
- Handle signed zero, infinity, and NaN exactly.
- Validate every meaningful combination of fill, alignment, sign, zero-padding, width, grouping, precision, alternate form, and type code.
- Reject invalid string alignment such as numeric-only `=` behavior where CPython rejects it.
- Preserve string precision/truncation and alignment behavior.
- Preserve custom `__format__` dispatch and require a string return value.
- For unsupported nonempty specs on objects without a usable formatter, raise the correct `TypeError` rather than falling back to repr.
- Decide/document locale-sensitive `n` behavior. If locale behavior is explicitly deferred as stdlib/platform-dependent, reject or constrain it consistently instead of silently approximating it.
- Raise the correct `ValueError` for malformed/invalid format specifications.

## Completion criteria
The supported int/float/complex/string formatting core agrees with CPython for ordinary and edge format specs, while locale-dependent behavior has an explicit boundary.

## Completion evidence
- Native formatting covers supported integer `b/o/x/X/d/n/c`, float default/`f/F/e/E/g/G/n/%`, complex, and string presentations.
- Fill/alignment/sign, alternate form, width, precision, zero-padding, `,`/`_` grouping, `z` negative-zero coercion, exponent spelling, signed zero, infinity, and NaN follow the covered CPython 3.12 behavior.
- Validation rejects invalid integer `z`, invalid string/complex `=` alignment, complex zero-padding, unsupported complex type codes, invalid grouping with `n`, malformed specs, and non-string `__format__` returns through the normal typed exception path.
- Numeric grouping remains correct when zero-padding and non-decimal radix prefixes interact, including cases such as `#08_X`.
- Float rounding and general-format switching are differentially covered at representative binary-float boundaries including `2.675`, `1.005`, `9.9995`, `9.99995`, and fixed/scientific carry-over cases.
- Locale-sensitive `n` behavior is intentionally bounded to Rimera's native non-locale-specialized core; external locale-specific punctuation/digit substitution remains stdlib/platform-dependent and is not silently approximated.
- `tests/fixtures/basic/gate3_repr_format.py` is compared byte-for-byte with `/opt/homebrew/bin/python3.12` by `gate3_repr_ascii_and_format_semantics_match_cpython_312`.

## DONE BY CHATGPT
