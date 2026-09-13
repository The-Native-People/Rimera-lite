# Gate 11 Slice 1 — modes, code-object contract, capabilities, and oracle matrix

Slice 1 freezes the authority boundary used by Gate 11. Dynamic source uses the
same parser, semantic analyzer, HIR, verified MIR and Cranelift lowering as an
ordinary Rimera build. There is no AST interpreter, Python bytecode executor,
CPython subprocess, or second dynamic semantics pipeline.

## Frozen contract

- Compile modes are the ABI-owned `exec`, `eval`, and `single` variants.
  `eval` uses expression grammar, `exec` module grammar, and `single` enforces
  CPython's one-interactive-statement cardinality.
- Dynamic compilation is opt-in through the `dynamic_compilation` capability.
  The CLI spelling is `--dynamic-compilation`; the project spelling is
  `[tool.rimera] dynamic_compilation = true`. The default is disabled.
- Capability-free builds retain the earlier static policy: stable
  `RIM-CAP-G7-02` rejection for reachable direct dynamic builtins and no final
  artifact. A normal static control also has no
  `rimera_dynamic_compiler_install` symbol. An opted-in artifact contains and
  invokes that installer.
- The compiler service is installed into one `RimeraContext` through a Rust ABI
  callback. `rimera-runtime` does not depend on `rimera-compiler`.
- A successful compilation owns one finalized Cranelift JIT module plus its
  native entry address. The owning context retains that native unit until
  teardown so managed code objects and escaped functions cannot observe a stale
  callable pointer. Gate 11 Slice 5 still owns cache policy and finer-grained
  reclamation/unloading.
- Managed code metadata is immutable through normal Python attribute setting.
  The Slice 1/2 code contract publishes the existing code-family metadata,
  including name/qualified name, filename, first line, flags, argument counts,
  locals/cell/free-variable metadata, and native entry ownership.

## CPython oracle matrix

The acceptance oracle is `/opt/homebrew/bin/python3.12`, CPython 3.12.11, on the
published macOS ARM64 target. The focused matrix covers:

- `exec`, `eval`, and `single` parse acceptance/rejection;
- `single` multiple-statement rejection;
- string and bytes-like source behavior;
- filename type behavior;
- `dont_inherit` truthiness;
- `optimize=-1/0/1/2` and invalid optimize values;
- syntax filename/text/line/offset metadata;
- readonly code metadata;
- capability denial and artifact omission.

The public fixture is executed once with CPython 3.12.11 and once through the
native Rimera executable and stdout must match exactly. The same Rimera program
also runs under a 262,144-byte managed-heap limit.

## Acceptance evidence

- `cargo test -p rimera-compiler dynamic::tests -- --nocapture`
- `cargo test -p rimera-compiler --test native_pipeline gate11_dynamic_namespaces_use_public_native_pipeline -- --nocapture`
- `cargo test -p rimera-cli -- --nocapture`
- enabled/disabled `nm` proof for `rimera_dynamic_compiler_install`
- native-only symbol scan for the capability-enabled artifact

Later Gate 11 slices must extend this contract rather than replacing it with an
interpreter or parallel compiler.

## DONE BY CHATGPT
