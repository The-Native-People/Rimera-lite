# Rimera native ABI v1

The v1 ABI is defined by `rimera-abi` and implemented by `rimera-runtime`.

- `RValue` is 16 bytes: `u32 tag`, `u32 flags`, `u64 payload`.
- Tags are `None=0`, `Bool=1`, `SmallInt=2`, and `Handle=3`.
- Handles encode a 32-bit generation in the high payload half and a 32-bit
  heap-slot index in the low half.
- `RStatus` is an `i32`: `Ok=0`, `Exception=1`, `InvalidArgument=2`, and
  `AbiMismatch=3`.
- A successful fallible operation initializes its output. Other statuses do
  not promise an output value.
- `RRootFrame` is stack-owned by generated code. Its slot pointer and length
  describe a compact shadow array containing exactly the values live across a
  runtime safepoint. The layout remains unchanged as liveness becomes more
  precise.
- Contexts own heaps, roots, and failure state. Values cannot cross contexts.
- Python exceptions never use host unwinding. In unwind-capable debug/test
  builds ABI guards catch internal Rust panics before they can cross the ABI;
  release builds use `panic = "abort"`, so no unwind-catching machinery is
  linked and unwinding still cannot cross the ABI.
- `RUnaryOperator`, `RBinaryOperator`, and `RCompareOperator` are `repr(u8)`
  protocol identifiers shared by syntax adapters, HIR, MIR, code generation,
  and the runtime. Their currently assigned values are stable ABI facts: unary
  `+`, `-`, `~`, `not`; binary `+`, `-`, `*`, `//`, `%`, `/`, `**`, shifts,
  and bitwise operations; and comparisons
  `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`, `not in`. New protocols are additive
  values; stages must not recreate local operator-number tables.

## Managed heap controls

- `rimera_context_set_heap_limit(context, bytes) -> RStatus` sets an optional
  managed-memory budget. Zero means unlimited. Lowering the limit first runs a
  collection and fails without changing the limit if reachable data remains
  above it.
- The budget is an estimate of Rimera-managed objects, not process RSS. A
  failed allocation records `managed heap limit exceeded` and returns
  `RStatus::Exception`.
- `rimera_value_array_new` and `rimera_value_array_get` provide the internal
  traced storage used by closure environments and future containers. Generated
  code still treats both the array and its elements as opaque `RValue`s.

The collector is precise, non-moving, and context-owned. Its root providers are
generated shadow frames, native temporary roots, and context-owned roots. The
current context roots its built-in type table, module globals, builtins,
handled and raised exceptions, closure cells/tuples, function defaults,
managed code objects, and an emergency `MemoryError` that remains usable when
the heap cannot allocate.

## Execution-kernel objects and calls

- `rimera_type_of` maps every immediate or managed Python-visible value to a
  rooted native type object. Generated code receives the type as an opaque
  `RValue`; it never observes a heap pointer or a builtin-specific object
  layout. `object`, `type`, and the exception hierarchy initialize eagerly;
  ordinary builtin types materialize lazily, retain stable identity for their
  context, and are rooted through the kernel and builtins dictionary.
  `NotImplemented` is a permanently rooted managed singleton with exact
  `NotImplementedType` identity. Managed IEEE-754 floats use the dedicated
  lazy `float` type and are constructed by `rimera_float_new` from an exact
  bit pattern.
- `bool` is a subtype of `int`, which is a subtype of `object`. Every other
  currently supported ordinary builtin type derives from `object`; type objects
  themselves have runtime type `type`. Internal `RValue` arrays are not Python
  values and deliberately map to `object` only within the runtime.

- `RParameterSpec`, `RCallArguments`, and `RKeywordArgument` describe the
  generic native call boundary. `RNameSpec` and `RCodeMetadataSpec` transport
  compiler-owned UTF-8 local/cell/free names plus filename and first source line
  when a managed code object is created. Every compiled Python function has the
  stable `RNativeFunction(context, function, bound, count, output) -> RStatus`
  signature.
