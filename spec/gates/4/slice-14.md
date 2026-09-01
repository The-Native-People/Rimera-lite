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
