# Gate 11 dynamic compilation contract

Work in progress; no completion status is implied by this design.

The compiler service is linked only into artifacts granted the
`dynamic_compilation` capability. Its registered Rust ABI callback compiles
borrowed source through the existing syntax, semantic analysis, HIR, verified
MIR and Cranelift lowering. The object emitter and JIT share the same lowering
functions. Runtime compilation never invokes Python or interprets source/IR.

The runtime owns argument validation, code objects, namespace selection,
exceptions and GC. The compiler owns parsing and native code generation.
Dependency flow remains compiler -> runtime -> ABI; runtime calls the installed
service through a callback and never depends on the compiler crate. Code is
published only after verification and native finalization. A context owns the
native allocations until teardown, including code reachable through escaped
functions. Finer-grained unloading/cache reclamation belongs to Slice 5.

Exec and single modes execute with separate globals and locals. Module name
operations use the supplied locals mapping; explicit global declarations and
nested functions retain globals-only semantics. Eval returns the expression's
native result. Namespace operations, mapping callbacks, builtins fallback,
exception state, and function metadata extend the existing runtime models.

Slices 1–4 require public source differentials, capability-denial/no-artifact
proof, immutable code metadata, namespace/closure semantics and lifetime proof.
This file will record accepted arguments and limits once the implementation is
verified. Slice 5 retains responsibility for cache policy and reclamation;
Slice 6 owns extended dynamic/async composition and Slice 7 the adversarial
resource-limit corpus.
