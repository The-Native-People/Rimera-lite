# Rimera working agreement

## Documentation quality

- User documentation must be explicit about every option, limit, default,
  accepted value, precedence rule, output location, and observable failure
  mode it presents. Do not use a short marketing description where a user needs
  operational meaning; verify claims in the implementation before publishing.
- Keep user-facing documentation focused on using Rimera. Explain internal
  compiler gates, ABI details, and contributor workflow only in developer
  specifications, never as prerequisites for ordinary users.

- Read `TODO.md` before selecting work so completed vertical slices are not
  repeated. Read `spec/architecture.md` before changing architecture, and read
  `spec/compatibility.md` plus `spec/rebuild.md` before implementation work.
  `TODO.md` is the resume board; the compatibility ledger is the authoritative
  record of proof, limits, and dependency order.
- Rimera's target pipeline is Python source -> resolver -> semantic analysis
  -> Rimera HIR/MIR/LIR -> Cranelift object files -> Rust-runtime linking.
- Do not add generated C, a Python bytecode interpreter, CPython, RustPython,
  or framework-specific branches to the new compiler.
- Keep compiler stages separate: parsing has no resolution effects; resolution
  has no codegen effects; semantics owns binding/effects; IR owns executable
  meaning; backend owns target lowering; runtime owns heap and exceptions.
- New runtime behavior requires a documented Rust ABI contract, an IR operation,
  backend lowering, and a public-entry native test. Do not implement semantics
  in linker glue.
- Preserve source spans and stable diagnostics. Unsupported behavior and denied
  capabilities must fail before artifact output; never silently substitute
  different Python behavior.
- Keep product definition, ABI rules, rebuild sequencing, decisions, and
  implementation evidence in the top-level `spec/` directory. Do not create
  parallel documentation trees.
- Use snake_case Rust files and behavior-describing test names. Put unit tests
  beside modules and integration tests under `tests/`.
- Every change must leave `cargo fmt --check`, `cargo clippy --workspace`, and
  `cargo test --workspace` green once the new workspace exists.
- Treat `Trash/` as read-only historical reference. Do not import its generated
  C architecture into the new implementation.

## Repository and dependency rules

- `/Users/nadhi/Desktop/tests-19th/Rimera-lite` is the repository root. Never
  run repository-wide commands against its parent directory.
- Every compiler representation has one owner: syntax, HIR, MIR, and LIR types
  may not be duplicated in adjacent crates. Adapters convert at stage borders.
- Dependency flow is one-way. Base contract crates depend on nothing above
  them; orchestration depends on stages, never the reverse. Dependency cycles
  and generic shared `utils` crates are forbidden.
- `rustpython-parser` is a syntax dependency only. Its AST cannot cross the
  `rimera-syntax` public boundary.
- `clang` is a linker driver only. A build command containing a C source file
  is a product defect.

## Ownership

- `rimera-abi`: stable value/status/root layouts shared with generated code.
- `rimera-runtime`: values, heap, garbage collection, Python operations.
- `rimera-compiler`: internal modules for core diagnostics, project inputs,
  syntax, resolution, HIR, semantic analysis, MIR, lowering, LIR, codegen,
  linking, and build orchestration. Module boundaries remain strict even though
  they do not need separate Cargo packages.
- `rimera-cli`: argument parsing and user-facing build presentation only.

## Diagnostics and proof

- User diagnostics contain a stable code, source span, and actionable message.
  They never mention implementation milestones, agents, probes, or TODO state.
- A failed build removes partial output. Unsupported syntax never falls back.
- Every runtime addition needs ABI documentation, an IR operation, codegen,
  runtime tests, and a public-entry native test.
- Tests must inspect final artifacts for forbidden CPython, generated-C,
  `setjmp`, and `longjmp` symbols.

## Compatibility progress

- Keep exactly one active compatibility gate in `TODO.md`. Update its checkbox
  and the detailed status in `spec/compatibility.md` together, only after
  end-to-end evidence passes. Never check work based on intent or scaffolding.

- Rimera is not generally Python 3.12 compatible. The native foundation is
  established, while overall language compatibility remains early. Never
  describe the project as fully compatible based on the supported fixture
  subset.
- Gate 5 functions, supported-signature calling, LEGB/closure cells, and
  structured exceptions/tracebacks are now `Implemented — conformance audit
  pending` on their single native binder/scope/exception/cleanup model. Other
  proven core areas include arbitrary integers/strings, managed collections,
  native iteration/`for`, recursive/starred unpacking, expanded calls,
  comprehensions, Gate 4 synchronous forms, and Gate 6 source generators with
  `yield`, `send`, `throw`, `close`, cleanup suspension, `yield from`, and the
  Gate 8 exactly-once GC close path for abandoned suspended generators.
