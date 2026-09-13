# Gate 11 Slice 2 — `compile` sources, flags, diagnostics, and code metadata

Slice 2 makes `compile()` a real native compilation operation behind the Slice
1 capability. A managed code object is published only after parsing, semantic
analysis, HIR/MIR lowering, MIR verification, Cranelift finalization, and native
entry creation succeed.

## Supported `compile()` surface

- Source forms: `str`, `bytes`, `bytearray`, and `memoryview` at the Python
  boundary. Bytes-like source is decoded as UTF-8; a UTF-8 BOM is accepted.
  AST-object input is deferred and is not silently interpreted.
- Filename forms: `str` and valid-UTF-8 `bytes`. Other bytes-like filename
  objects are rejected. CPython's surrogate-preserving non-UTF-8 byte filename
  behavior is deferred because Rimera strings do not currently expose that
  surrogate representation.
- Modes: `exec`, `eval`, `single`.
- `flags`: the frozen Slice 2 supported value is `0`. Nonzero masks fail before
  code publication. CPython-valid masks whose contracts require AST return,
  type-comment parsing, top-level await, or future-feature inheritance remain
  explicitly deferred rather than being accepted as no-ops.
- `dont_inherit`: accepts arbitrary values through Python truthiness. With the
  current `flags=0` boundary there are no inherited future flags to apply.
- `optimize`: `-1`, `0`, `1`, and `2` are accepted. Positive optimization
  removes asserts, level 2 also removes docstrings, and `__debug__` is lowered
  to the corresponding constant. Other values raise `ValueError` before
  publication.
- Embedded NUL source is rejected before compilation.

## Diagnostics and publication

Parser failures become managed `SyntaxError` instances carrying the source
filename plus message/text/line/offset information used by the current runtime
exception model. Option failures use `ValueError`. Unsupported source/filename
shapes use managed type/syntax failures at the Python boundary. No managed code
object or callable native address is published on failure.

Successful code objects retain immutable Python-visible metadata and an opaque
native-unit owner. The public differential proves metadata reads and readonly
attribute assignment, source-form handling, filename handling, option failures,
syntax offset propagation, native execution, and constrained-heap operation.

## Deliberately not claimed by Slice 2

- Full `eval()` namespace/closure conformance: Slice 3.
- Full `exec()` write/declaration/class-body conformance: Slice 4.
- Dynamic cache keys and fine-grained unloading/reclamation: Slice 5.
- AST-input compilation, nonzero `PyCF_*` feature masks, or a general Python
  3.12 dynamic-code compatibility claim.

## Acceptance evidence

- compiler dynamic-unit tests cover all three modes, deterministic/source-
  sensitive metadata, UTF-8 byte parity, option rejection, invalid source bytes,
  optimization transforms, and verified MIR;
- the public CPython 3.12.11/native differential covers managed `compile()` code
  creation, bytes-like sources, bytes filename, invalid filename type,
  `dont_inherit`, invalid flags, optimize behavior, readonly metadata,
  `SyntaxError.offset`, native execution, and a 262,144-byte heap run;
- native-only and capability on/off symbol scans remain green.

## DONE BY CHATGPT
