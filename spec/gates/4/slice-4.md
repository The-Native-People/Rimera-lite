# Gate 4 Slice 4 — General Starred Unpacking

## Goal

Support one starred target in any legal position at every nesting level.

## Implementation

- Implement leading, middle, trailing, and nested starred targets.
- Collect the starred result into a new native list while retaining the exact
  prefix and suffix counts.
- Consume arbitrary generic iterators once; do not require length or rewind.
- Match CPython too-few errors and preserve target-store ordering.
- Reject multiple stars at one sequence level during semantic analysis.
- Ensure growth of the starred list remains exactly rooted under forced GC and
  heap-limit failure.

## Public fixtures

- `*head, tail`, `head, *middle, tail`, and nested equivalents.
- Custom single-pass and failing iterators.
- Attribute/item leaves and partial-store visibility on failure.

## Completion criteria

All legal target positions execute natively and invalid forms emit stable
source diagnostics without an artifact.

## Completion evidence

- MIR `Unpack` records independent fixed-prefix and fixed-suffix counts plus
  the starred flag. Lowering derives them from the direct starred child at each
  recursive sequence level, so leading, middle, trailing, and nested stars use
  one execution path.
- The additive documented ABI
  `rimera_unpack_ex(context, value, before_count, after_count, starred, output)`
  consumes a generic iterator exactly once. The original `rimera_unpack` entry
  remains available for the ABI-v1 exact/trailing-star subset.
- Runtime extended unpacking consumes the prefix, drains the remaining
  iterator once, splits the fixed suffix, and publishes the middle segment as
  a newly allocated native list. Managed consumed values are rooted across
  every `next`, list allocation, and final value-array allocation.
- Too-few extended unpacking raises the managed `ValueError` identity with the
  CPython `expected at least N, got M` message. The runtime ABI regression also
  pins prefix/star/suffix result order.
- Semantic target-tree validation rejects multiple stars at the same sequence
  level as `RIM-CAP-G4-02`; the public build fixture verifies the diagnostic
  span and that no artifact is emitted.
- `gate4_starred_unpacking.py` covers leading, middle, trailing, and nested
  stars over a custom single-pass iterator, starred attribute/item leaves, a
  newly allocated list result, and failure atomicity. The low-heap runtime
  regression forces collection during starred-list allocation and proves the
  source/managed children remain rooted until the allocation failure returns.

## Proof

`gate4_general_starred_unpacking_matches_cpython_312` is byte-for-byte green
against CPython 3.12.11. Runtime proofs
`gate4_extended_unpack_abi_preserves_prefix_star_suffix_and_value_errors` and
`gate4_starred_unpack_low_heap_roots_source_until_memory_error` are green. The
full workspace passes 193 tests with zero ignored, clippy is warning-free, and
the release artifact remains 485,048 bytes.

## DONE BY CHATGPT
