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

## Completion evidence

- `gate8_generator_cleanup.py` proves normal resume/send, injected throw with
  suppression and propagation, `GeneratorExit`, ignored-close failure,
  captured-exit mutation, user delegation, and abandoned-generator cleanup
  against CPython 3.12.11 under heap pressure.
- `unreachable_suspended_generators_close_once_before_collection` proves the GC
  lifecycle runs the ordinary Close resume operation exactly once before sweep.
- The public test is
  `gate8_slice6_suspension_throw_close_and_delegated_cleanup_match_cpython_312`.
