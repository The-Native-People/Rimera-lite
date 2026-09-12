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

## Implemented contract

- Managed exceptions now carry a traced optional instance dictionary in addition to type, live `args`, traceback, cause, context, suppression, and exception-group metadata. Generic attribute dispatch exposes user fields/methods plus `args`, `__dict__`, `__traceback__`, `__cause__`, `__context__`, `__suppress_context__`, and `with_traceback` with Python-shaped mutation validation.
- Builtin exception classes are valid user-class bases. User exception construction runs the ordinary class/call path and descriptor-bound `__init__`; raising a class normalizes it through that same call path, while raising an existing instance preserves identity.
- Explicit source raises attach the managed traceback frame before a same-function handler receives the exception; escaping native functions continue to append their frame. Traceback objects expose `tb_lineno` and `tb_next`, with CPython-shaped read-only/validation behavior.
- Exception dictionaries, causes, contexts, traceback links, and self-cycles participate in the normal tracing graph. Metadata allocation failure does not partially publish an exception dictionary.

## Completion proof

- `gate5_slice5_exception_objects_tracebacks_and_normalization_match_cpython_312` matches CPython 3.12.11 under a 96,000-byte heap limit for user exception subclasses and keyword `__init__`, `args`/string rendering, arbitrary instance state, class-vs-instance raising, identity, traceback line chains, `with_traceback`, cause/context/suppression mutation, metadata type errors, invalid raised values, and cyclic exception metadata.
- `gate5_exception_graphs_survive_low_heap_collection_and_failed_metadata_is_atomic` proves a rooted self-referential exception graph survives repeated forced collections under bounded headroom, dead temporary exception graphs are reclaimed, and a zero-headroom metadata allocation failure leaves the rooted exception valid with no partially installed `__dict__`.

## Acceptance evidence — 2026-09-02

- Oracle: `/opt/homebrew/bin/python3.12 --version` -> `Python 3.12.11`.
- Public Slice 5 differential passes under a 96,000-byte managed heap limit with exact stdout/stderr/status comparison; the runtime low-heap graph/failure-atomicity test also passes.
- `spec/abi-v1.md` documents traced exception dictionaries/metadata, class normalization through the generic call path, same-function raise-site traceback attachment, and traceback attribute behavior.
- Integration boundary: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `cargo test --doc --workspace`, and `git diff --check` all pass.
- Full workspace test total: 262 passed, 0 failed, 0 ignored (3 ABI + 8 CLI + 40 compiler unit + 153 public native-pipeline + 58 runtime).
- At this Slice 5 boundary promotion remained Slice 8-owned; the later Slice 8
  audit closed Gate 5. Gates 7, 8, and Gate 9 Slice 1 have also closed; Slice 2 is the
  current active implementation tracker.

## DONE BY CHATGPT
