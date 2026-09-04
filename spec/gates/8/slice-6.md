# Gate 8 Slice 6 — Suspension, Generator `throw`/`close`, and Delegated Cleanup

## Goal

Preserve active context managers across generator suspension and guarantee
cleanup for resumed, thrown, closed, delegated, and abandoned paths.

## Integrated work

- Persist entered-manager exit actions and their exact roots across every yield
  in the body, enter result handling, nested cleanup, and `yield from` delegate.
- On normal resume continue without re-entering; on `throw` deliver the injected
  exception through the body and invoke exits only when control leaves scope.
- On `close`, propagate `GeneratorExit` through all active context exits and
  `finally` regions in inner-to-outer order, including suppression/replacement
  and yielded-during-close failures.
- Compose delegating/delegated generators whose context scopes end at different
  layers without duplicate or skipped exits.
- Clear saved context actions and manager graphs after terminal completion while
  preserving reflectively retained generator/frame/traceback state.

## Completion proof

Public CPython differentials cover suspension states, sends, throws, closes,
delegation, nested contexts/finally, abandonment, traceback order, forced GC,
and post-completion collection.
