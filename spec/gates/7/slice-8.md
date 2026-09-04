# Gate 7 Slice 8 — Python-Level PEP 688 Buffer Providers

## Goal

Extend Gate 3's native-exporter memoryview core to Python-defined buffer
providers through generic object protocols.

## Integrated work

- Implement Python 3.12 `__buffer__` and `__release_buffer__` discovery,
  descriptor binding, flag forwarding, return validation, and exception
  preservation for user objects.
- Connect provider results to the existing managed memoryview format, shape,
  strides, readonly, slicing, casting, equality, hashing, and raw-conversion
  paths without duplicating buffer storage.
- Call release exactly once at the correct lifetime boundary, including nested
  views, explicit release, context use supported by the object, failed
  construction, callback exceptions, and exporter cycles.
- Enforce resize/export restrictions and keep exporter/view graphs rooted across
  callbacks, allocation, and collection.
- Keep native bytes/bytearray/memoryview exporters on the same authoritative
  buffer contract and preserve their Gate 3 behavior.

## Implemented surface

- `memoryview(user_object)` performs ordinary special-method lookup for
  `__buffer__`, descriptor-binds it, forwards CPython 3.12's requested flag mask
  (`284` for this constructor path), preserves provider exceptions, and requires
  a live managed memoryview result.
- Provider exports use one traced `BufferLeaseObject` shared by every derived
  view. Slices, casts, `toreadonly()`, and `memoryview(existing_view)` extend the
  same lease, so releasing one view cannot prematurely invoke
  `__release_buffer__`; the final live view invokes it exactly once with the
  exact memoryview returned by `__buffer__`.
- Explicit release, failed post-acquisition construction, callback failure, and
  exporter/view cycles all release the underlying native export exactly once.
  Release-callback exceptions follow CPython's unraisable semantics: they do not
  replace the caller's exception state or make `memoryview.release()` fail.
- GC finalizes provider leases outside the raw heap mark/sweep phase, preserves
  the provider/exported-view graph for the callback, then recollects finalized
  cycles. Native bytes/bytearray/memoryview exporters retain the existing Gate 3
  path and bytearray resize/export accounting.

## Completion proof

Public CPython 3.12.11 differentials cover writable and read-only providers,
flags, nested/sliced lifetimes, mutations, validation failures, provider
exceptions, callback-exception semantics, and a 48 KiB forced-GC/heap-pressure
fixture. Runtime proofs
`gate7_buffer_release_callback_failure_is_unraisable_and_drops_export_once`,
`gate7_buffer_failed_construction_releases_acquired_export_once_under_low_heap`,
and `gate7_buffer_exporter_cycle_finalizes_once_and_collects` pin exactly-once
release, low-heap atomicity, and cyclic reclamation. Focused Slice 8 public proof
passes 3/3 tests and the runtime lease matrix passes 3/3 tests.

## DONE BY CHATGPT
