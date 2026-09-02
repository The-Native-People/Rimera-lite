# Gate 4 Slice 17 — Set and Dictionary Comprehensions

## Goal

Compile set and dictionary comprehensions through the Slice 15 scope/CFG and
the permanent Gate 3 hash tables.

## Implementation

- Add set and key/value sinks to comprehension MIR.
- Evaluate dictionary keys before values for each produced entry.
- Insert through generic hash/equality dispatch without retaining Rust borrows
  across callbacks.
- Preserve equal-key insertion position and replacement semantics.
- Root results, iterators, keys, values, targets, and closures across callbacks.
- Propagate hashing/equality/mutation failures without corrupting the result.

## Proof

- Differentials cover nesting, filters, duplicate keys/elements, custom hash
  and equality, side effects, failures, and scope isolation.
- GC tests force collection and unrelated table mutation inside callbacks.

## Completion evidence

- `<setcomp>` and `<dictcomp>` reuse the Slice 15 hidden-scope/CFG machinery
  and insert through additive `rimera_set_insert` and
  `rimera_dictionary_insert` ABI operations backed by the permanent Gate 3
  ordered hash table. Dictionary lowering evaluates each key before its value.
- `gate4_set_dict_comprehensions.py` matches CPython 3.12 for nested iteration,
  duplicate elements/keys, replacement without key movement, source-order side
  effects, custom `__hash__`/`__eq__`, membership, and structured hash failure.
- MIR safepoint proof pins set/result values and dict result/key/value roots.
  Runtime proof verifies failed unhashable inserts leave no partial entry, while
  the existing forced-GC hash callback test mutates unrelated table state and
  confirms revalidated native entry identifiers remain correct.

## DONE BY CHATGPT
