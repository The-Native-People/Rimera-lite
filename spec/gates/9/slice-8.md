# Gate 9 Slice 8 — Namespace Packages, Search Roots, and Parent Publication

## Goal

Support deterministic namespace-package composition across explicitly declared
project/package roots.

## Integrated work

- Resolve namespace portions only from ordered declared roots; ambient
  `sys.path`, working-directory accidents, and implicit venv paths are ignored.
- Build one namespace package identity with ordered `__path__` locations and
  coherent specs, then publish loaded children on every parent.
- Define regular-package precedence, duplicate portion handling, root
  ambiguity/collision diagnostics, and cache invalidation inputs.
- Keep path/spec/location graphs managed and collectible.

## Completion proof

Public multi-root fixtures match CPython under the explicitly mirrored path
order for merging, precedence, child discovery, metadata, identity, failures,
and repeated constrained-heap imports.