- `rimera_function_new` creates one traceable function object pointing at one
  traceable code object. The code object owns the opaque native code identity,
  complete authoritative parameter-kind specification, filename, first line,
  local names, cell names, and free names; Python never observes the native
  address or Cranelift representation. The function object owns mutable
  Python-level `__name__`, `__qualname__`, positional-default tuple,
  keyword-only-default dictionary, optional annotations dictionary, and an
  immutable optional tuple of managed closure cells plus mutable Python-visible
  `__type_params__` tuple metadata for Gate 7 generic declarations. Ordinary
  attribute operations expose those fields plus stable `__closure__` and
  `__code__` identities. Code attributes (`co_name`, `co_qualname`, `co_filename`,
  `co_firstlineno`, argument counts, `co_nlocals`, `co_varnames`, `co_cellvars`,
  `co_freevars`, and the supported synchronous `co_flags`) are read-only.
  Closure tuple identity is read-only while each managed cell exposes mutable
  and deletable `cell_contents`. `__code__` assignment accepts only a managed
  code object with the same free-variable count, matching CPython's closure
  compatibility rule; compatible replacement changes the function's executed
  code and binder signature without creating a second signature model.
  Names/qualified names require strings and cannot be deleted; defaults accept
  tuple/`None`, keyword defaults and annotations accept dict/`None`, and deleting
  the latter three resets their Python-visible empty/`None` state. Reading
  missing annotations lazily materializes an empty dictionary. Generated
  annotation lowering still installs its evaluated dictionary through
  `rimera_attr_set`.
  `rimera_call` is the authoritative binder for positional-only,
  positional-or-keyword, keyword-only, `*args`, and `**kwargs` parameters. It
  resolves the function's current managed code object on every activation and
  reads the current defaults/keyword-defaults objects, so legal metadata and
  code mutation affect later calls without introducing a second binding path.
- Gate 4 expanded call sites preserve source-order parts with
  `RCallArgumentKind` (`Positional`, `Starred`, `Keyword`, and
  `KeywordUnpack`). `rimera_call_arguments_new` allocates an internal traced
  accumulator rooted to its callable; `rimera_call_argument_add` appends or
  expands one already-evaluated part through the generic iterator/mapping
  protocols; and `rimera_call_prepared` sends the accumulated positional and
  keyword values through the same authoritative `rimera_call` binder rather
  than implementing a second binding algorithm. The accumulator traces its
  callable and every accumulated value, charges retained vector/string
  capacity through managed-size accounting, and each mutating ABI return
  refreshes that accounting before heap-limit enforcement. Duplicate expanded
  keyword names and non-string mapping keys fail before callee entry at the
  CPython 3.12 callback point.
- `rimera_generator_function_new` creates the same traceable function/code pair
  and marks the managed code object as a generator with its persistent-slot
  count. Calling it through
  `rimera_call` performs ordinary argument binding but only allocates a
  suspended generator; it does not start execution. Its one permanent resume
  entry is `RNativeGeneratorResume(context, generator, operation, input,
  output, outcome) -> RStatus`. `RGeneratorOperation` selects `Next`, `Send`,
  `Throw`, or `Close`; an `Ok` result writes `RGeneratorOutcome::Yielded` or
  `RGeneratorOutcome::Returned`. Source generators and Gate 4 generator
  expressions use this same object, binder, resume entry, and Cranelift state
  dispatcher; there is no generator-specific interpreter or second ABI.
- A source `Yield` MIR terminator records the yielded value, the SSA resume
  input when the expression consumes `send`, its normal continuation, its
  injected-exception successor, and an optional active delegation value.
  Codegen saves every liveness-selected persistent value before publishing the
  next state and restores those slots before entering the compiler-selected
  continuation. The generator owns and traces persistent slots, the active
  delegate, saved handled-exception stack, a pending managed exception that may
  cross cleanup suspension, and its terminal return value; terminal completion
  or failure clears obsolete slots/delegates/exception state exactly once.
- Python-visible `__iter__`, `__next__`, `send`, `throw`, and `close` are bound
  managed methods reached through ordinary attribute/call dispatch. Generator
  `return` is surfaced by the caller as managed `StopIteration.value`; escaped
  `StopIteration` is converted to `RuntimeError` under PEP 479. `throw` and
  `close` inject the managed exception at the exact compiler suspension point,
  so local handlers/finally blocks remain ordinary verified MIR control flow.
- `yield from` uses the additive `RGeneratorDelegateOutcome` contract
  (`Yielded`, `Completed`, `Propagate`) and three opaque helpers:
  `rimera_generator_delegate_start` performs the first generic iterator step,
  `rimera_generator_delegate_set` publishes/clears the traced active delegate,
  and `rimera_generator_delegate_resume` forwards the current next/send/throw/
  close operation. Delegated `StopIteration.value` becomes the `yield from`
  expression result; a non-terminal delegate exception is returned as
  `Propagate` while the managed exception remains installed so generated code
  enters the delegator's compiled exception successor. Native generators,
  generator expressions, builtin iterators, and user iterator objects all use
  this path.
- Generated generator-resume code keeps the heap representation opaque through
  `rimera_generator_function_get`, `rimera_generator_state_get`,
  `rimera_generator_state_set`, `rimera_generator_slot_get`,
  `rimera_generator_slot_set`, and the delegation helpers above. These helpers
  expose neither heap pointers nor generator layout and are not a runtime MIR
  evaluator.
