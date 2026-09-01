# Gate 3 Slice 6 — Generic Dict/Set Collision Correctness

## Goal
Make the permanent generic hash tables safe under Python callbacks, collisions, mutation, deletion, reinsertion, and exceptions.

## Remaining implementation
- Audit lookup/update/remove paths that snapshot entries before calling user `__eq__` or `__hash__`.
- After any user callback, do not trust a previously captured entry index without revalidating that the entry still exists and is still the intended candidate.
- Correct behavior when `__hash__` mutates the same dictionary/set.
- Correct behavior when `__eq__` deletes the candidate key/element.
- Correct behavior when `__eq__` inserts new entries, clears the collection, replaces values, or triggers resizing/tombstones.
- Preserve insertion order under replacement, delete/reinsert, and collision chains.
- Propagate callback exceptions without corrupting table/bucket/order state.
- Ensure equal-but-distinct keys replace values without replacing the original stored key identity where CPython preserves it.
- Recheck mixed numeric keys using Slice 5 equality/hash rules.
- Implement dictionary union operators: `dict | mapping` according to CPython operand restrictions/result type.
- Implement mutable `dict |= mapping/iterable-pairs` using the same update semantics and preserving receiver identity.
- Keep dict views live after every mutation.

## Completion criteria
The generic table remains structurally valid and semantically correct even when hashing/equality callbacks mutate the collection or raise.

## DONE BY CHATGPT
