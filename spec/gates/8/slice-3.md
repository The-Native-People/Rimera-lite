# Gate 8 Slice 3 — Targets, Multiple Managers, Partial Entry, and Ordering

## Goal

Complete `as` binding and multiple-manager acquisition/unwind semantics.

## Integrated work

- Bind supported name, attribute, item, recursive, and starred `as` targets
  through the existing assignment-target model after successful entry.
- Treat target-assignment failure as a body failure that exits the manager with
  the active exception triple.
- Evaluate and enter multiple managers left-to-right and exit successfully
  entered managers right-to-left, including parenthesized syntax.
- If later lookup, entry, or target binding fails, unwind only managers already
  entered, with correct suppression/replacement behavior and side-effect order.
- Preserve exact roots and failure-atomic visible state across target callbacks,
  manager mutation, nested allocations, and partial acquisition.

## Completion proof

Public CPython differentials cover every target family, multi-manager ordering,
partial entry, target failure, suppression/replacement during unwind, callback
mutation, forced GC, and heap limits.

## Status

Complete. Name, attribute, item, recursive, and starred targets use the existing
assignment-target lowering. Multiple and parenthesized managers enter
left-to-right and captured exits run right-to-left. Public native differentials
cover partial acquisition, target callback failure, suppression, replacement
with preserved exception context, and constrained-heap execution.
