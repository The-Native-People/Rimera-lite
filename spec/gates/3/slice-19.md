# Gate 3 Slice 19 — Unicode Code-Point Boundary

## Goal
Resolve or explicitly bound the mismatch between Python strings and Rust `String`/`char` for lone surrogate code points.

## Problem
Python strings can contain surrogate code points such as `U+D800`. Rust `char` and `String` cannot represent surrogate scalar values.

## Completed decision — Path B
Gate 3 keeps the existing UTF-8/Rust-scalar string representation and explicitly narrows its Python-string compatibility claim to Unicode scalar values. Full preservation of lone surrogate code points is deferred to the final Unicode/string-codec compatibility corpus after the numbered synchronous core gates.

- `chr()` accepts the supported scalar domain through `U+10FFFF`, including values immediately below and above the surrogate block.
- `ord()`, repr/ascii, length/indexing/slicing/iteration, hashing, and existing UTF-8 behavior continue to use the established native string representation.
- `U+D800..U+DFFF` creation through `chr()` is an intentional unsupported boundary and raises a structured `ValueError` with the stable message `Rimera Gate 3 strings do not support lone surrogate code points`.
- Out-of-range values still use the normal Python-shaped `chr() arg not in range(0x110000)` failure instead of conflating the deliberate surrogate boundary with numeric range validation.
- Gate 3 therefore does **not** claim that Rimera strings and Python strings have identical code-point domains.

## Completion criteria
There is no silent false claim that Python strings and Rust Unicode scalar strings have identical domains. Either the gap is implemented or it has an explicit later-gate owner and documented compatibility boundary.

## Proof
- `ffi::tests::chr_surrogate_boundary_is_explicit_and_structured` proves the runtime distinguishes the supported scalar edge from the deliberate lone-surrogate boundary and records a managed `ValueError`.
- `gate3_unicode_scalar_domain_matches_cpython_312` differentially proves supported scalar edges against CPython 3.12.11.
- `gate3_lone_surrogate_creation_has_an_explicit_runtime_boundary` proves CPython can create `U+D800` while Rimera deterministically rejects that exact unsupported domain instead of corrupting or misrepresenting it.

## DONE BY CHATGPT
