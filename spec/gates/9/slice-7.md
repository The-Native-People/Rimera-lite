# Gate 9 Slice 7 — Regular Packages, `__init__`, Relative Imports, and Metadata

## Goal

Make regular packages and relative imports execute through the same module
state machine as source modules.

## Integrated work

- Resolve package directories through `__init__.py`, initialize every parent
  before a child, and publish child attributes on ready parents.
- Resolve explicit relative levels from the importing module's canonical
  package and reject beyond-top-level imports.
- Populate coherent managed `__name__`, `__package__`, `__file__`, `__path__`,
  `__loader__`, and `__spec__` metadata without exposing compiler paths or
  pointers as runtime implementation details.
- Preserve package cycles, initialization order, rollback, and traceback paths.

## Completion proof

Nested-package fixtures match CPython for parent/child order, every relative
level, metadata values/types/identity, cycles, errors, and GC behavior.

## Status

Implemented. The resolver normalizes explicit relative levels from the owning
canonical package before semantic analysis; regular-package initializers use
the same cache/state machine as source modules. Public native proof is
`regular_packages_relative_imports_and_metadata_match_cpython`, including
parent/child order, level-one and level-two imports, a reentrant package cycle,
managed loader/spec identity, and a constrained heap. The stable no-artifact
beyond-top-level boundary is
`relative_import_beyond_top_level_fails_before_artifact_output`.
