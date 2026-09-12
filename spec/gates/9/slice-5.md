# Gate 9 Slice 5 — Plain and Dotted `import`, Aliases, and Binding Order

## Goal

Implement Python's ordinary import statement binding rules over the completed
module graph and cache.

## Integrated work

- Preserve all comma-separated aliases and dotted names in syntax/HIR with
  exact spans.
- Make `import a.b` bind `a`, while `import a.b as c` binds the requested leaf
  module to `c`; publish child modules on their parent packages.
- Support imports in module, function, class, branch, loop, handler, generator,
  and context-manager scopes through normal binding ownership.
- Preserve left-to-right import and binding side effects when a later alias
  fails.

## Completion proof

Public CPython differentials cover every binding location, dotted/aliased
identity, repeated aliases, ordering, partial statement failure, tracebacks,
and GC pressure.
