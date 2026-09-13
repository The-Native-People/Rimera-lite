# Gate 11 Slice 6 — nested and cross-feature dynamic composition

The public acceptance fixture is
`tests/fixtures/basic/gate11_dynamic_composition.py`. It runs in the same
capability-enabled native artifact and 262,144-byte managed-heap boundary as
the earlier Gate 11 corpus, with CPython 3.12.11 stdout comparison and a final
forbidden-symbol scan.

## Proven composition

Dynamic code may call `compile`, `eval`, or `exec` again through the installed
context compiler. Nested compilation uses the same capability, validation,
cache, publication, namespace, and exception paths; there is no recursive
interpreter or secondary execution model.

Within the already-supported language surface, dynamically compiled code now
composes with registered imports, callbacks flowing in both directions, source
generators, immediately completing native coroutines, classes, descriptors,
instances, managed exceptions, and traceback/frame/code metadata.
Compatible `function.__code__` replacement can install dynamic code on an
existing function; that function's traced code edge retains the JIT unit.

Generator/coroutine functions produced by a JIT unit use the existing
suspension ABI and inherit that unit's lifetime owner. Exceptions cross the
same `RStatus` edges and attach ordinary managed traceback frames. Imports use
the Gate 9 registry/cache and do not create a dynamic-only loader. Descriptors
and callbacks use generic call and attribute protocols.

## Boundary

This slice does not claim arbitrary standard-library modules, `asyncio`,
networking, subprocesses, top-level await flags, or unsupported Python syntax.
It proves composition only for capabilities already closed by earlier gates.
Size/depth exhaustion, hostile mappings/callbacks, cache-publication failure,
and broader atomicity remain the active Slice 7 work.
