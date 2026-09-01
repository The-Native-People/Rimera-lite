# Gate 3 Slice 17 — Dictionary View Finishing

## Goal
Finish the normal behavior of live, traced `dict_keys`, `dict_values`, and `dict_items` objects.

## Completed implementation
- Add reverse iteration for keys, values, and items in reverse dictionary insertion order.
- Extend recursive repr protection to views so self-referential view structures render `...` rather than recurse indefinitely.
- Expose the Python 3.12 `.mapping` property if Gate 3 claims the normal dictionary-view surface.
- Ensure keys/items set-like operators (`|`, `&`, `-`, `^`) continue to use generic hash/equality after the source dictionary mutates.
- Preserve set-like comparison semantics for keys/items.
- Preserve identity-style equality behavior for values views.
- Preserve liveness: existing views must immediately reflect source dictionary insertions, deletions, replacement, and clear operations.
- Preserve tracing: a live view must keep its dictionary reachable and participate correctly in cycles.
- Recheck forward and reverse iteration invalidation rules when dictionary size changes during iteration.
- Propagate callback exceptions from hashing/equality/membership operations.

## Completion criteria
All three view kinds remain live/traced and their supported repr, iteration, reverse iteration, equality, and set-like behavior match CPython.

## Completion evidence
- `dict_keys`, `dict_values`, and `dict_items` remain live handles over the source dictionary; insert, delete, replacement, and clear operations are visible immediately through existing views.
- Dictionary-view tracing keeps the source dictionary reachable, including view/dictionary cycles, without copying the dictionary payload into the view.
- Forward and reverse view iterators follow insertion order / reverse insertion order and reject dictionary size changes through the shared collection-version invalidation path.
- `.mapping` returns a traced, live `mappingproxy` over the source dictionary and rejects writes through the normal item-assignment path.
- Keys/items retain set-like comparisons and `|`, `&`, `-`, and `^` behavior through the generic hashing/equality collection machinery; callback failures propagate through those ordinary protocol calls. Values views keep CPython's identity-style equality behavior.
- Dictionary-view repr participates in the shared recursion guard, so recursive view structures render finite `...` markers rather than recursing indefinitely.

## Proof
`gate3_dictionary_view_finishing_matches_cpython_312` compares live mutation, reverse iteration, `.mapping`, set-like operations/comparisons, values-view equality, recursive repr, iterator invalidation, and proxy read-only behavior byte-for-byte with `/opt/homebrew/bin/python3.12`. The broader native Gate 3 dictionary/hash regressions remain green.

## DONE BY CHATGPT
