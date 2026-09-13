# Rimera configuration

Rimera accepts a small, explicit environment surface. Environment variables
configure build tooling only; they do not silently alter Python semantics.

| Variable | Values | Effect |
| --- | --- | --- |
| `RIMERA_CACHE_DIR` | Path whose final component is `.rimera` | Replaces the default `<project>/.rimera` cache directory. Native `.o` intermediates are always written under its `objects/` subdirectory. |
| `RIMERA_RUNTIME_ARCHIVE` | Existing path to `librimera_runtime.a` | Overrides the workspace-built Rust runtime archive used at link time. |
| `RIMERA_COLOR` | `auto` (default), `always`, `never` | Controls ANSI color in the Rimera CLI and generated executable diagnostics. |

Project defaults may be defined in `pyproject.toml`:

```toml
[tool.rimera]
output = "dist/app"
profile = "release"          # `debug` or `release`
debug = true
heap_limit_bytes = 1048576
run = false
target = "aarch64-apple-darwin"
async = "auto"             # `auto`, `compio`, `monoio`, or `tokio`
dynamic_compilation = false # opt in to Gate 11 runtime compilation
module_roots = ["."]        # ordered import roots, relative to the project root
```

The table is read from the project root (the nearest ancestor containing
`pyproject.toml`). Command-line flags override its values. `output` is resolved
relative to that project root. A build still requires an output path unless
`--run` or `run = true` selects the cache-backed executable path.

`async` selects the statically linked executor backend for source-level async
root execution. `auto` (the default) selects Compio when the compiled module
graph reaches `rimera.async_runtime`; `compio` selects it explicitly. `monoio`
and `tokio` are recognized future selections and currently fail before artifact
publication with `RIM-ASYNC-001`. Command-line `--async` overrides the project
setting. Synchronous source neither links nor advertises an async backend even if
`compio` is selected explicitly.

`dynamic_compilation` is `false` by default. Setting it to `true`, or passing
`--dynamic-compilation`, grants the Gate 11 runtime-compilation capability and
links the native compiler service. Capability-free artifacts omit the service.
The CLI flag can enable the capability for one build; like `--run`/`--debug`, it
does not provide a negative override for a project setting that is already true.

`module_roots` is an ordered array of path strings used for project-module
discovery. Relative entries are resolved from the project root. The default is
`["."]`. Providing the option replaces that default; duplicate canonical roots
are removed while preserving the first occurrence. Every root must exist and
remain inside the project root. Rimera never appends the working directory, an
ambient `PYTHONPATH`, a virtual environment, or the system Python search path.
Directories without `__init__.py` form namespace-package portions in this
declared order; a regular module or package found in a later root takes
precedence over accumulated namespace portions, matching Python import search.

Examples:

```shell
RIMERA_COLOR=always rimera-lite build main.py -o dist/app
RIMERA_CACHE_DIR="$PWD/.rimera" rimera-lite build main.py -o dist/app
RIMERA_RUNTIME_ARCHIVE=/opt/rimera/librimera_runtime.a rimera-lite build main.py -o dist/app
rimera-lite main.py --async compio -o dist/app
rimera-lite main.py --dynamic-compilation -o dist/app
```

The cache is disposable. It contains compiler intermediates only and must not
be committed to source control. Object names are deterministic for the entry
point, output path, target, profile, and effective managed-heap limit.

When `rimera-lite <entry> --run` is used without `--output`, the compiled executable
is placed under `<entry-parent>/.rimera/bin/<entry-stem>`, run immediately, and
its exit status becomes Rimera's exit status.

Generated executables use the same color policy for uncaught tracebacks:
`auto` colors only a terminal and honours `NO_COLOR`; `always` is useful when
capturing ANSI output deliberately, while `never` keeps diagnostics plain.

## Managed heap limit

`rimera-lite build main.py -o app --heap-limit-bytes 1048576` embeds an optional
per-context managed-memory budget in the executable. The default is unlimited.
The value controls estimated Rimera heap bytes rather than total process memory;
reachable objects that exceed it produce a nonzero runtime failure.

## Debug builds

`rimera-lite build --debug` enables compiler trace output and writes a readable
`.ir.py` file under `.rimera/ir/`. It begins with `# IR REPRESENTATION` and
uses Python-like assignments and `print(...)` calls for MIR values. This is a
debugging view, not source Python that Rimera re-compiles. Passing an IR dump
to `rimera-lite build` is rejected with `RIM-INPUT-002`; compile the original entry.
