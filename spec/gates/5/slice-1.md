# Gate 5 Slice 1 — Ownership, Oracle, and Gap Matrix

## Goal

Establish the exact delta between the proven native function/scope/exception
kernel and the Gate 5 contract before changing representations.

## Integrated work

- Inventory Python 3.12 function forms, parameter shapes, decorators,
  annotations, scope declarations, cell transitions, raise forms, handlers,
  exception groups, and cleanup transfers.
- Map every behavior to its single syntax, HIR, semantic, MIR, ABI, runtime,
  and public-test owner. Remove or consolidate duplicate stage-local contracts.
- Build a CPython 3.12.11 differential matrix covering success, error type,
  message boundary, traceback order/position, side effects, and cleanup order.
- Classify each gap into Slices 2–7. Later-gate reflection, imports, dynamic
  code, and async behavior must retain stable capability diagnostics and emit
  no artifact.
- Pin the existing supported behavior so closure work cannot regress generator
  expressions, class bodies, comprehensions, descriptors, or structured
  exception cleanup.

## Completion proof

- Focused syntax/sema and MIR ownership tests cover every Gate 5 input form.
- Public negative tests prove stable spans/codes and no artifacts for excluded
  later-gate behavior.
- The gap matrix names one owner and one later slice for every unresolved item;
  it contains no generic "function support" or "exception support" bucket.
