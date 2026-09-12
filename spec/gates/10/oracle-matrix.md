# Gate 10 Async Oracle Matrix

This matrix freezes ownership and expected Python 3.12 behavior before Rimera
implements asynchronous execution. CPython 3.12.11 is the language oracle.
Compio is an executor backend, never a Python-semantics oracle.

Gate 10 remains queued until Gate 9 closes. These records may be prepared while
Gate 9 is active because they add no production async behavior and make no async
compatibility claim.

## Ownership map

| Surface | Owner | Must not own |
| --- | --- | --- |
| Parsing and placement diagnostics | `rimera-compiler::syntax` | scheduling, runtime state |
| Scope/effect meaning and binding | HIR/sema | backend selection, polling |
| Suspension states, resume/injection edges, liveness | MIR/verifier | Python object layout, executor queues |
| Target lowering and direct resume dispatch | Cranelift/codegen | scheduler policy |
| Coroutine/async-generator objects, frames, exceptions, cleanup, GC | `rimera-runtime` | ready queues, timers, platform I/O |
| Opaque task ids, submission/completion transport, wake/cancel/timer facade | future `rimera-async-runtime` | `RValue` inspection, Python semantics |
| Ready queue, polling, wakers, timers, platform I/O | Compio backend | coroutine/frame semantics |
| `--async` and project-setting precedence | `rimera-cli`/project config | per-poll backend dispatch |
| Reachability and selected archive/platform libraries | compiler linker | Python async semantics |

The allowed dependency direction is `rimera-runtime -> rimera-async-runtime ->
rimera-abi`. The async-runtime crate may receive opaque handles and completion
records only. It may not depend on `rimera-runtime`, inspect `RValue`, inspect
frames/exceptions, or create a second Rimera scheduler.

## Syntax and classification

| Form / case | Expected CPython 3.12 outcome | Rimera owner | Slice |
| --- | --- | --- | --- |
| `async def f(): ...` | valid coroutine-function definition when body contains no `yield` | syntax/HIR/sema | 2 |
| call of coroutine function | returns lazy coroutine object; body has not executed | runtime call/object model | 3 |
| `await x` in `async def` | valid; delegates to coroutine fast path or `__await__` protocol | HIR/MIR/runtime | 2, 3, 6 |
| `await` at module scope | `SyntaxError: 'await' outside function` | syntax | 2 |
| `await` in sync function | `SyntaxError: 'await' outside async function` | syntax | 2 |
| `async for` in `async def` | valid | syntax/HIR/MIR/runtime | 2, 7 |
| `async for` outside async function | `SyntaxError` | syntax | 2 |
| async comprehension in async function | valid with comprehension scope/evaluation order | syntax/HIR/MIR/runtime | 2, 7 |
| async comprehension outside async function | `SyntaxError` | syntax | 2 |
| `async with` in `async def` | valid | syntax/HIR/MIR/runtime | 2, 9 |
| `async with` outside async function | `SyntaxError` | syntax | 2 |
| `yield` in `async def` | classifies function as async generator | syntax/HIR/sema/runtime | 2, 8 |
| `yield from` in async function | `SyntaxError: 'yield from' inside async function` | syntax | 2 |
| non-empty `return` in async generator | `SyntaxError: 'return' with value in async generator` | syntax/sema | 2, 8 |

The checked-in syntax snapshot is
`tests/fixtures/async/gate10_slice1/syntax_oracle.expected.json`.

## Coroutine and awaitable protocol

| Case | Expected CPython 3.12 outcome | Owner | Slice |
| --- | --- | --- | --- |
| coroutine creation | argument binding occurs; body remains lazy until first resume | runtime call/coroutine object | 3 |
| immediate coroutine return | first resume completes with return value | coroutine resume MIR/runtime | 3 |
| nested native coroutine await | child runs directly inside root task; result propagates to parent | MIR/runtime | 3 |
| coroutine exception | managed exception propagates through awaiting parent with traceback | runtime exception/frame model | 3 |
| completed coroutine awaited again | `RuntimeError: cannot reuse already awaited coroutine` | runtime coroutine state | 3 |
| `close()` before completion | injects close/`GeneratorExit` semantics and executes pending cleanup once | runtime completion/cleanup | 3, 6 |
| abandoned never-awaited coroutine | runtime warning equivalent to CPython lifecycle rule | runtime lifecycle | 3 |
| object with valid `__await__` | method called once for that await expression; returned iterator is driven | object protocol/runtime | 6 |
| missing `__await__` | managed `TypeError` | object protocol/runtime | 6 |
| `__await__` returns non-iterator | managed `TypeError` | object protocol/runtime | 6 |
| await iterator returns via `StopIteration.value` | value becomes await-expression result | runtime delegation | 6 |
| value sent into suspended await | resume value reaches delegated iterator/coroutine | MIR/runtime | 2, 6 |
| exception injected while suspended | exception enters normal managed exception/cleanup path | MIR/runtime | 2, 6 |
| cancellation observed at suspension | cancellation is injected as managed completion; cleanup runs once | runtime/executor boundary | 5, 6 |

`protocol_oracle.py` manually drives `__await__` iterators using `send`, `throw`,
and `close`; it deliberately does not use `asyncio`.

## Executor/task boundary

