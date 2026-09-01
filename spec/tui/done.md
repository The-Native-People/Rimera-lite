## Native build completion

The CLI renders this loader when `RIMERA_COLOR=always` or stderr is a terminal:

```text
Rimera Lite [♣]
  |-+ source  app.py
  |-+ target  aarch64-apple-darwin · debug · trace
[============] 100% [♣]
Debug:
  -+ cache  /project/.rimera
  -+ ir     /project/.rimera/ir/build.ir.py
[✔] Build complete  dist/app

Done in 0.61s +|+ Size: 470856 bytes.
```

The loader is a single in-place row: its label and stage checkpoint advance
through the build rather than leaving one loader row per stage. The active bar
is cyan, the completed bar is green, and each percentage is a real compiler-
stage checkpoint rather than an elapsed-time estimate.

With `--debug`, trace lines appear in magenta and the final summary includes
the `.rimera/ir/*.ir.py` debugging view.