- `rimera_cell_*`, `rimera_global_*`, and
  `rimera_function_closure_get` implement mutable lexical cells, module
  globals, and builtins fallback without exposing heap pointers to generated
  code. Global fallback lazily publishes known builtin types and native builtin
  functions in the context-owned builtins dictionary.
- Gate 7 namespace reflection extends that same compiled-cell contract with two
  additive opaque helpers: `rimera_reflection_scope_configure(context,
  namespace_or_null, comprehension)` declares whether the active native
  activation observes a live prepared namespace, a refreshed frame-style locals
  snapshot, or an inlined-comprehension overlay; and
  `rimera_reflection_local_register(context, name, name_len, cell)` registers the
  authoritative compiled `Cell` for one Python-visible binding. Generated code
  emits these operations from verified MIR; the runtime does not allocate a
  second local-variable store, interpret MIR, or expose frame addresses.
  Module `globals()`/`locals()` use the context-owned globals dictionary. Class
  bodies pass their existing prepared namespace and therefore expose live
  write-through class locals. Ordinary functions and source generators lazily
  allocate one managed locals dictionary per activation/frame and refresh only
  compiler-owned binding keys from registered cells; the dictionary identity is
  stable during the activation, independently retained snapshots survive return,
  and user-added non-binding keys survive later refreshes. Python 3.12 inlined
  list/set/dict comprehensions temporarily overlay their registered iteration
  cells onto the nearest non-comprehension owner view and restore/remove those
  names on exit. Generator expressions instead use their own suspended
  frame-style snapshot containing `.0`, iteration locals, and referenced frees.
  Suspended generators persist the reflection cell registry and snapshot in the
  managed generator object across resume boundaries and clear obsolete internal
  reflection roots at terminal completion. All configured namespaces, cells,
  snapshots, and overlay state participate in the context's precise root graph.
  The lazily published managed builtins `globals`, `locals`, `vars`, and `dir`
  are invoked through ordinary `rimera_call`: zero-argument `vars()` delegates
  to the current locals rule; one-argument `vars()` uses ordinary `__dict__`
  lookup; and `dir` uses current namespace/default attribute sources or a normal
  bound `__dir__` callback, consuming its iterable generically and sorting the
  resulting managed list through ordinary list semantics. Allocation failures
  reached through these generic calls preserve the common managed `MemoryError`
  path instead of being reclassified as argument-binding `TypeError`s.
- Gate 7 Slice 1 adds the deliberately narrow opaque import helper
  `rimera_import_name(context, name, name_len, output)`. Verified `ImportName`
  MIR lowers directly to this ABI and may resolve only explicitly registered
  managed module shells; the current registry is `inspect` and `weakref`.
  Each successful first import allocates one traced `ModuleObject` over one live
  managed namespace, publishes `__name__`, `__package__`, `__loader__`, and
  `__spec__`, and only then inserts the completed module into the context-owned
  cache. The cache is a precise context root, so repeated imports preserve module
  identity and namespace mutation survives collection. A construction failure
  publishes no cache entry; managed-heap exhaustion follows the ordinary
  `MemoryError` ABI path, and an unregistered name raises managed
  `ModuleNotFoundError`. This helper does not execute Python module source and
  does not implement dotted/package imports, `from ... import ...`, public
  `sys.modules`, a user-visible `__import__`, or either module shell's stdlib API.
- Gate 7 Slice 5 extends the existing managed exception/traceback ABI without
  exposing native frame pointers. Function/generator failures use
  `rimera_traceback_append(context, filename, filename_len, function,
  function_len, line, column)` to append one managed `TracebackObject` linked to
  one stable managed `FrameObject`; the frame owns managed references to its
  authoritative `CodeObject`, globals, refreshed locals snapshot, optional
  caller frame, and source line. Module failures use the additive
  `rimera_traceback_append_module` signature with the same arguments and status
  contract but a module-only capture path, keeping function/generator
  introspection machinery out of tiny module-only release artifacts. Compiler
  exception edges append a frame only when crossing a new Python frame; bare
  reraising and cleanup propagation preserve the existing chain. Traceback/frame
  allocation failure publishes no partial link and maps heap exhaustion to the
  ordinary managed `MemoryError` path.
- Gate 7 Slice 6 keeps the Gate 6 native resume ABI opaque while publishing
  Python-visible generator metadata from managed ownership. Each generator owns
  stable name/qualified-name metadata and a managed frame whose code, globals,
  locals, and suspension line are Python-visible; `gi_running`, `gi_suspended`,
  and the current `yield from` delegate are derived from the authoritative
  generator lifecycle. Verified generator-yield code calls
  `rimera_generator_frame_line_set(context, generator, line)` solely to update
  Python-visible suspension metadata; it does not expose or mutate the native
  resume address/state-machine layout. Terminal completion/close/failure clears
  the generator-to-frame edge while independently retained frames remain normal
  traced managed objects. Generator and frame payloads are boxed inside the
  managed-object union and their payload bytes are charged explicitly, so this
  reflection surface does not inflate the common per-object managed-size base or
  weaken the established 8 KiB startup contract.
