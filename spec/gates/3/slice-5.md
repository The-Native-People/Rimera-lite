# Gate 3 Slice 5 — Exact Numeric Equality and Hashing

## Goal
Make Python's numeric equality and hash invariant exact enough for generic dictionary/set keys across `bool`, `int`, `float`, and `complex`.

## Remaining implementation
- Stop comparing arbitrary-size integers to floats by lossy blanket conversion to `f64`.
- Implement exact int↔float comparison using Python-compatible integer/floating decomposition so values near and above 2**53 do not collapse incorrectly.
- Preserve exact comparisons for finite integral floats, non-integral floats, infinities, NaNs, and signed zero.
- Make bool follow integer numeric semantics.
- Implement mixed int/float/complex equality consistently, including zero-imaginary complex values.
- Ensure `a == b` implies `hash(a) == hash(b)` across all supported numeric types.
- Replace the current float hash shortcut that only aligns integral floats inside `i64` range with CPython-compatible numeric hashing for all finite doubles.
- Recheck `+0.0` and `-0.0` hashes.
- Recheck infinities and NaNs.
- Rework complex hashing to use the corrected component hashes and CPython-compatible combination rules.
- Keep Python's `-1` hash sentinel normalization to `-2`.
- Ensure arbitrary-size user `__hash__` results continue to normalize to the runtime's `Py_hash_t`-shaped domain.

## Why this is Gate 3 critical
Generic dict/set keys rely on equality and hash agreement. A lossy mixed-numeric comparison can create duplicate-equal keys, failed lookup, wrong replacement, or set membership errors.

## Completion criteria
Representative large mixed numeric values obey exact CPython equality and hash invariants and work correctly as dictionary/set keys.

## DONE BY CHATGPT
