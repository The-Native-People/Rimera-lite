# Gate 3 Slice 2 — Finish Slicing Mutation Correctness

## Goal
Make list and bytearray slice mutation follow CPython 3.12 semantics for empty slices, extended slices, negative steps, clipping, resizing, and deletion.

## Remaining implementation
- Fix empty ordinary list slice insertion. `x[1:1] = values` must insert at normalized start index 1 even though the normalized index list is empty.
- Fix the same empty-slice insertion behavior for `bytearray`.
- Fix cases such as `x[3:1] = values`: with a positive step the replacement is inserted at the normalized start rather than incorrectly falling back to the end.
- Fix heavily clipped negative/positive bounds where an empty selected index list still has a meaningful insertion point.
- Refactor slice normalization so assignment can retain normalized `(start, stop, step)` information instead of deriving insertion position from the selected-index vector.
- Fix negative-step slice deletion for lists. Deletions must remove physical indexes in descending numerical order so shifting indexes cannot corrupt subsequent removals.
- Fix the same negative-step deletion logic for bytearrays.
- Recheck extended slice assignment with positive and negative steps: replacement length must exactly equal selected slice length when `step != 1`.
- Recheck bytearray slice resizing while memoryviews export the buffer. Any resize while exports are active must fail with the proper buffer error.
- Keep non-resizing bytearray slice replacement legal where CPython permits it.
- Ensure mutation attempts on immutable builtin families (`str`, `bytes`, `tuple`, `range`, `frozenset`, immutable memoryview cases) fail through the correct structured exception path.
- Preserve generic `__index__` handling for slice bounds and steps.
- Preserve zero-step rejection.

## Completion criteria
- Empty-slice insertion works at every normalized position.
- Negative-step deletion cannot shift or panic on indexes.
- Extended-slice length rules match CPython.
- Bytearray export/resize rules are preserved.

## Out of scope
- Additional source-level slice syntax owned by Gate 4. This slice concerns the runtime semantics once a `slice` object reaches item operations.

## DONE BY CHATGPT