- Gate 7 Slice 7 adds two opaque construction helpers for Python 3.12 generic
  metadata. `rimera_type_parameter_new(context, kind, name, name_len, output)`
  receives the stable `RTypeParameterKind` (`TypeVar`, `TypeVarTuple`, or
  `ParamSpec`) discriminator and allocates one traced managed parameter object.
  `rimera_type_alias_new(context, name, name_len, type_params, value, output)`
  allocates one traced managed type-alias object over an already-built parameter
  tuple and alias value. Verified `TypeParameterNew`/`TypeAliasNew` MIR lowers
  directly to these calls; generated code never observes their heap layout.
  Functions/classes/type aliases publish the same managed parameter identities
  through ordinary `__type_params__` attributes. Lazy PEP 695 bound/constraint
  evaluation is not faked by this ABI and remains a source-level capability
  boundary.
- Gate 7 Slice 8 adds no buffer pointer ABI. Python-level PEP 688 providers are
  reached through the existing generic special-method/call path: memoryview
  construction descriptor-binds `__buffer__`, forwards the CPython constructor
  flag mask, validates a managed memoryview result, and records one internal
  traced `BufferLeaseObject` containing the provider and exact returned view.
  Derived memoryviews trace and share that lease. The final live view invokes
  `__release_buffer__` exactly once and releases the exact returned view;
  callback failures are unraisable and cannot replace caller exception state.
  Failed post-acquisition allocation and cyclic exporter graphs use the same
  once-only release contract. GC discovers provider leases during lifecycle
  processing but invokes Python only after the raw heap phase, then recollects
  finalized cycles. Native bytes/bytearray/memoryview exporters continue using
  the Gate 3 representation and resize/export accounting.
- The rooted `type` object is callable through `rimera_call`; `isinstance` and
  `issubclass` are managed native builtin functions invoked through the same
  path, never compiler intrinsics. `isinstance` and `issubclass` use MRO
  membership and accept recursive tuple class-info with CPython-compatible
  validation. The three-argument `type` form remains reserved for native class
  construction.
- `rimera_class_new(context, name, name_len, bases, base_count, namespace,
  output)` creates a traceable user type without exposing type-object pointers.
  Empty bases imply `object`; ordered Rimera user-class bases are accepted and
  linearized with C3 into a permanent traced MRO. Duplicate bases and
  inconsistent hierarchies return structured `TypeError` failures without
  publishing a partial class. User subclasses of `list`, `tuple`, `dict`,
  `set`, `float`, `complex`, `bytes`, `bytearray`, and `frozenset` retain a
  traced opaque native payload and their user-class identity. Their storage is
  initialized through the same builtin constructor semantics, including the
  supported multi-argument complex/bytes/bytearray forms, mapping/iterable dict
  inputs, mutable-base `__init__`/`super().__init__` behavior, and generic-key
  collection storage. User classes have runtime type `type`; ordinary
  object-layout classes accept no constructor arguments.
  The three-argument `type(name, bases, namespace)` call uses this identical
  constructor after validating `str`, tuple, and dictionary inputs.
- `rimera_namespace_new`, `rimera_namespace_set`, `rimera_namespace_get`,
  `rimera_class_name_get`, `rimera_class_free_get`, and `rimera_namespace_delete` build and mutate the ordered,
  traceable namespace used by compiled class bodies. Gate 4 annotations add
  `rimera_annotations_ensure(context, namespace_or_null)`: a null namespace
  selects module globals, while a non-null value selects the prepared class
  namespace. It preserves an existing `__annotations__` value and otherwise
  creates the native dictionary through normal namespace/mapping operations,
  so custom `__prepare__` mappings keep their ordinary `__getitem__` and
  `__setitem__` semantics. `rimera_attr_get`,
  `rimera_attr_set`, and `rimera_attr_delete` operate on opaque values and
  UTF-8 attribute names. Attribute lookup applies a type-MRO data descriptor,
  then an instance dictionary, then an MRO non-data descriptor or ordinary
  value. Compiled functions are non-data descriptors and become traceable
  bound-method values whose generic call prepends the receiver. `property`,
  `staticmethod`, `classmethod`, and custom `__get__`/`__set__`/`__delete__`
  values therefore share the generic call path; codegen has no descriptor
  special case. Class namespace writes and deletes advance a type version tag
  reserved for attribute-cache invalidation.
