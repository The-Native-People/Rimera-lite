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
