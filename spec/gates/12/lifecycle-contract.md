# Gate 12 lifecycle contract

This contract fixes the meaning of managed lifetime before the public weak
reference and finalization APIs expand. It applies to opaque generational
`RValue` handles only; a Rust address is never Python identity and is never a
weak-reference key.

## Object states

- **Strongly reachable** — discovered from a generated shadow frame, temporary
  native root, context root, or a traced strong edge. It survives the cycle.
- **Weakly observed** — named only by the non-tracing referent field of a live
  weak-reference object. This does not make the referent reachable.
- **Finalizable** — unreachable ordinary marking, but retained by the lifecycle
  phase because a later gate's once-only finalization obligation is pending.
- **Resurrected** — a finalizable object made strongly reachable by Python code
  during lifecycle work. Slice 5 owns the once-only resurrection transition;
  Slices 1–2 do not expose `__del__` or resurrection.
- **Dead** — unreachable after lifecycle processing and removed during sweep.
  Its slot generation advances before reuse, so its old handle cannot resolve.
  A retired maximum-generation slot is never reused.

A weak-reference object has its own independent strong/dead state. While that
object is strongly reachable, its callback and cached hash are strong children;
its referent is always weak. If the weak-reference object is itself unreachable,
neither it nor its callback is retained merely to announce referent death.

## Collection phases and transitions

1. **Mark** traces all ordinary roots and strong object fields to a fixed point.
   Dynamic-code builtins associations are ephemerons and also reach a fixed
   point. Weak referent fields are not traversed.
2. **Lifecycle** examines only marked weak-reference objects. When their
   referent is unmarked, every observation is cleared before Python can run.
   Callback actions are retained with explicit temporary roots and ordered by
   descending registration ordinal, matching newest-first CPython behavior for
   references to the same object. Buffer and suspended-generator actions remain
   on this same phase boundary.
3. **Sweep** reclaims every unmarked object, charges managed bytes down, and
   increments or retires the slot generation. Lifecycle actions execute only
   after the heap has returned to its idle phase.
4. **Re-entry** runs callbacks through the ordinary Python call path. Callback
   allocation and collection are legal. A subsequent collection observes the
   already-cleared referent, so the callback cannot be scheduled twice.

The allocating safepoint's pre-existing raised exception is restored after a
callback. Slice 2 callback failures are unraisable and do not replace it; Slice
4 owns Python-visible unraisable reporting together with `__del__`.

## CPython 3.12.11 oracle matrix

| Case | Required observation |
| --- | --- |
| `weakref.ref(obj)()` while live | Exact referent identity |
| Two callback-free refs to one object | Same reference identity |
| Callback registration | `None` disables a callback; any other object is accepted and invoked later |
| Several live callbacks | Newest registration runs first |
| Dead referent | Calling the reference returns `None` |
| Hash before death | Referent hash is cached and remains available after death |
| First hash after death | `TypeError: weak object has gone away` |
| Equality while both live | Delegates to referent equality |
| Equality after either dies | Reference identity only |
| Proxy while live | Attribute, call, and supported special-method operations target the referent |
| Proxy after death | `ReferenceError: weakly-referenced object no longer exists` |
| Proxy hash | Always `TypeError: unhashable type: 'weakref.ProxyType'` |
| Unsupported target | `TypeError: cannot create weak reference to '<type>' object` |

The checked-in public differential covers the observable matrix under a 256
KiB managed heap, and the heap unit tests cover pre-callback clearing,
newest-first action order, callback retention, and silent disposal of an
unreachable weak-reference object.

## Slice 2 public surface and limits

The managed `weakref` module publishes `ref`/`ReferenceType`, `proxy`,
`ProxyType`, `CallableProxyType`, `getweakrefcount`, and `getweakrefs`.
Weak-reference targets currently include user instances whose class layout has
weak-reference support, plus managed functions, bound methods, types, modules,
code objects, and suspended generator-family objects. Ordinary user classes
without `__slots__` have weak-reference support. A slotted class requires an
explicit `__weakref__` slot or inheritance from a weak-reference-capable base.

Slice 3 adds `WeakKeyDictionary`, `WeakValueDictionary`, and `WeakSet`. A weak
key entry traces its value but not its key; a weak value entry traces its key
but not its value; a weak-set entry traces neither observed object. The
container strongly owns the corresponding managed reference object. Each
container observation is distinct from the public callback-free `weakref.ref`
canonical object: `getweakrefcount`/`getweakrefs` still observe it, while a
later public `weakref.ref(obj)` never canonicalizes to the container-private
reference. Lifecycle pruning removes entries whose observation was cleared and
requests another mark/sweep turn, so a removed strong payload is reclaimed
during the same public collection request.

Lookup snapshots candidate entries and revalidates their weak-handle identity
after user equality, so equality may mutate or clear the container without a
stale index write. Explicit size mutations increment a version and invalidate
an active iterator. GC-driven pruning does not: weak-container iterators own a
snapshot of entry identity, revalidate it against the live container before
each yield, skip entries cleared during iteration, and never trace all
referents merely because iteration is active. Size-preserving replacement is
therefore visible through an iterator that was already created, matching the
ordinary dictionary iterator behavior used by CPython's weak containers.

Constructors accept no keyword arguments and zero or one positional source.
`WeakSet` accepts an iterable. The weak dictionaries accept a mapping whose
iteration yields keys and whose item protocol returns values; iterable-of-pairs
construction is not yet supported. Weak dictionaries expose item get/set/delete,
membership, length/truth, iteration, `keys`, `values`, `items`, `get`, `pop`,
`setdefault`, and `clear`. The dictionary helpers preserve CPython's observable
lookup protocol: `pop` performs one key hash, while a missing `setdefault`
hashes once for `WeakKeyDictionary` and twice for `WeakValueDictionary` (hits
hash once for either kind). `WeakSet` exposes membership, length/truth,
iteration, `add`, `discard`, `remove`, and `clear`. Union/algebra operations,
`update`, `copy`, `pop` on `WeakSet`, and subclassing these native container
types remain outside Slice 3.

## Slice 4 finalization and shutdown

User instances whose effective class MRO exposes `__del__` participate in the
collector lifecycle without a second ownership system. An unreachable ordinary
instance is retained for one lifecycle turn and finalized before its weak
observations are cleared; the next collection turn clears weakrefs and runs
their callbacks if the object was not resurrected. The instance carries a
once-only `finalized` state set before invoking Python so allocation, nested
collection, callback re-entry, and shutdown cannot invoke the same `__del__`
twice.

Finalizer and weakref-callback exceptions are unraisable: Rimera emits an
`Exception ignored in:` diagnostic with the managed exception/traceback and
then restores the allocating caller's exception state. Runtime shutdown walks
still-live finalizable instances in an explicit monotonic creation order rather
than heap-slot order, so generational slot reuse cannot perturb observable
teardown order. The public CPython 3.12.11 differential covers ordinary
finalizer-before-weakref ordering, inherited and dynamically installed
`__del__`, once-only execution, unraisable finalizer/callback exceptions, and
creation-ordered shutdown; a heap test proves the protected two-turn weakref
transition.

Resurrection and cyclic-isolate ordering remain Slice 5. `weakref.finalize` and
broader teardown composition remain Slice 6. This is not a claim for the rest
of the Python `weakref` standard-library module.