- Class suites compile as hidden native `ClassBody` functions with their
  prepared namespace as an internal positional parameter. They use the normal
  five-word native function ABI and generic call path, so class-body failures,
  closures, GC roots, and traceback frames follow ordinary compiled-function
  contracts. `rimera_class_name_get` resolves an ordinary dynamic class-body
  name from the prepared namespace before module globals and lazy builtins.
  `rimera_class_free_get(context, namespace, cell, name, len, output)` is the
  class-body free-variable companion: it checks the prepared namespace first
  and falls back to the explicit enclosing closure cell on a miss. Explicit
  class `global` bindings bypass both helpers and explicit class `nonlocal`
  bindings read/write/clear the enclosing cell directly. A custom `__prepare__` mapping is accessed
  through ordinary `__getitem__`, `__setitem__`, and `__delitem__` calls while
  the body executes; a missing `KeyError` continues class-scope fallback.
  As in CPython, default `type.__new__` still requires a dictionary, while a
  custom metaclass may consume or convert its prepared mapping in `__new__`.
- `__bases__` reassignment computes C3 MRO plans for the target and every
  descendant before committing any mutation. The committed set receives new
  version tags together, so a failed plan leaves all bases and MROs unchanged.
- A type carries an inherited slot layout plus dictionary and weak-reference
  capability flags. `__slots__` accepts the supported string, tuple, or list
  declaration shapes during native class construction. Slot declarations
  install traced member descriptors in the class namespace; instances retain
  their slot values in fixed traced storage and allocate an ordered dictionary
  only when their effective layout permits one. Weak-reference storage is a
  layout reservation only in ABI v1; weak-reference behavior remains deferred.
- After a class type is allocated but before it is published in a module global,
  the runtime invokes supported descriptor `__set_name__` hooks and direct-base
  `__init_subclass__` hooks through `rimera_call`. Source class decorators are
  then applied bottom-up by normal MIR call operations.
- Class statements with explicit metaclasses resolve `__prepare__` through
  descriptor lookup, forward class keywords to `__prepare__`, `__new__`, and
  `__init__` in source order, execute custom hooks through `rimera_call`, and
  retain all inputs as temporary roots. Implicit selection
  chooses the most-derived metaclass compatible with every base.
  `__mro_entries__` replacements are normalized before C3 calculation and the
  original tuple is published as `__orig_bases__`.
- Existing unary/binary/comparison/truth/length/item/containment/call/iterator
  ABI calls retain exact builtin fast paths and enter the authoritative managed
  special-method dispatcher for user instances. `RBinaryOperator` includes
  matrix multiplication and is shared by normal, reflected, and in-place
  dispatch. Strict right-hand subclasses receive reflected priority, each
  candidate runs at most once, and fallback continues only for the rooted
  `NotImplemented` singleton. `rimera_inplace` applies `__iop__` before the
  normal/reflected route; list `+=` and `*=` retain list identity.
  `RCompareOperator::Is` and `IsNot` are native identity checks and never
  perform dunder lookup.
- `rimera_item_delete` is the ABI operation for `del receiver[index]` and
  dispatches `__delitem__` before native collection storage. Attribute and
  item augmented assignment evaluate their receiver/index exactly once and
  lower through `rimera_inplace` plus the ordinary store operation.
- Runtime operations may install a concrete managed exception in the context
  before returning `RStatus::Exception`. ABI wrappers preserve that raised
  value rather than reclassifying its error-message text. Gate 3 builtin paths
  use this contract for `ValueError`, `KeyError`, `BufferError`,
  `OverflowError`, `IndexError`, `TypeError`, and `ZeroDivisionError`, while
  preserving exceptions raised by user protocol callbacks.
- `hash`, `repr`, `format`, and `reversed` are lazy managed builtins invoked
  through `rimera_call`. Hash results must be integers and normalize `-1` to
  `-2`; a class defining `__eq__` without `__hash__` receives the unhashable
  sentinel. `repr`/`str`/`format` invoke user protocol methods through the
  normal call path and require string returns. Gate 4 formatted strings use the
  additive `RFormatConversion` (`None`, `Str`, `Repr`, `Ascii`) identifier and
  `rimera_format_value(context, conversion, value, spec, output)`. The runtime
  applies the selected ordinary conversion and then the same generic formatting
  protocol used by `format`; generated code retains the value, nested spec,
  and accumulated prefix through its precise safepoint roots. Native representation covers
  CPython-shaped float/complex display, Unicode 15.0 printability escapes, and
  recursive dictionary-view cycle markers. Native formatting covers the
  supported int/float/complex/string mini-language including alignment, sign,
  alternate form, zero-padding, grouping, precision, `z`, and special float
  values; locale-specific `n` transformations remain outside this core ABI
  contract. The lazy `round` and `divmod` builtins also stay entirely on
  `rimera_call`: float `round` uses exact binary64 rational decomposition plus
  arbitrary-precision decimal ties-to-even rounding, while native float/mixed
  `divmod` computes its quotient/remainder pair as one coordinated operation;
  custom `__round__`, `__divmod__`, and `__rdivmod__` continue through normal
  managed protocol dispatch. Iteration validates `__iter__` results, supports
  legacy `__getitem__` sequence iteration, and reverse iteration uses
  `__reversed__` or the `__len__`/`__getitem__` protocol fallback. Native reverse
  iterators additionally cover bytes/bytearray, ordered dictionaries and their
  live views with mutation-version checks, plus arbitrary-precision arithmetic
  range reversal. Managed range values expose native `start`/`stop`/`step` and
  bound `count`/`index` methods; only `len(range)` converts the logical length to
  the platform ssize-sized public length boundary, while truth/index/slice and
  iterator arithmetic remain arbitrary precision.
