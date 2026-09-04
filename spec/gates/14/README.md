# Gate 14 — Python 3.12 Standard Library

Gate 14 supplies the standard-library surface expected by ordinary Python 3.12
programs. Prefer the upstream pure-Python implementation when it runs unchanged
on Rimera; add Rust-native modules only where CPython normally relies on a
builtin/extension/platform primitive or where bootstrapping requires one.

Every claimed module must pass its applicable upstream CPython tests plus
Rimera public-entry, packaging, GC, capability, and cross-platform proof.

## Progress ledger

- [ ] Slice 1 — module inventory, support tiers, bootstrap order, and test harness
- [ ] Slice 2 — import/bootstrap, encodings, codecs, `io`, and filesystem paths
- [ ] Slice 3 — data structures, algorithms, text, parsing, and serialization
- [ ] Slice 4 — numbers, dates, randomness, compression, hashing, and databases
- [ ] Slice 5 — OS/process/threading/signals/networking primitives and wrappers
- [ ] Slice 6 — testing, logging, diagnostics, packaging helpers, and developer tools
- [ ] Slice 7 — `asyncio`, concurrent execution, subprocesses, and shutdown behavior
- [ ] Slice 8 — full corpus, platform matrix, documentation, and gate closure

## Slice acceptance

1. Classify every Python 3.12 stdlib module as pure Python, native-backed,
   platform-specific, intentionally unavailable, or test-only with a reason.
2. Establish the encoding/import/I/O bootstrap cycle without frozen Python
   bytecode or CPython extension dependencies.
3. Run unchanged library modules for collections, functional tools, regex/text,
   JSON/CSV/config/parser, copy/pickle, and related families against corpora.
4. Complete mathematical, temporal, secure-random, archive/compression, hash,
   and database families through shared native primitives.
5. Provide capability-governed filesystem, environment, process, thread,
   signal, socket, select, SSL, and platform operations with faithful errors.
6. Make standard diagnostics and tooling work without requiring CPython at
   runtime, including unittest/doctest where applicable.
7. Run `asyncio`, futures/executors, subprocess and shutdown tests over the Gate
   10 protocols and the same platform primitives.
8. Publish a machine-readable module/support matrix and close only after the
   selected CPython stdlib suite passes on every supported target.

Platform exclusions must remain visible; “stdlib compatible” cannot mean only
that modules import successfully.
