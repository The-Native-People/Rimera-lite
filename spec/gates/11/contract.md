# Gate 11 dynamic compilation contract

All eight slices are frozen and complete. Gate 11 is `Implemented — conformance
audit pending` for the documented capability-enabled subset.

The compiler service is linked only into artifacts granted the
`dynamic_compilation` capability. Its registered Rust ABI callback compiles
borrowed source through the existing syntax, semantic analysis, HIR, verified
MIR and Cranelift lowering. The object emitter and JIT share the same lowering
functions. Runtime compilation never invokes Python or interprets source/IR.

The runtime owns argument validation, code objects, namespace selection,
exceptions and GC. The compiler owns parsing and native code generation.
Dependency flow remains compiler -> runtime -> ABI; runtime calls the installed
service through a callback and never depends on the compiler crate. Code is
published only after verification and native finalization. A context owns each
native unit while any managed code from it remains reachable, including through
escaped functions. Slice 5 reuses exact-key units and unloads dead units after
managed collection; teardown drops every remaining unit.

Exec and single modes execute with separate globals and locals. Module name
operations use the supplied locals mapping; explicit global declarations and
nested functions retain globals-only semantics. Eval returns the expression's
native result. Namespace operations, mapping callbacks, builtins fallback,
exception state, and function metadata extend the existing runtime models.

Slices 1–6 require public source differentials, capability-denial/no-artifact
proof, immutable code metadata, namespace/closure semantics and lifetime proof.
The frozen Slice 1–2 argument surface is: modes `exec`/`eval`/`single`; source
`str` plus UTF-8 bytes/bytearray/memoryview at the Python boundary; filename
`str` plus valid-UTF-8 `bytes`; `flags=0`; arbitrary `dont_inherit` by truthiness;
and `optimize=-1/0/1/2`. AST source, nonzero `PyCF_*` feature masks, and
surrogate-preserving non-UTF-8 byte filenames remain deferred rather than
silently approximated. Slices 5–6 close cache/reclamation and supported
dynamic/async composition. Slice 7 fixes the 1,048,576-byte source, 32-level
execution, and 128-live-unit limits plus failure atomicity. Slice 8 closes the
focused corpus and artifact audit without expanding those boundaries.