- The rooted builtin-function representation also exposes `getattr`,
  `setattr`, `delattr`, `hasattr`, and `callable` through `rimera_call`.
  These use the same native attribute lookup and special-method dispatch as
  source attribute expressions; no compiler intrinsic or alternate object
  representation is involved.
- Builtin type values continue through the ordinary `rimera_call` ABI. The
  supported `bool`, `int`, `str`, `float`, `complex`, `bytes`, `bytearray`,
  `list`, `tuple`, `dict`, `set`, `frozenset`, and `memoryview` constructors use
  native runtime conversion/collection protocols rather than compiler
  intrinsics. Core `int`/`float`/`complex` text parsing, raw memoryview byte
  copying, and supported builtin-storage subclass construction are proven
  against CPython 3.12.11. Managed slice values expose immutable
  `start`/`stop`/`step` and a bound `indices` method without a new ABI opcode;
  normalization remains inside the existing runtime item/slice contract.
  Native-exporter memoryviews accept the supported one-character native formats
  with optional `@`, including `n`, `N`, and `P`, and use target-native widths
  for C `long`, ssize-sized, size-sized, and pointer-sized values. Equality is
  logical element/shape equality, while readonly byte-format hashing validates
  the ultimate exporter. Gate 7 Slice 8 extends this same managed memoryview
  model to user-defined PEP 688 `__buffer__`/`__release_buffer__` providers via
  ordinary generic dispatch and an internal traced lease; no partial exporter
  pointer or duplicate buffer ABI is exposed to generated code.
- `rimera_complex_new`, `rimera_bytes_new`, `rimera_bytearray_new`, and
  `rimera_slice_new` are additive ABI-v1 constructors for source literals and
  normalized slice bounds. Their inputs and outputs are opaque values; null
  slice-bound pointers represent an omitted bound. `rimera_dictionary_view_new`
  creates a rooted live keys, values, or items view over a native dictionary.
  Bytes, bytearrays, complex values, slices, frozen sets, dictionary views,
  and memoryviews are ordinary traceable managed objects whose exact runtime
  identity is materialized lazily through `rimera_type_of`.
- Native `dict.keys`, `dict.values`, and `dict.items` bind as normal managed
  callables and produce live traced view objects. Native-exporter memoryviews
  expose `release`, `tobytes`, `tolist`, `hex`, `format`, `itemsize`, `ndim`,
  `cast`, `shape`, `strides`, `readonly`, and `nbytes` through ordinary attribute
  lookup. These operations preserve opaque handles; no generated code obtains
  a buffer pointer. The GC lifecycle phase releases an unreachable native
  bytearray export before sweep; explicit `release()` remains idempotent.
- `rimera_super_new(context, start_type, receiver, output)` constructs a
  traceable explicit-super value after validating that the receiver is an
  instance or subtype of `start_type`. Generic attribute lookup begins after
  `start_type` in the receiver type's C3 MRO and binds compiled methods to the
  original receiver. The builtin `super` type invokes this behavior through
  the ordinary call path. Class lowering creates one shared traced
  `__class__` cell for methods containing zero-argument `super()`, fills it
  only after successful native type construction, and records each generic
  native call's bound receiver in the context. The no-argument form reads that
  cell and receiver rather than using a compiler-only super operation.
- `rimera_tuple_new` creates immutable managed tuples. Internal
  `rimera_value_array_*` storage remains non-Python-visible and is used for
  fixed ABI result bundles.
- `rimera_unpack_ex(context, value, before_count, after_count, starred, output)`
  is the additive extended-unpack ABI used by Gate 4 recursive assignment
  targets. The source expression is evaluated once and one current iterator is
  advanced monotonically. For extended unpacking, the runtime intentionally
  applies `iter()` to that current iterator again before draining the starred
  remainder, matching CPython's observable `UNPACK_EX` callback order without
  rewinding or re-evaluating the source. Fixed prefix/suffix values are returned
  in source-target order and the starred slot contains a newly allocated native
  list. The original `rimera_unpack` entry point is retained for ABI-v1
  compatibility and maps to the trailing-star/exact subset.
