# Gate 3 Slice 9 — `repr`, `str`, and `ascii` Edge Parity

## Goal
Finish Python-shaped representation for Gate 3 builtin values, nested containers, Unicode strings, floats/complex numbers, and recursive views.

## Completed implementation
### Float/complex representation
- Replace the approximate Rust `f64::to_string()`-based float repr with CPython-compatible shortest-roundtrip formatting.
- Match scientific-notation thresholds, exponent sign/zero padding, signed zero, infinities, and NaNs.
- Make complex repr use the corrected float-component representation and CPython parentheses/sign rules.

### String/bytes representation
- Base escaping on Python Unicode printability rather than a small hand-picked set of spaces/control characters.
- Escape non-printable code points such as zero-width characters in the same form CPython uses (`\x`, `\u`, `\U`).
- Preserve quote selection, backslash escaping, common control escapes, and bytes hex escaping.

### Nested/recursive representation
- Ensure list/tuple/dict/set/frozenset/slice/dict-view representations recursively call each element's `__repr__` and validate it returns `str`.
- Extend recursion protection to dictionary view objects. Self-referential `dict_values`/`dict_items` structures must render `...` rather than recurse indefinitely.
- Preserve normal cycle markers for recursive list/dict/tuple paths.

### `str` / `ascii`
- `str()` must continue to use `__str__` and fall back correctly.
- `ascii()` must operate on `repr()` output and escape every non-ASCII code point without changing already-ASCII repr syntax.

## Completion criteria
Representative builtin/nested/cyclic representations match CPython's visible text and cannot recurse indefinitely.

## Completion evidence
- Float repr now follows CPython 3.12 shortest-roundtrip display policy, including fixed/scientific cutovers, signed and zero-padded exponents, signed zero, infinities, and NaNs.
- Complex repr uses the corrected component representation and preserves Python sign/parenthesis behavior, including negative-zero imaginary components.
- Unicode repr printability is derived from Unicode 15.0 general categories, matching the Unicode version used by CPython 3.12; non-printable code points use Python-shaped `\\x`, `\\u`, or `\\U` escapes.
- Dictionary views participate in repr recursion protection, so self-referential `dict_values` and `dict_items` render `...` rather than recursing indefinitely.
- `str`, `repr`, and `ascii` preserve user protocol dispatch and validate that user `__str__`/`__repr__` implementations return strings.
- `tests/fixtures/basic/gate3_repr_format.py` is compared byte-for-byte with `/opt/homebrew/bin/python3.12` by `gate3_repr_ascii_and_format_semantics_match_cpython_312`.

## DONE BY CHATGPT
