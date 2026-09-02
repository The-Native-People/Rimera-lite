# Gate 4 Slice 14 — F-Strings and Formatting Conversions

## Goal

Implement Python formatted-string expressions through the generic formatting
protocol.

## Implementation

- Own `JoinedStr` and `FormattedValue` syntax/HIR, preserving literal segments,
  expression spans, conversion flags, and nested format specifications.
- Evaluate segments left-to-right once.
- Apply `!s`, `!r`, and `!a` through the ordinary `str`, `repr`, and `ascii`
  paths, then invoke `format(value, spec)`.
- Support debug expressions (`{expr=}`), escaped braces, nested replacement
  fields in format specs, and concatenation into one native string.
- Root accumulated segments and formatted values across calls and allocation.
- Produce parse/semantic diagnostics for malformed fields without artifacts.

## Proof

- CPython differentials cover conversions, alignment/specifiers, nesting,
  Unicode, side effects, callback exceptions, and debug forms.
- Forced-GC tests retain earlier segments during later formatting callbacks.

## Completion evidence

- `JoinedString` and `FormattedValue` lower to left-to-right native string
  concatenation plus the documented `rimera_format_value` ABI. `!s`, `!r`,
  and `!a` reuse ordinary runtime conversion paths before generic formatting;
  nested format specs are themselves normal joined/formatted expressions.
- `gate4_fstrings.py` matches CPython 3.12 for debug expressions, escaped braces,
  alignment and numeric specs, nested precision/width, Unicode, side effects,
  custom `__format__`, and all three conversions.
- MIR safepoint proof pins the formatted value, nested spec, and previously
  accumulated prefix as live roots across later formatting callbacks.

## DONE BY CHATGPT
