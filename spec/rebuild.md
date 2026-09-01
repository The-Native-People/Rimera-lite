# Rimera native rebuild map

This is derived from [the native architecture specification](architecture.md).
Capability status and the active dependency order are maintained in the
[Python 3.12 compatibility ledger](compatibility.md).

1. Build the new workspace and HIR/MIR/LIR, verifier, object emitter, linker,
   and Rust ABI before porting semantics.
2. Port context, `RValue`, heap, exceptions, tracebacks, and module state.
3. Implement control flow, functions, calls, objects, classes, containers, and
   iteration through MIR and Cranelift.
4. Add package/lock resolution, modules, native stdlib, capability checks,
   caches, and manifests.
5. Add conditional native dynamic compilation through the same pipeline.
6. Delete frozen compatibility subsystems only after native differential proof.
