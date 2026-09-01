# Gate 3 Slice 7 — Constructor and Conversion Exactness

## Goal
Tighten builtin construction/conversion so `int`, `float`, `complex`, `bytes`, and `bytearray` accept and reject the same core forms as CPython 3.12.

## `int` remaining work
- Replace underscore stripping with Python's actual numeric-literal underscore grammar.
- Reject forms such as `1__2`, `_12`, and `12_`.
- Correct base-0 behavior: reject nonzero legacy-leading-zero forms such as `010`, while accepting valid zero forms such as `00`/`0_0`.
- Preserve optional sign/prefix handling for bases 2–36.
- Support Unicode decimal digits if the runtime claims exact core `int(str)` construction.
- Raise the correct `ValueError`/`TypeError` for invalid literal/base/object cases.

## `float` remaining work
- Do not rely solely on Rust `str.parse::<f64>()` as Python's parser.
- Support valid numeric underscores and reject invalid placement.
- Support Python decimal-digit rules, including Unicode decimal digits where required.
- Match accepted `inf`, `infinity`, `nan`, exponent, sign, and whitespace spellings.
- Preserve `__float__` then `__index__` fallback rules and validate return types.

## `complex` remaining work
- Replace the approximate sign-splitting parser with Python-compatible parsing.
- Correct exponent signs such as `1e-3j`.
- Do not globally remove spaces; reject forms CPython rejects such as `1 + 2j`.
- Support valid parentheses, signs, `j` shorthand, underscores, exponent forms, NaN, and infinity.
- Preserve `__complex__`, numeric fallback, and two-argument real/imag semantics.

## `bytes` / `bytearray` remaining work
- Treat memoryview/buffer input as raw logical bytes, not as an iterable of interpreted scalar elements.
- Preserve integer zero-fill construction, string+encoding/errors construction, bytes-like copying, and iterable-of-index values.
- Keep element range validation `0..255` and correct exception types.

## Completion criteria
Core constructor parsing and conversion cases agree with CPython 3.12.11 without compiler-side special cases.

## Evidence
`gate3_constructor_and_builtin_subclass_semantics_match_cpython_312` proves the core integer/base/underscore/Unicode cases, float and complex parser/protocol behavior, raw typed-memoryview byte copying, bytes/bytearray range and encoding forms, and bytes-family subclass numeric conversions against `/opt/homebrew/bin/python3.12` 3.12.11.

## DONE BY CHATGPT
