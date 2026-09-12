# Gate 9 Slice 3 — Per-Module MIR/Object Emission, Linking, and Native Initialization

## Goal

Compile every reachable source module into a native initializer and link the
ordered object set into one executable.

## Integrated work

- Analyze each module in its own globals namespace and emit one verified MIR
  program and one deterministic object per canonical module.
- Give native module initializers an opaque ABI that receives the authoritative
  managed module object and returns ordinary `RStatus`.
- Link all reachable objects exactly once, invoke the entry module through the
  same initializer contract, and preserve one Python `<module>` traceback frame
  per source module rather than compiler chunk frames.
- Include module source hashes, target, profile, runtime ABI, and object hashes
  in deterministic object/cache keys.

## Completion proof

Public chain/diamond programs match CPython output, side-effect order, globals
isolation, and cross-module tracebacks. Repeated builds produce identical graph,
object ordering, and manifest data; final artifacts remain native-only.
