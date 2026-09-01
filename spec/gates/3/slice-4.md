# Gate 3 Slice 4 — Complex Number Semantics

## Goal
Make the managed `complex` builtin family behave correctly across construction, arithmetic, comparison, hashing, representation, and mixed numeric operations.

## Remaining implementation
- Fix the complex fast path so a complex value with zero imaginary component remains a complex operand. Do not drop `(1+0j)` into the plain float/int path merely because `imag == 0.0`.
- Ensure `(1+0j) == 1` and `(1+0j) == 1.0` are true.
- Ensure the corresponding hash invariant holds: equal complex/integer/float numeric values hash equally.
- Support mixed complex + int/float arithmetic for add, subtract, multiply, true divide, reflected variants, unary operations, and power.
- Fix the operator discriminant bug: complex floor division (`//`) must be rejected; complex true division (`/`) must use complex division.
- Ensure complex ordering comparisons (`<`, `<=`, `>`, `>=`) raise `TypeError` rather than using real-component ordering.
- Keep equality/inequality available against compatible numeric values.
- Complete builtin `pow()` behavior for complex operands where the two-argument form is legal.
- Keep three-argument modular `pow()` restricted to appropriate integer-like operands; custom object protocol dispatch remains separate.
- Recheck complex division by zero exception behavior.
- Recheck signed-zero, NaN, and infinity component behavior where Python defines it.
- Keep construction through `__complex__`, `__float__`, and numeric fallback consistent with the constructor work in Slice 7.

## Completion criteria
- Zero-imaginary complex values do not lose complex semantics.
- `/` and `//` are routed correctly.
- Mixed numeric equality and hashing agree with CPython.
- Complex arithmetic reaches the normal native pipeline without compiler intrinsics.

## DONE BY CHATGPT
