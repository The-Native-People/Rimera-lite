# Gate 16 — Packaging and PyPI Ecosystem Compatibility

Gate 16 is the ecosystem implementation gate. It consumes the completed
language, module, async, dynamic-code, lifecycle, stdlib, and extension gates
and proves that real projects install, build, import, run, test, and package
without Rimera-specific source branches.

It does not authorize a drop-in guarantee by itself. Gate 17 independently
audits the complete public contract and qualifies a release.

## Progress ledger

- [ ] Slice 1 — ecosystem tiers, package corpus, targets, and success metrics
- [ ] Slice 2 — project metadata, markers, dependencies, lockfiles, wheels, and sdists
- [ ] Slice 3 — Python-compatible launcher/environment behavior and tooling workflows
- [ ] Slice 4 — foundational package corpus and transitive dependency graphs
- [ ] Slice 5 — scientific/data, web, CLI, serialization, and application corpora
- [ ] Slice 6 — installation/build failures, isolation, capabilities, and reproducibility
- [ ] Slice 7 — compatibility, performance, memory, startup, and artifact matrix
- [ ] Slice 8 — ecosystem audit, published package matrix, and gate closure

## Slice acceptance

1. Define separately: Python 3.12 language compatibility, stdlib compatibility,
   source-package compatibility, and binary-extension compatibility. Select
   non-toy projects with pinned versions and transitive dependency graphs.
2. Resolve PEP 508/517/518/621 metadata, extras, markers, editable/source builds,
   wheel tags, resources, entry points, and reproducible lockfiles.
3. Support the launcher, `sys`/environment/path behavior, script entry points,
   test runners, build frontends, and subprocess expectations required by each
   ecosystem tier.
4. Prove foundational packaging/build/runtime libraries before frameworks that
   transitively depend on them.
5. Run representative real applications without framework-specific compiler
   branches and record precise pass/fail reasons.
6. Make failed resolution/build/import atomic and capability-safe; identical
   locked inputs produce identical artifacts without ambient Python state.
7. Publish correctness first, then startup, execution, memory, artifact size,
   and compile-time results against pinned CPython 3.12 on every target.
8. Run the full ecosystem boundary and publish a machine-readable matrix of
   package name, version, target, install mode, extension ABI, and result.

## Required flagship profiles

Gate 16 cannot close with only synthetic packages. Its minimum continuously
qualified application profiles are:

- **Flask:** install the locked Flask/Werkzeug/Jinja/Click/MarkupSafe graph;
  run the CLI, application factory, routing, request/response, templates,
  sessions, JSON, error handlers, test client, and a live WSGI HTTP workload.
- **FastAPI:** qualify both the base and declared standard installation tiers,
  including Starlette, Pydantic and `pydantic-core`, Uvicorn, validation,
  dependency injection, OpenAPI generation, middleware, background work,
  test clients, HTTP serving, WebSockets, and lifespan behavior.
- **discord.py:** install its locked async/network dependency graph and run
  gateway protocol simulations, event dispatch, commands, rate limiting,
  reconnect/resume, HTTP, WebSocket, SSL, cancellation, and shutdown tests.
- **Pycord:** run the corresponding bot, command, interaction, gateway,
  reconnect, and shutdown suites. Qualify base, voice, and speed extras as
  separate profiles because their native dependencies differ.

Each profile pins the framework and complete transitive graph. “Supported”
means install, import, public upstream tests applicable to the target, a real
minimal application, failure behavior, and sustained workload all pass without
patching the framework or adding a framework-specific compiler branch. Passing
one version does not imply future releases; latest-compatible versions run in a
separate tracking lane and enter the guarantee only after qualification.

Passing Gate 16 establishes measured package-ecosystem coverage. Gate 17 must
still prove that the underlying public contract is complete enough to warrant
a drop-in release guarantee.
