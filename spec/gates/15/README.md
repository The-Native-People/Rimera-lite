# Gate 15 — Native Extensions, Platform ABI, and Foreign Interoperability

Gate 15 establishes the extension boundary needed by real packages. A source-
compatible runtime is not a CPython drop-in if packages depend on `.so`,
`.dylib`, `.pyd`, or binary wheels that cannot load.

Slice 1 must choose and document the supported contract: a CPython 3.12 C-API/
stable-ABI compatibility layer, a rebuild-only source API, HPy, or an explicit
combination. Rimera must not claim binary-wheel compatibility beyond the ABI it
actually implements, and implementing an API must not embed CPython.

## Progress ledger

- [ ] Slice 1 — extension compatibility contract, ABI tiers, targets, and corpus
- [ ] Slice 2 — object handles, ownership, errors, calls, types, and module init
- [ ] Slice 3 — buffers, capsules, callbacks, GC participation, and finalization
- [ ] Slice 4 — dynamic-library loading, symbol/version checks, and isolation
- [ ] Slice 5 — build backends, headers/tooling, wheel tags, and reproducible linking
- [ ] Slice 6 — FFI callbacks, threads, TLS/runtime attachment, and blocking calls
- [ ] Slice 7 — representative extension corpus, sanitizers, failures, and security
- [ ] Slice 8 — platform matrix, compatibility declaration, and gate closure

## Slice acceptance

1. Publish exact source and binary compatibility tiers and select representative
   abi3, version-specific, HPy, and Rust/C/C++ extension fixtures.
2. Implement the selected object/reference API over stable Rimera handles with
   Python exceptions, generic calls, heap types, and multi-phase module init.
3. Share buffer exports and GC graphs safely across the boundary; callbacks and
   finalizers must preserve rooting and failure atomicity.
4. Validate architecture, ABI version, symbols, capabilities, and unload policy
   before extension code can affect runtime state.
5. Integrate common PEP 517 build flows and deterministic wheel selection/build
   without silently accepting incompatible CPython wheels.
6. Define foreign-thread attachment, runtime/context ownership, blocking-call
   release rules, callbacks, and thread-local exception state.
7. Run high-value extension packages under sanitizers and adversarial failure,
   malformed-binary, callback, GC, and shutdown cases.
8. Publish the passing extension matrix. Any unsupported CPython API remains a
   documented compatibility limit, not an assumed implementation.

This gate determines whether Rimera can claim source-extension compatibility,
stable-ABI compatibility, or broader CPython binary compatibility.
