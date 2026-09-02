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
handled and raised exceptions, closure cells, function defaults, and an
emergency `MemoryError` that remains usable when the heap cannot allocate.

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
  generic native call boundary. Every compiled Python function has the stable
  `RNativeFunction(context, function, bound, count, output) -> RStatus`
  signature.
- `rimera_function_new` creates traceable function objects containing native
  code, the complete parameter specification, defaults, closure cells, and an
  optional traced annotations dictionary. Function `__annotations__` is
  exposed through the ordinary attribute path: reading lazily materializes an
  empty dictionary, assignment accepts a dictionary or `None` reset, and
  deletion resets it to the lazy-empty state. Generated annotation lowering
  installs an evaluated dictionary through `rimera_attr_set`; there is no
  compiler-visible function metadata pointer.
  `rimera_call` is the authoritative binder for positional-only,
  positional-or-keyword, keyword-only, `*args`, and `**kwargs` parameters.
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
- `rimera_generator_function_new` creates a traceable callable with the same
  binding metadata plus a persistent-slot count. Calling it through
  `rimera_call` performs ordinary argument binding but only allocates a
  suspended generator; it does not start execution. Its resume entry uses
  `RNativeGeneratorResume(context, generator, operation, input, output,
  outcome) -> RStatus`. `RGeneratorOperation` selects next, send, throw, or
  close, while an `Ok` result writes `RGeneratorOutcome::Yielded` or
  `RGeneratorOutcome::Returned`. The generator's persistent slots, delegate,
  saved handled-exception state, and terminal result are traced by its owning
  context. Codegen must preserve every value live across suspension in those
  explicit slots and clear obsolete slots on terminal completion. Gate 4
  generator expressions are the first source feature using this resume ABI:
  they compile to hidden `Generator` MIR functions with explicit `Yield`
  terminators, ordinary `iter`/`next` behavior, repeated exhaustion, exception
  termination, and public forced-GC proof. General source `yield`, `send`,
  `throw`, `close`, and `yield from` remain Gate 6-owned.
- Generated generator-resume code keeps the heap representation opaque through
  the narrow `rimera_generator_function_get`, `rimera_generator_state_get`,
  `rimera_generator_state_set`, `rimera_generator_slot_get`, and
  `rimera_generator_slot_set` helpers. A suspension saves every liveness-selected
  persistent value before publishing the next state; resumption restores those
  slots before entering the compiler-selected continuation. These helpers expose
  neither heap pointers nor generator layout and are not a runtime MIR evaluator.
- `rimera_cell_*`, `rimera_global_*`, and
  `rimera_function_closure_get` implement mutable lexical cells, module
  globals, and builtins fallback without exposing heap pointers to generated
  code. Global fallback lazily publishes known builtin types and native builtin
  functions in the context-owned builtins dictionary.
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
  `rimera_class_name_get`, and `rimera_namespace_delete` build and mutate the ordered,
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
  contracts. `rimera_class_name_get` resolves the prepared namespace before
  module globals and lazy builtins. A custom `__prepare__` mapping is accessed
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
  the ultimate exporter. PEP 688 user-defined `__buffer__`/
  `__release_buffer__` dispatch is intentionally deferred to Gate 7 rather than
  being represented by a partial ABI-v1 exporter hook.
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
  `rimera_generator_resume`, and the state/slot accessors are the single native
  generator ABI. Gate 4 generator expressions now exercise that ABI through
  verified suspension MIR and public native source tests. This does not imply
  general generator completion: source `yield`, `send`, `throw`, `close`, and
  `yield from` remain Gate 6-owned.

## Structured exceptions

- Exception instances are managed values containing their type, `args`,
  traceback, explicit cause, implicit context, and suppression flag.
- `rimera_raise`, `rimera_reraise`, `rimera_handler_enter`,
  `rimera_handler_leave`, and `rimera_exception_propagate` maintain structured
  context-owned state. Generated code uses explicit normal and exception CFG
  edges; host unwinding is not exception control flow.
- `rimera_traceback_append` attaches immutable filename/function/line frames
  when an exception escapes a compiled function.
- `rimera_exception_split` recursively partitions ordinary exceptions and
  exception groups while preserving subgroup shape. The split result is an
  internal two-element value array containing matched and remainder values;
  `None` denotes an empty partition.
- `rimera_exception_set_active`, `rimera_exception_clear_active`, and
  `rimera_exception_combine` support deferred `except*` handler execution and
  merging. All matching handlers run before accumulated handler failures and
  the unmatched remainder are propagated.
