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
