# Gate 4 Slice 6 — Mapping Displays and Dictionary Unpacking

## Goal

Implement `{**mapping}` and mixed dictionary displays with Python evaluation
and replacement order.

## Implementation

- Add ordered dictionary-display HIR entries for key/value and unpack forms.
- Evaluate entries left-to-right exactly once.
- Expand native and user mappings through the generic mapping protocol.
- Require string keys only where Python requires them; ordinary dictionary
  display unpacking accepts any hashable key.
- Preserve first insertion position when an equal key is replaced.
- Propagate hash, equality, iteration, and mapping failures without corrupting
  the destination table.
- Add MIR operations or generic calls with explicit exception edges and exact
  root plans; generated code must not inspect hash-table storage.

## Proof

- Differentials cover duplicate keys, side effects, custom mappings, collisions,
  and failures during expansion.
- GC tests force collection during hash/equality callbacks.

## Completion evidence

- Syntax and HIR preserve one ordered dictionary-display entry stream containing
  ordinary key/value pairs and `**mapping` expansions. Lowering mutates the
  destination immediately after each source entry rather than evaluating all
  expressions up front, preserving Python's callback/failure order.
- Ordinary pairs emit generic `ItemSet`; expansion emits the fallible MIR
  `DictionaryMerge` operation and the documented additive
  `rimera_dictionary_merge` ABI. Generated code never inspects dictionary
  storage.
- Native and user mappings expand through mapping protocols only (`keys()` plus
  generic item access); iterable-of-pairs fallback is deliberately excluded
  from display unpacking. Arbitrary hashable dictionary keys remain legal.
- Equal-key replacement reuses the existing generic hash/equality table path,
  retaining the first stored key/insertion position while replacing its value.
  Hash, equality, key iteration, item access, and mapping failures propagate at
  the source entry that triggered them.
- `gate4_dictionary_unpacking_matches_cpython_312` is byte-for-byte green for
  mixed displays, visible side effects, custom mappings, collisions, duplicate
  replacement, non-string keys, and expansion failure. The former Slice 6
  capability fixture is now a positive smoke.
- `gate4_dictionary_merge_roots_values_across_hash_and_equality_callbacks`
  forces tracing collection from user `__hash__` and `__eq__` callbacks and
  proves the destination, source, first key identity, and replacement value
  remain valid throughout the merge.

## DONE BY CHATGPT
