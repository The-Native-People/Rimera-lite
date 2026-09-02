# Gate 5 Slice 5 — Exception Objects, Hierarchy, Normalization, and Traceback API

## Goal

Complete the managed Python exception value model used by all native failures.

## Integrated work

- Complete the synchronous exception hierarchy required by the finished core,
  including user subclasses, exact type identity, `args`, construction, string
  rendering, matching, and traced instance state.
- Normalize raised classes and instances through the ordinary class/call path;
  reject invalid raised values with the correct replacement exception.
- Complete `__traceback__`, `with_traceback`, cause, context, and suppression
  state without exposing raw runtime pointers.
- Preserve source spans, native frame order, traceback immutability rules, and
  exception identity across calls, reraises, and handler bindings.
- Root exception/cause/context/traceback cycles during propagation and reclaim
  them after the last reachable handler/frame reference disappears.

## Completion proof

Runtime tests and public differentials cover builtin and user exception classes,
normalization failures, metadata mutation boundaries, traceback replacement,
cycles, forced GC, and low-heap failures.
