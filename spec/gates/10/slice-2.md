# Gate 10 Slice 2 — Async Syntax, HIR/MIR Suspension, Resume, and Injection

## Goal

Represent every Gate 10 async form in the owned compiler pipeline without
executing it through a hidden interpreter or synchronous-generator shortcut.

## Integrated work

- Parse and resolve `async def`, `await`, `async for`, async comprehensions,
  `async with`, and asynchronous-generator syntax with exact Python 3.12 scope
  and placement diagnostics.
- Add owned HIR nodes and explicit verified MIR suspension kinds for coroutine
  await, async iteration, async-generator yield, and async cleanup.
- Define resumptions carrying a value, managed exception, cancellation
  injection, or close request through the existing completion representation.
- Preserve live SSA values, shadow roots, source spans, exception state, and
  pending cleanup across every suspension edge.
- Reject unsupported or illegal combinations before object emission and remove
  partial artifacts through the normal build transaction.

## Performance invariants

Suspension layout is computed once from liveness. Resume dispatch is a direct
state switch with no AST walk, name lookup, heap allocation, or backend call
when the coroutine can continue synchronously.

## Completion proof

Verifier/unit tests cover every state and malformed edge, while public negative
fixtures match CPython diagnostics and native artifact inspection proves the
absence of bytecode, CPython, generated C, and interpreter symbols.

## Closure evidence

`mir::tests::coroutine_suspension_kinds_are_explicit_verified_and_live` proves
coroutine suspension has distinct verified MIR state/liveness. The public
`gate10_slice2_async_await_uses_explicit_native_suspension_and_no_artifact_negatives`
fixture proves `async def`/`await` lower through explicit native `Suspend::Await`,
not generator yield, and pins CPython-shaped illegal-placement diagnostics with
no artifact publication. The focused public proof is green.

## DONE BY CHATGPT
