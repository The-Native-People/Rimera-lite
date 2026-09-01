# Gate 4 Slice 21 — Sequence and Mapping Patterns

## Goal

Implement Python sequence and mapping structural patterns on generic protocols.

## Implementation

- Sequence patterns use the approved sequence protocol, exclude strings and
  byte-oriented values where Python excludes them, and support one star in any
  legal position.
- Mapping patterns evaluate keys in source order, use mapping lookup semantics,
  reject duplicate keys, and support `**rest` creation.
- Nest all existing pattern forms recursively.
- Apply tentative binding/rollback from Slice 19 at every nested failure.
- Root extracted values, iterators/views, and rest mappings across callbacks.
- Preserve lookup/equality exceptions and avoid observable mutation of inputs.

## Proof

- CPython differentials cover native/user sequences and mappings, stars/rest,
  nested patterns, missing/duplicate keys, callback side effects, and GC.
