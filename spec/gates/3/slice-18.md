# Gate 3 Slice 18 — Managed-Size Accounting

## Goal
Make Gate 3 builtin collections account for their real dynamic managed storage so GC thresholds and heap limits remain meaningful.

## Completed implementation
- Audit `HeapObject::managed_size()` for every Gate 3 builtin family.
- For list-like vectors, account for allocated capacity rather than only logical length where the allocation is retained.
- For generic dictionaries/sets/frozensets, include the permanent `OrderedHashTable` dynamic storage rather than only `len * size_of(RValue)` approximations.
- Include entry vector capacity.
- Include insertion-order/tombstone vector storage.
- Include hash-bucket vectors and bucket capacities.
- Include map/node/metadata allocations used by the hash-to-bucket index.
- Include per-entry pair storage for dictionaries.
- Include string-key capacity only for any legacy dictionary representation that still legitimately remains.
- Recheck bytes and bytearray capacity accounting.
- Recheck memoryview format/shape/stride/suboffset dynamic storage.
- Recheck slice/dictionary-view fixed-size accounting.
- Avoid double-counting exporter/source objects: each heap object accounts for its own owned allocation only.
- Ensure allocation/resizing paths update the context's managed-byte tracking consistently.

## Completion criteria
Large Gate 3 collections cannot evade heap limits by allocating substantial vector/hash-table capacity that `managed_size()` fails to count, and the existing low-heap GC regressions remain valid.

## Completion evidence
- `HeapObject::managed_size()` accounts list storage by retained vector capacity, bytes/bytearray by owned capacity, legacy string-key dictionaries by entry capacity plus owned key capacity, and slice/dictionary-view/mapping-proxy objects by their fixed owned storage.
- Generic dictionaries, sets, and frozensets delegate to `OrderedHashTable::managed_size()`, which includes entry-vector capacity (including dictionary key/value pair storage), insertion-order/tombstone capacity, hash-bucket vector capacities, and the hash-index map/node metadata estimate.
- Memoryviews count only their owned dynamic metadata (`format`, shape, strides, and suboffsets); exporter/source allocations remain owned and counted by the exporter itself rather than being double-counted by each view.
- In-place runtime mutation refreshes managed-byte accounting before returning across the protected runtime boundary, so retained collection growth can deterministically trip configured heap limits.
- Corrected accounting exposed startup pressure against the public 8 KiB heap contract; `BufferError`, `GeneratorExit`, and `StopIteration` now stay lazily materialized through the existing builtin-exception path instead of inflating every fresh context.

## Proof
- `ffi::tests::ordered_hash_table_managed_size_keeps_retained_capacity_visible` proves deleted entries do not hide retained table capacity.
- `ffi::tests::in_place_collection_growth_refreshes_managed_bytes_and_heap_limit` proves mutation refresh and heap-limit enforcement.
- `ffi::tests::lazy_kernel_fits_the_public_low_heap_budget` preserves the lazy-startup budget after corrected accounting.
- Public native regressions `dead_values_are_reclaimed_under_a_small_heap_limit` and `reachable_values_over_heap_limit_fail_deterministically` prove collection still reclaims dead values while reachable managed storage deterministically raises `MemoryError`.

## DONE BY CHATGPT
