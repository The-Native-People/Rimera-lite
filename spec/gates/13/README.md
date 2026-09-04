# Gate 13 — Python 3.12 Language and Runtime Conformance Corpus

Gate 13 converts the earlier category-level proofs into a systematic Python
3.12 conformance audit. It closes semantic gaps; it does not bless known
differences, chase unstable wording blindly, or substitute test-count volume
for coverage.

The oracle is the pinned CPython 3.12 patch release recorded by the compatibility
ledger. Where CPython behavior is implementation-specific, Rimera documents the
boundary instead of claiming language-level parity.

## Progress ledger

- [ ] Slice 1 — executable syntax inventory, coverage map, and differential harness
- [ ] Slice 2 — numeric tower, conversions, hashing, formatting, and limits
- [ ] Slice 3 — Unicode, strings, bytes/buffers, codecs boundary, and representations
- [ ] Slice 4 — containers, iteration, slicing, mutation, and protocol precedence
- [ ] Slice 5 — calls, scopes, classes, descriptors, and metaclass edge behavior
- [ ] Slice 6 — exceptions, tracebacks, cleanup, generators, async, and lifecycles
- [ ] Slice 7 — randomized/stateful composition, GC pressure, and failure atomicity
- [ ] Slice 8 — final language audit, specification reconciliation, and gate closure

## Slice acceptance

1. Map every Python 3.12 grammar/AST form and applicable data-model operation to
   an executable differential, an earlier proof, or a named non-language scope.
2. Exercise arbitrary integers and mixed numeric rules at boundary values,
   including NaNs/infinities, conversion grammar, hashes, rounding, and errors.
3. Cover Unicode scalar behavior, normalization-sensitive operations where
   specified, formatting/repr, byte families, buffer providers, and codec APIs
   needed by core execution.
4. Audit mutation during callbacks, iterator invalidation, recursive objects,
   slicing extremes, generic protocols, and subclass/reflection interactions.
5. Stress binder and scope matrices, dynamic classes, descriptors, slots,
   metaclasses, annotations/type parameters, and transactional mutations.
6. Cross every control-transfer and suspension boundary with exception graphs,
   finalizers, retained frames, and exact traceback positions.
7. Add deterministic generative/state-machine differentials with minimized
   regressions, forced collection, constrained heaps, and malformed inputs.
8. Reconcile all compatibility rows and promote only behavior proven by the
   complete corpus.

Passing this gate supports a Python 3.12 **language/runtime compatibility**
claim. It is not yet a standard-library or package-ecosystem claim.
