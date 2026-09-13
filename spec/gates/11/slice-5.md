# Gate 11 Slice 5 — dynamic cache and native-unit lifetime

The public acceptance fixture is
`tests/fixtures/basic/gate11_cache_lifetime.py`. It runs through the ordinary
capability-enabled native artifact at a 262,144-byte managed-heap limit and is
compared with CPython 3.12.11 before the artifact is scanned for forbidden
interpreter, CPython, generated-C, `setjmp`, and `longjmp` symbols.

## Cache key and publication

One context owns a linear collision-free cache of finalized JIT units. A key
compares the complete source text, filename, `exec`/`eval`/`single` mode,
accepted flags, and effective optimization level. It does not rely on a hash
alone. Identical keys reuse the finalized native unit but every successful
`compile()` still allocates and returns a distinct managed code object, matching
Python-visible identity. Any changed key component compiles a separate unit.

Compilation and native finalization happen before managed publication. A
compiler, verifier, finalizer, or code-object allocation failure publishes no
cache entry. During cache-hit publication the unit is temporarily pinned so an
allocation-triggered collection cannot unload the address being copied into the
new code object.

## Ownership and reclamation

Every dynamic code object records both its own entry address and the owning
native-unit address. Code objects created for nested functions, generators, or
coroutines inherit the active dynamic unit address. Function objects already
trace their code objects, so an escaped callable keeps the complete JIT unit
loaded without rooting dead module code.

After each completed managed collection, units with no surviving owning code
object are removed and their Cranelift memory is freed. An executing activation
is safe because active calls root their function and its code. Reusing a key
after reclamation recompiles it; no stale callable pointer or cross-context
owner is retained. Context teardown drops all remaining units after execution
has ended.

Captured custom builtins use a weak-function/strong-builtins association in the
collector. A reachable function retains its defining builtins, including
through association chains; an unreachable function and its otherwise-dead
builtins are reclaimed together. This avoids a context-lifetime root.

## Evidence and boundary

Runtime tests prove cache reuse, distinct code identities, owner destruction,
recompilation after reclamation, and ephemeron fixed-point behavior. The public
fixture proves escaped-callable safety, source/mode/optimization key separation,
repeated compile/execute/reclaim pressure, CPython output parity, and the
native-only artifact boundary. Slice 6 closes cross-feature dynamic
composition; Slice 7 closes adversarial limits and publication atomicity.