- `rimera_pattern_sequence(context, value, before_count, after_count, starred,
  output, matched)` is the Gate 4 structural-sequence extractor. It performs the
  Python match-sequence eligibility/length preflight, excludes string and
  byte-oriented values, then reuses the generic iterator/unpack path to publish
  a managed value-array only after structural success. Length mismatch writes
  `matched = 0`; protocol exceptions remain managed exceptions.
- `rimera_pattern_mapping_check(context, mapping, minimum_count, output)` runs
  the mapping eligibility/length preflight before pattern key expressions are
  evaluated, preserving CPython ordering. `rimera_pattern_mapping(context,
  mapping, keys, key_count, include_rest, output, matched)` then hashes/checks
  dynamic keys left-to-right, invokes ordinary mapping `get`, treats a missing
  key as `matched = 0`, rejects runtime-equal duplicate keys with `ValueError`,
  and optionally appends a freshly allocated `**rest` dictionary without
  mutating the source mapping. Both operations retain opaque managed handles
  and root all live mapping/key/extracted values across callbacks.
- `rimera_pattern_class(context, subject, class, positional_count, keyword_blob,
  keyword_blob_len, output, matched)` is the Gate 4 class-pattern extractor.
  `class` must be a runtime type and matching uses the ordinary native
  `isinstance`/subtype contract. Positional fields resolve `__match_args__`
  through normal class/metaclass attribute lookup, require a tuple of strings,
  enforce positional count and duplicate-attribute rules, and preserve the
  builtin single-self match convention for Python's match-self builtin types.
  Keyword fields use ordinary descriptor-aware instance attribute lookup;
  missing attributes write `matched = 0` while descriptor and validation
  exceptions propagate unchanged. Keyword identifiers are encoded only inside
  the compiler/runtime ABI as one NUL-separated UTF-8 metadata blob; an empty
  keyword set is represented by a null pointer and zero length. Extracted
  values are returned as the same opaque managed value-array bundle used by
  the other structural-pattern helpers and remain rooted across callbacks.
- `rimera_list_new` creates a traceable managed list from a contiguous input
  array. The constructor temporarily roots every input until the list has been
  allocated and published; generated code still observes only opaque handles.
  Gate 4 comprehensions add `rimera_list_append(context, list, value)`, a
  fallible in-place sink over the same managed list storage. Every successful
  mutating ABI return refreshes retained-capacity accounting before the common
  managed-heap limit check, so comprehension growth cannot bypass the budget.
- `rimera_dict_new` and `rimera_set_new` create insertion-ordered traced
  collections from contiguous ABI arrays. Gate 4 comprehensions add
  `rimera_set_insert(context, set, value)` and
  `rimera_dictionary_insert(context, dictionary, key, value)`. Both route
  through the same generic hash/equality protocol and ordered hash table as
  ordinary set/dict mutation, preserve callback exceptions, and publish no
  partial entry on failed hashing/equality. Dictionary comprehension code
  evaluates its key before its mapped value, then calls the insert sink.
  `rimera_dictionary_merge` is the
  additive Gate 4 mapping-display operation for `{**mapping}`: it accepts
  mappings only (not iterable-pair fallback), obtains keys/values through the
  generic mapping protocol, and applies each entry immediately so source
  expression, hash/equality callback, replacement, and failure order remain
  left-to-right. Key validation uses the native hash protocol, including user
  `__hash__` values and the equality-without-hash unhashable sentinel. Bool/int/float/complex equality and hashing use one
  exact numeric domain so equal mixed numeric keys always share a hash,
  including arbitrary-size integers and finite binary64 values. Duplicate
  dictionary keys replace their value without moving or replacing the original
  stored key; duplicate set values retain their first position. Hash-table
  lookup/update/remove revalidate stable entry identifiers after user equality
  callbacks mutate the collection. Native dict `|` and dict-source `|=` reuse
  the hashes already stored in the source table while preserving order; the
  ordinary in-place update path also accepts mappings and iterable pairs.
- Gate 4 list/set/dictionary comprehensions compile as ordinary hidden native
  functions named `<listcomp>`, `<setcomp>`, or `<dictcomp>` using the existing
  `RNativeFunction` call ABI. The containing scope evaluates the outermost
  iterable and creates its iterator before invoking the hidden function as
  positional parameter `.0`; targets, later iterables, filters, and sinks run
  inside that activation. Closure cells/free variables use the normal function
  closure ABI, and generated MIR publishes precise roots at every iterator,
  truth, target-unpack, callback, and collection-sink safepoint. These hidden
  list/set/dictionary activations are traceback-transparent on failure so
  Python 3.12 user tracebacks reflect the inlined-comprehension frame model
  rather than exposing Rimera's implementation helper. Generator expressions
  reuse the same implicit-scope/clause lowering but create a hidden `<genexpr>`
  `Generator` function instead: its sink is an explicit `Yield`, its generator
  frame remains Python-visible, and liveness-derived values cross suspension
  through the generator slots documented above.
