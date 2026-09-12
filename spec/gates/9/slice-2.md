# Gate 9 Slice 2 — Deterministic Project Discovery, Graph Edges, and Diagnostics

## Goal

Resolve the reachable local source graph without executing code or consulting
ambient Python state.

## Integrated work

- Discover project-owned `.py` modules from canonical imports and project roots.
- Parse reachable files once, record ordered import edges and source hashes, and
  terminate cycles without recursive duplication.
- Reject traversal outside declared roots, ambiguous module/package ownership,
  invalid relative levels, missing sources, duplicate canonical identities, and
  case-colliding paths with stable diagnostics before artifact output.
- Make graph order deterministic independently of filesystem enumeration order.

## Completion proof

Public project fixtures cover chains, diamonds, cycles, missing modules, path
escape, ambiguity, case collision, malformed dependencies, and no-artifact
failures. Resolver unit tests pin identical graphs and hashes across repeated
runs.
