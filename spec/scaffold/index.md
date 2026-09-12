# Scaffold projects for python with rimera.

```bash
rimera-lite scaffold basic
```

or

```bash
rimera-lite scaffold
```

```
- root
---|
   |- main.py
   |- README.md
   |- pyprojects.toml
```

pyprojects.toml
``toml
[tools.rimera]
output = "dist/app"
profile = "release"
debug = false
heap_limit_bytes = 1048576
run = false
target = "aarch64-apple-darwin"
```

## Create your own scaffolds

Naming, use template-<YOUR NAME HERE>-scaffold.toml in .rimera/scaffolds

template<PRO>-scaffold.toml
```toml
# Define files via
[files]
"main.py" = "# The content inside main.py"
"./scripts/run.py" = "import basic \n print("Starting thing")"

# Available commands
[commands]
"start" = "rimera-lite ./scripts/run.py"
```
