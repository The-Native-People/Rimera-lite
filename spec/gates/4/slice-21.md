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

## Completion evidence

- Owned syntax/HIR now represents sequence, star, mapping, and nested structural
  patterns directly; the former `RIM-CAP-G4-21` parser boundary is removed.
- MIR adds explicit `PatternSequence`, `PatternMappingCheck`, and
  `PatternMapping` operations. `gate4_structural_pattern_extractors_publish_exact_safepoint_roots`
  proves subjects and evaluated keys are rooted at every extractor safepoint and
  that the same operations carry local exception successors inside protected CFG.
- Lowering evaluates mapping length before key expressions, evaluates keys in
  source order, stores captures only in a tentative hidden dictionary, and
  commits real bindings only after the complete nested pattern succeeds.
- Runtime sequence matching excludes `str`/`bytes`/`bytearray`, validates
  exact/minimum lengths, and reuses generic iterator unpacking so a successful
  star produces a real list. Mapping matching uses ordinary hashing/equality and
  `get` semantics, detects dynamic duplicate keys, and creates `**rest` by
  copying/removing from a new dictionary rather than mutating the input.
- `gate4_sequence_and_mapping_patterns_match_cpython_312` matches CPython
  3.12.11 for exact/starred/native-subclass sequences, nesting, exclusions,
  rollback, mapping lookup/rest, missing keys, callback order, and duplicate
  dynamic keys.
- `gate4_sequence_and_mapping_patterns_survive_forced_gc` runs nested sequence
  plus mapping/rest extraction under a 32 KiB managed-heap limit while user
  length/get callbacks allocate heavily, proving subjects, extracted values,
  tentative captures, and rest mappings survive collection pressure.
- Public artifacts use the native-only forbidden-symbol scan; no generated C,
  CPython ABI, bytecode interpreter, or alternate execution path is introduced.

## DONE BY CHATGPT
