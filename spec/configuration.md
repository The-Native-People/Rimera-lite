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
```

The table is read from the project root (the nearest ancestor containing
`pyproject.toml`). Command-line flags override its values. `output` is resolved
relative to that project root. A build still requires an output path unless
`--run` or `run = true` selects the cache-backed executable path.

Examples:

```shell
RIMERA_COLOR=always rimera build main.py -o dist/app
RIMERA_CACHE_DIR="$PWD/.rimera" rimera build main.py -o dist/app
RIMERA_RUNTIME_ARCHIVE=/opt/rimera/librimera_runtime.a rimera build main.py -o dist/app
```

The cache is disposable. It contains compiler intermediates only and must not
be committed to source control. Object names are deterministic for the entry
point, output path, target, profile, and effective managed-heap limit.

When `rimera <entry> --run` is used without `--output`, the compiled executable
is placed under `<entry-parent>/.rimera/bin/<entry-stem>`, run immediately, and
its exit status becomes Rimera's exit status.

Generated executables use the same color policy for uncaught tracebacks:
`auto` colors only a terminal and honours `NO_COLOR`; `always` is useful when
capturing ANSI output deliberately, while `never` keeps diagnostics plain.

## Managed heap limit

`rimera build main.py -o app --heap-limit-bytes 1048576` embeds an optional
per-context managed-memory budget in the executable. The default is unlimited.
The value controls estimated Rimera heap bytes rather than total process memory;
reachable objects that exceed it produce a nonzero runtime failure.

## Debug builds

`rimera build --debug` enables compiler trace output and writes a readable
`.ir.py` file under `.rimera/ir/`. It begins with `# IR REPRESENTATION` and
uses Python-like assignments and `print(...)` calls for MIR values. This is a
debugging view, not source Python that Rimera re-compiles. Passing an IR dump
to `rimera build` is rejected with `RIM-INPUT-002`; compile the original entry.