- `rimera_unpack` consumes a supported native iterable and returns a traced
  internal value array for flat assignment unpacking. It implements exact and
  final-starred target arity without exposing iterator internals to codegen.
- `rimera_length` and `rimera_item_get` provide generic managed-length and
  integer-indexed tuple/list access. They return `RStatus::Exception` with a
  structured `TypeError` or `IndexError` on invalid operands or bounds.
- `rimera_print` remains the generic Python default-print operation over opaque
  values and therefore owns normal object stringification. For the exact source
  shape `print("literal")`, semantic analysis may prove that the global `print`
  binding is never mutated and lower to `rimera_print_literal(context, bytes,
  len)`. That additive ABI operation writes the already-known UTF-8 string plus
  the default newline without constructing a managed string or entering generic
  display dispatch. Rebound/aliased/local/free `print`, keyword forms, and all
  nonliteral arguments remain on the ordinary lookup/call or generic-print
  paths; the optimization does not change Python name-binding semantics.

## ABI reachability and reserved runtime helpers

Final Gate 3 release codegen declares only runtime imports required by the
verified MIR. This lets the native linker dead-strip unrelated builtin families
without changing the stable runtime ABI.

- `rimera_type_of`, `rimera_bytearray_new`, `rimera_dictionary_view_new`, and
  `rimera_super_new` are runtime-owned helpers used by managed implementation
  paths and/or direct runtime proofs rather than ordinary generated-code imports.
- `rimera_memoryview_release` is the low-level native-export lifecycle helper;
  Python-visible `memoryview.release()` is reached through managed attribute and
  call dispatch, while GC also releases unreachable exports during lifecycle
  processing.
- `rimera_exception_new` is structured-exception runtime scaffolding; generated
  source raises through the higher-level exception operations documented below.
- `rimera_generator_function_new`, `rimera_generator_new`,
  `rimera_generator_resume`, the state/slot accessors, and the three delegation
  helpers form the single native synchronous-generator ABI. Gate 4 generator
  expressions and Gate 6 source generators both exercise it through verified
  suspension MIR and compiled Cranelift continuation blocks. The implemented
  synchronous surface includes source `yield`, yield-expression resume values,
  `send`, `throw`, `close`, PEP 479, suspension through handlers/finally, and
  generic `yield from` delegation. Async generators and async iteration remain
  later async-gate work.

## Structured exceptions

- Exception instances are managed Python-visible values containing their type,
  live `args`, an optional traced instance dictionary, traceback, explicit
  cause, implicit context, and suppression flag. Ordinary attribute dispatch
  exposes `args`, `__dict__`, `__traceback__`, `__cause__`, `__context__`,
  `__suppress_context__`, user-defined exception attributes/methods, and
  `with_traceback`; metadata mutations validate Python's value constraints and
  are published only after any required allocation succeeds.
- Calling a user exception subclass uses the ordinary type/class invocation
  path and descriptor-bound `__init__`; raising an exception class normalizes
  it through that same generic call path before installing it as the active
  managed exception. Raising an existing exception preserves object identity.
  Class/cause normalization is owned specifically by the source-level
  `rimera_raise` ABI boundary; the lower-level `raise_value` operation accepts
  already-normalized managed exception instances. This separation preserves
  Python semantics while allowing programs with no source `raise` to dead-strip
  the generic call/constructor graph.
- `rimera_raise`, `rimera_reraise`, `rimera_handler_enter`,
  `rimera_handler_leave`, and `rimera_exception_propagate` maintain structured
  context-owned state. Generated code uses explicit normal and exception CFG
  edges; host unwinding is not exception control flow.
- `rimera_traceback_append` attaches managed filename/function/line frames at
  explicit source `raise` sites before a local handler receives the exception,
  and again when an exception escapes a compiled function. Traceback objects
  expose read-only `tb_lineno` and validated mutable `tb_next` through ordinary
  attribute dispatch, so `__traceback__`/`with_traceback` reuse the same traced
  object chain.
- `rimera_exception_split` recursively partitions ordinary exceptions and
  exception groups while preserving subgroup shape. The split result is an
  internal two-element value array containing matched and remainder values;
  `None` denotes an empty partition.
- `rimera_exception_set_active`, `rimera_exception_clear_active`, and
  `rimera_exception_combine` support deferred `except*` handler execution and
  merging. All matching handlers run before accumulated handler failures and
  the unmatched remainder are propagated.