| Case | Expected Gate 10 outcome | Owner | Slice |
| --- | --- | --- | --- |
| submit one root coroutine | exactly one Rimera wrapper allocation and one backend task | async-runtime/Compio | 4, 10 |
| await nested local coroutine | no nested executor task; direct child-to-parent resume | runtime | 3, 4 |
| ready wake | backend wakes exactly the owning task; no Rimera mirror ready queue | Compio facade | 4, 5, 10 |
| stale task id | deterministic rejected handle, no alias to reused slot | async-runtime arena | 4 |
| completion retrieval | constant-time id lookup and exactly one terminal completion | async-runtime | 4 |
| dropped root / shutdown | task state releases exactly once and retained Python roots are released safely | async-runtime/runtime | 4, 11 |
| executor re-entry | defined deterministic boundary; no hidden nested scheduler | async-runtime/runtime | 4 |

## Timers, wakeups, cancellation, and buffers

| Case | Expected Gate 10 outcome | Owner | Slice |
| --- | --- | --- | --- |
| monotonic timer | backend wake at/after deadline; no periodic scan | async-runtime/Compio | 5, 10 |
| idle task | zero periodic polling/CPU work | Compio | 5 |
| duplicate wake | coalesced without losing a racing wake | async-runtime/Compio | 5 |
| cancellation requested before suspension | request recorded; observed at next defined suspension boundary | runtime/async-runtime | 5 |
| cancellation racing wake/completion | one terminal result; no duplicate completion | async-runtime/runtime | 5, 11 |
| borrowed/pinned I/O buffer | owner remains rooted/pinned until completion or acknowledged cancellation | runtime/async-runtime | 5 |
| expired operation callback | may not retain or use an expired borrow | async-runtime/backend | 5 |

## Async iteration and comprehensions

| Case | Expected CPython 3.12 outcome | Owner | Slice |
| --- | --- | --- | --- |
| `aiter(x)` / `x.__aiter__()` | asynchronous iterator object or `TypeError` | object protocol/runtime | 7 |
| `anext(x)` / `x.__anext__()` | awaitable; `StopAsyncIteration` ends iteration | object protocol/runtime | 7 |
| `anext(x, default)` | default returned on async exhaustion | runtime builtin | 7 |
| `async for` target/body | awaits each `__anext__`; no task per element | HIR/MIR/runtime | 7 |
| break/continue/return/exception | compiler-planned cleanup and exact pending completion | MIR/runtime | 7 |
| async list/set/dict comprehension | Python scope/evaluation/filter order | HIR/MIR/runtime | 7 |
| mixed sync/async comprehension clauses | Python 3.12 ordering and closure rules | HIR/MIR/runtime | 7 |

## Async generators

| Case | Expected CPython 3.12 outcome | Owner | Slice |
| --- | --- | --- | --- |
| async-generator call | lazy async-generator object | runtime | 8 |
| `__anext__` / `asend` | returns operation awaitable; yielded item becomes operation result | runtime | 8 |
| first non-`None` send | CPython-compatible error | runtime | 8 |
| `athrow` | injects managed exception at suspended point | runtime | 8 |
| `aclose` | injects `GeneratorExit`, executes cleanup exactly once | runtime | 8 |
| overlapping operation | CPython-compatible running/overlap error | runtime | 8 |
| await inside async generator | resumes inside same root task, not task-per-yield/await | runtime | 8 |
| exhaustion | `StopAsyncIteration` | runtime | 8 |
| abandoned generator | exactly-once finalization hook/cleanup | runtime lifecycle | 8, 11 |

## Asynchronous context managers

| Case | Expected CPython 3.12 outcome | Owner | Slice |
| --- | --- | --- | --- |
| lookup | special `__aenter__` / `__aexit__` lookup on type in Python order | object model/runtime | 9 |
| entry | await `__aenter__`; bind target only after successful completion | MIR/runtime | 9 |
| multiple managers | enter left-to-right, exit right-to-left | MIR cleanup planner/runtime | 9 |
| partial entry failure | exit only managers that completed entry | MIR cleanup planner/runtime | 9 |
| body exception | exact managed exception triple passed to `__aexit__` | runtime exception model | 9 |
| truthy exit result | suppress pending body exception | runtime truth/cleanup | 9 |
| exit suspends | pending completion and manager roots survive suspension | MIR/runtime/GC | 9 |
| exit fails/cancellation replaces completion | normal exception chaining/context and exactly-once cleanup | runtime | 9 |

## Backend selection and artifact behavior

| Case | Expected Gate 10 outcome | Owner | Slice |
| --- | --- | --- | --- |
| synchronous source | no `rimera-async-runtime`, Compio, Monoio, or Tokio symbols and no async size delta | linker/reachability | 10, 12 |
| async source + `auto` | statically select Compio on supported target | CLI/project/linker | 10 |
| `--async compio` | statically link Compio adapter | CLI/linker | 10 |
| `--async monoio` / `tokio` before adapter exists | stable no-artifact unavailable-backend diagnostic | CLI/project | 10 |
| CLI vs project config | CLI overrides project setting | CLI/project | 10 |
| hot poll path | no backend string lookup, trait-object dispatch, channel hop, lock, or Rimera allocation | codegen/async-runtime | 10, 11 |

## Cross-feature and final audit ownership

Slice 11 owns composition with imports, classes/descriptors, closures, exceptions,
synchronous generators, GC, low heaps, cancellation races, ready/timer storms,
deep await chains, fan-out, and benchmark regression proof. Slice 12 owns the
complete workspace/Clippy/fmt/docs/reproducibility/release/symbol audit and is
the only slice that may close Gate 10 or advance the active compatibility gate.