- Current proven object-model partial: GC-rooted lazy builtin type identities,
  exact `rimera_type_of`, module-scope classes and instances, native attribute
  read/write/delete, descriptor precedence, supported slots, class cells, both
  supported `super` forms, user multiple inheritance, C3 MRO, subtype behavior,
  and class decorators through generic calls. Source and dynamic user
  metaclasses now implement managed prepared namespaces, most-derived
  selection, `__new__`, `__init__`, `__mro_entries__`, and metaclass descriptor
  precedence. Class suites execute as hidden native functions over prepared
  namespaces; supported `list`, `tuple`, `dict`, and `set` subclasses carry
  traced native payloads while preserving user type identity. Custom prepared
  mappings run through ordinary item protocols, and class keywords flow to
  `__prepare__`, `__new__`, and `__init__` in source order.
- Current proven generic-protocol slice: rooted `NotImplemented`, every shared
  binary family including matrix multiplication, reflected strict-subclass and
  in-place dispatch, identity comparisons, truth, length, containment, hash,
  item get/set/delete, callable, validated iteration, legacy sequence and
  reverse-iteration hooks. Managed floats, complex values, immutable bytes,
  bytearrays, slices, frozensets, and native-exporter memoryviews are
  source-to-runtime values. Builtin value families must extend these paths
  rather than add intrinsics.
- Gate 7 pulled forward a deliberately narrow native import prerequisite:
  `import name` and `import name as alias` reach an owned `ImportName` MIR/ABI
  path for explicitly registered managed module shells. The current registry
  contains `inspect` and `weakref`; repeated imports preserve managed module
  identity, each module owns a traced live `__dict__`, and normal module
  attribute mutation uses that namespace. This is infrastructure only: it does
  **not** claim the `inspect` or `weakref` stdlib APIs, arbitrary modules,
  `from ... import ...`, dotted/package imports, Python module-source execution,
  public `sys.modules`, or a user-visible `__import__` builtin. Future import
  work must extend this single native registry/module-object path rather than
  creating a second loader.
- Gate 7 is closed through all ten reflection slices and is `Implemented —
  conformance audit pending`. The proven surface includes namespace views,
  identity/type/attribute helpers, managed function/code/cell and
  exception/traceback/frame metadata, generator state/suspension metadata,
  Python 3.12 generic-declaration type metadata, audited class/type method
  tables, Python-level PEP 688 buffer providers, descendant/metaclass observer
  invalidation, cross-family GC composition, and low-heap reflective mutation
  atomicity. The narrow `inspect`/`weakref` module shells remain infrastructure,
  not stdlib API claims.
- Gate 8 synchronous context managers are closed and `Implemented — conformance
  audit pending` through compiler-planned cleanup, exact exceptional exits,
  generator lifecycle, GC, low-heap, and public native artifact proof.
- Not implemented as compatibility claims: `eval`/`exec`, weak
  reference/finalizer behavior, stdlib corpora, native stdlib bindings, or PyPI
  compatibility. Gate 10 closure also does not claim stdlib `asyncio`, networking,
  subprocesses, framework integration, or cross-thread async guarantees.
- Gates 1–10 are closed. Gate 10 proves async syntax/HIR/MIR, lazy native
  coroutines, direct/generic await, the one-root executor boundary,
  deterministic timers/wakes/cancellation and buffer lifetime, async iteration,
  comprehensions, async generators/finalization, suspended `async with` cleanup,
  static Compio selection, cross-feature GC/cancellation stress, fixed
  performance budgets, final reproducibility/native-artifact audits, and Slice
  13's guarded AOT performance qualification in its documented macOS ARM64
  boundary. Every comparable accepted p50 speed/throughput row beats CPython
  3.12.11; materialized lifecycle diagnostics remain distinct from AOT-elided
  same-source workload results. The sole active compatibility slice is
  **Gate 11 Slice 1**, covering modes, code-object contracts, capabilities, and
  the dynamic-compilation oracle matrix. Execute Gates 11–17 in
  numeric order for dynamic compilation, lifecycle semantics, language
  conformance, stdlib, extension/platform ABI, ecosystem proof, and final
  drop-in release qualification. Do not jump to frameworks, modules, or
  packages ahead of that order. Only Gate 17 may authorize a drop-in claim, and
  only for its published target/capability/stdlib/extension-ABI matrix.
- For implementation requests, begin from the earliest relevant incomplete
  dependency in `spec/compatibility.md`. Repair missing prerequisites and then
  continue the requested vertical slice; do not stop at runtime scaffolding.
- A Python capability is complete only when it passes through syntax, semantic
  analysis, MIR and verification, Cranelift lowering, the documented Rust ABI,
  GC/lifetime behavior, and a public native differential test.
- Update `spec/compatibility.md` in the same change whenever proof expands or
  invalidates a recorded status. Never move a status based on intent, a unit
  test for one layer, an ignored test, or hand-constructed MIR alone.
- Preserve and extend the existing execution kernel. Do not create parallel
  object, collection, exception, or compiler-stage models to make a feature
  appear complete.
