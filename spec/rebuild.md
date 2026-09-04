# Rimera native rebuild map

This is derived from [the native architecture specification](architecture.md).
Capability status and the active dependency order are maintained in the
[Python 3.12 compatibility ledger](compatibility.md).

1. Build the new workspace and HIR/MIR/LIR, verifier, object emitter, linker,
   and Rust ABI before porting semantics.
2. Port context, `RValue`, heap, exceptions, tracebacks, and module state.
3. Implement control flow, functions, calls, objects, classes, containers, and
   iteration through MIR and Cranelift.
4. Close the synchronous core, reflection, and context-manager gates.
5. Add package/lock resolution, modules, capability checks, caches, and
   manifests; then async protocols, conditional native dynamic compilation,
   and weak-reference/finalizer lifecycle semantics.
6. Run the Python 3.12 language corpus, standard library, native extension and
   platform ABI matrix, then real locked package/application acceptance.
7. Qualify a clean release against the complete public-surface inventories and
   publish its exact drop-in guarantee matrix.
8. Delete frozen compatibility subsystems only after native differential proof.
