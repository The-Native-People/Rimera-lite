# Rimera specifications

- [Compatibility resume board](../TODO.md) — concise checked/unchecked work
  tracker and the single active implementation gate.

- [Architecture](architecture.md) — authoritative product and compiler design.
- [Native ABI v1](abi-v1.md) — `RValue`, statuses, roots, and context rules.
- [Rebuild sequence](rebuild.md) — implementation order.
- [Python 3.12 compatibility ledger](compatibility.md) — proven language
  surface, known limits, and mandatory next-work dependency order.
- [Foundation evidence](foundation.md) — implemented native milestone and proof.
- [Architecture decisions](decisions.md) — irreversible technical choices.
- [Configuration](configuration.md) — supported environment variables and cache rules.
- [Gate 4 execution slices](gates/4/README.md) — dependency-ordered contracts
  for unpacking, expanded calls, comprehensions, and remaining synchronous
  syntax.

The remaining `tui/` files are display references, not compiler contracts.

## Contract policy

Only versioned specifications with tests and compatibility rules are public
contracts. MIR serialization, build manifests, capability schemas, and native
module-object metadata remain internal until they receive their own versioned
specification.
