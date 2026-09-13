use std::collections::{BTreeMap, BTreeSet};
#[cfg(panic = "unwind")]
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::ptr;

use rimera_abi::{
    RGeneratorOperation, RGeneratorOutcome, RNativeGeneratorResume, RNativeModuleInitializer,
    RRootFrame, RStatus, RTag, RValue,
};

use crate::heap::{Heap, HeapObject, HeapStats, LifecycleAction, MIN_COLLECTION_THRESHOLD};
use crate::module_registry::{ModuleRegistry, NativeModuleDefinition};
use crate::object::{
    BoundMethodObject, BuiltinFunctionKind, BuiltinFunctionObject, ClassMethodObject, CodeObject,
    DictionaryObject, ExceptionObject, FastCallMetadata, FrameObject, FunctionKind,
    GeneratorObject, InstanceObject, MemberDescriptorObject, ModuleObject, ModuleState,
    PropertyMethodKind, PropertyMethodObject, PropertyObject, StaticMethodObject, SuperObject,
    SuspendedKind, TYPE_FLAG_BUILTIN, TYPE_FLAG_EXCEPTION, TYPE_FLAG_INSTANTIABLE, TracebackObject,
    TypeLayout, TypeObject,
};

const HEAP_LIMIT_MESSAGE: &str = "managed heap limit exceeded";

fn inherited_layout(heap: &Heap, bases: &[RValue]) -> Result<TypeLayout, String> {
    let layouts = bases
        .iter()
        .filter_map(|base| match heap.get(*base) {
            Some(HeapObject::Type(object)) if object.layout != TypeLayout::Object => {
                Some(object.layout)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if layouts.windows(2).any(|pair| pair[0] != pair[1]) {
        return Err("multiple bases have instance lay-out conflict".to_owned());
    }
    Ok(layouts.first().copied().unwrap_or(TypeLayout::Object))
}

/// Builtin types whose metadata is only needed once Python code observes the
/// corresponding value.
const LAZY_BUILTIN_TYPES: &[(&str, &str)] = &[
    ("NoneType", "object"),
    ("NotImplementedType", "object"),
    ("ellipsis", "object"),
    ("int", "object"),
    ("float", "object"),
    ("complex", "object"),
    ("bool", "int"),
    ("str", "object"),
    ("bytes", "object"),
    ("bytearray", "object"),
    ("list", "object"),
    ("tuple", "object"),
    ("dict", "object"),
    ("set", "object"),
    ("frozenset", "object"),
    ("slice", "object"),
    ("dict_keys", "object"),
    ("dict_values", "object"),
    ("dict_items", "object"),
    ("mappingproxy", "object"),
    ("memoryview", "object"),
    ("module", "object"),
    ("range", "object"),
    ("function", "object"),
    ("code", "object"),
    ("builtin_function_or_method", "object"),
    ("iterator", "object"),
    ("cell", "object"),
    ("traceback", "object"),
    ("frame", "object"),
    ("generator", "object"),
    ("coroutine", "object"),
    ("async_generator", "object"),
    ("async_generator_asend", "object"),
    ("async_generator_athrow", "object"),
    ("anext_awaitable", "object"),
    ("super", "object"),
    ("TypeVar", "object"),
    ("TypeVarTuple", "object"),
    ("ParamSpec", "object"),
    ("TypeAliasType", "object"),
    ("SourceFileLoader", "object"),
    ("NamespaceLoader", "object"),
    ("ModuleSpec", "object"),
];

/// Gate-specific exception classes that do not need to inflate the permanent
/// startup kernel. They are published exactly like eager exception types on
/// first lookup or first raise, and retain exception-type identity thereafter.
const LAZY_BUILTIN_EXCEPTIONS: &[(&str, &str)] = &[
    ("SyntaxError", "Exception"),
    ("IndentationError", "SyntaxError"),
    ("TabError", "IndentationError"),
    ("KeyError", "Exception"),
    ("OverflowError", "Exception"),
    ("BufferError", "Exception"),
    ("AssertionError", "Exception"),
    ("GeneratorExit", "BaseException"),
    ("StopIteration", "Exception"),
    ("StopAsyncIteration", "Exception"),
    ("ImportError", "Exception"),
    ("ModuleNotFoundError", "ImportError"),
];

fn is_public_builtin_type_name(name: &str) -> bool {
    matches!(
        name,
        "object"
            | "type"
            | "bool"
            | "int"
            | "float"
            | "complex"
            | "str"
            | "bytes"
            | "bytearray"
            | "list"
            | "tuple"
            | "dict"
            | "set"
            | "frozenset"
            | "slice"
            | "memoryview"
            | "range"
            | "super"
            | "BaseException"
            | "Exception"
            | "BaseExceptionGroup"
            | "ExceptionGroup"
            | "TypeError"
            | "ValueError"
            | "SyntaxError"
            | "IndentationError"
            | "TabError"
            | "RuntimeError"
            | "NameError"
            | "AttributeError"
            | "UnboundLocalError"
            | "ZeroDivisionError"
            | "IndexError"
            | "MemoryError"
            | "KeyError"
            | "OverflowError"
            | "BufferError"
            | "AssertionError"
            | "GeneratorExit"
            | "StopIteration"
            | "StopAsyncIteration"
            | "ImportError"
            | "ModuleNotFoundError"
    )
}

#[derive(Debug)]
struct KernelRoots {
    types: BTreeMap<&'static str, RValue>,
    globals: RValue,
    builtins: RValue,
    module_cache: RValue,
    emergency_memory_error: RValue,
}

const GLOBAL_LOOKUP_CACHE_LEN: usize = 64;

#[derive(Debug, Clone, Copy)]
struct GlobalLookupCacheEntry {
    callsite: usize,
    namespace: RValue,
    entry_index: usize,
    valid: bool,
}

impl GlobalLookupCacheEntry {
    const EMPTY: Self = Self {
        callsite: 0,
        namespace: RValue::NONE,
        entry_index: 0,
        valid: false,
    };
}

#[derive(Debug, Clone, Copy)]
struct NativeLocals {
    values: *const RValue,
    len: usize,
}

#[derive(Debug, Clone)]
struct ActiveCall {
    function: RValue,
    receiver: Option<RValue>,
    generator: Option<RValue>,
    frame: Option<RValue>,
    local_cells: Vec<(String, RValue)>,
    native_locals: Option<NativeLocals>,
    locals_snapshot: Option<RValue>,
    namespace: Option<RValue>,
    comprehension: bool,
    transient_names: BTreeSet<String>,
    overlay_base: Option<RValue>,
    overlay_restore: Vec<(String, Option<RValue>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GeneratorResume {
    pub value: RValue,
    pub outcome: RGeneratorOutcome,
}

type BufferLeaseFinalizer = fn(&mut RimeraContext, RValue) -> Result<(), String>;
type GeneratorFinalizer = fn(&mut RimeraContext, RValue) -> Result<(), String>;
pub(crate) type AsyncRootRunner = fn(&mut RimeraContext, RValue) -> Result<RValue, String>;

#[derive(Debug)]
pub struct RimeraContext {
    pub(crate) heap: Heap,
    pub(crate) roots: *mut RRootFrame,
    native_roots: Vec<RValue>,
    context_roots: Vec<RValue>,
    pub(crate) exception: Option<String>,
    pub(crate) raised: Option<RValue>,
    pub(crate) handled: Vec<RValue>,
    ambient_exception_context: Vec<RValue>,
    global_lookup_cache: [GlobalLookupCacheEntry; GLOBAL_LOOKUP_CACHE_LEN],
    active_calls: Vec<ActiveCall>,
    module_namespaces: Vec<RValue>,
    module_frame: Option<RValue>,
    module_registry: ModuleRegistry,
    kernel: Option<KernelRoots>,
    next_collection_bytes: usize,
    heap_limit_bytes: Option<usize>,
    dynamic_compilation_enabled: bool,
    pub(crate) dynamic: crate::dynamic::DynamicState,
    buffer_lease_finalizer: Option<BufferLeaseFinalizer>,
    generator_finalizer: Option<GeneratorFinalizer>,
    async_root_runner: Option<AsyncRootRunner>,
    async_root_running: bool,
}

impl Default for RimeraContext {
    fn default() -> Self {
        Self {
            heap: Heap::default(),
            roots: ptr::null_mut(),
            native_roots: Vec::new(),
            context_roots: Vec::new(),
            exception: None,
            raised: None,
            handled: Vec::new(),
            ambient_exception_context: Vec::new(),
            global_lookup_cache: [GlobalLookupCacheEntry::EMPTY; GLOBAL_LOOKUP_CACHE_LEN],
            active_calls: Vec::new(),
            module_namespaces: Vec::new(),
            module_frame: None,
            module_registry: ModuleRegistry::default(),
            kernel: None,
            next_collection_bytes: MIN_COLLECTION_THRESHOLD,
            heap_limit_bytes: None,
            dynamic_compilation_enabled: false,
            dynamic: crate::dynamic::DynamicState::default(),
            buffer_lease_finalizer: None,
            generator_finalizer: None,
            async_root_runner: None,
            async_root_running: false,
        }
    }
}

impl RimeraContext {
    pub(crate) fn enable_dynamic_compilation(&mut self) {
        self.dynamic_compilation_enabled = true;
    }

    #[must_use]
    pub(crate) fn dynamic_compilation_enabled(&self) -> bool {
        self.dynamic_compilation_enabled
    }

    pub(crate) fn allocate(&mut self, object: HeapObject) -> Result<RValue, String> {
        let size = object.managed_size();
        let projected = self.heap.live_bytes().saturating_add(size);
        if projected > self.next_collection_bytes
            || self.heap_limit_bytes.is_some_and(|limit| projected > limit)
        {
            self.collect();
        }
        if self
            .heap_limit_bytes
            .is_some_and(|limit| self.heap.live_bytes().saturating_add(size) > limit)
        {
            let family = match &object {
                HeapObject::Generator(_) => "generator",
                HeapObject::AsyncNextAwaitable(_) => "async-next-awaitable",
                HeapObject::AsyncGeneratorOperation(_) => "async-generator-operation",
                HeapObject::BoundMethod(_) => "bound-method",
                HeapObject::Exception(_) => "exception",
                HeapObject::Frame(_) => "frame",
                HeapObject::Tuple(_) => "tuple",
                HeapObject::List(_) => "list",
                HeapObject::String(_) => "string",
                _ => "other",
            };
            eprintln!(
                "DBG heap-limit family={family} live={} request={} limit={}",
                self.heap.live_bytes(),
                size,
                self.heap_limit_bytes.unwrap_or(0)
            );
            return Err(HEAP_LIMIT_MESSAGE.to_owned());
        }
        Ok(self.heap.allocate(object))
    }

    pub fn collect(&mut self) {
        loop {
            let roots = self.discover_roots();
            let lifecycle_actions = self.heap.collect(roots);
            if lifecycle_actions.is_empty() {
                break;
            }
            for action in lifecycle_actions {
                let saved_raised = self.raised.take();
                let saved_exception = self.exception.take();
                match action {
                    LifecycleAction::FinalizeBufferLease(lease) => {
                        let finalizer = self.buffer_lease_finalizer.expect(
                            "buffer lease finalizer must be installed before lease allocation",
                        );
                        let _ = self
                            .with_temporary_roots(&[lease], |context| finalizer(context, lease));
                    }
                    LifecycleAction::CloseGenerator(generator) => {
                        let finalizer = self.generator_finalizer.expect(
                            "generator finalizer must be installed before generator allocation",
                        );
                        let _ = self.with_temporary_roots(&[generator], |context| {
                            finalizer(context, generator)
                        });
                    }
                }
                // GC-triggered release callback failures are unraisable here;
                // generator cleanup failures follow the same rule. Neither may
                // replace the exception state of the allocating safepoint.
                self.raised = saved_raised;
                self.exception = saved_exception;
            }
        }
        self.next_collection_bytes =
            MIN_COLLECTION_THRESHOLD.max(self.heap.live_bytes().saturating_mul(2));
    }

    pub(crate) fn enable_buffer_lease_finalizer(&mut self) {
        if self.buffer_lease_finalizer.is_none() {
            self.buffer_lease_finalizer = Some(Self::finalize_buffer_lease);
        }
    }

    fn enable_generator_finalizer(&mut self) {
        if self.generator_finalizer.is_none() {
            self.generator_finalizer = Some(Self::finalize_generator);
        }
    }

    fn finalize_generator(&mut self, generator: RValue) -> Result<(), String> {
        let (kind, started, function, qualified_name_override) = match self.heap.get(generator) {
            Some(HeapObject::Generator(object)) => (
                object.kind,
                object.started,
                object.function,
                object.qualified_name_override.as_deref(),
            ),
            _ => return Err("suspended-object finalizer target is invalid".to_owned()),
        };
        if kind == SuspendedKind::Coroutine && !started {
            let qualified_name =
                qualified_name_override.unwrap_or_else(|| match self.heap.get(function) {
                    Some(HeapObject::Function(function)) => function.qualified_name.as_str(),
                    _ => "<invalid coroutine>",
                });
            eprintln!("RuntimeWarning: coroutine '{qualified_name}' was never awaited");
        }
        let Some(HeapObject::Generator(object)) = self.heap.get_mut(generator) else {
            return Err("suspended-object finalizer target is invalid".to_owned());
        };
        object.finalizer_ran = true;
        self.resume_generator(generator, RGeneratorOperation::Close, RValue::NONE)
            .map(|_| ())
    }

    /// Run lifecycle cleanup for live suspended objects before destroying the
    /// entire runtime context. Tracing GC normally discovers unreachable
    /// coroutines at allocation safepoints; a final expression can create an
    /// unawaited coroutine without another allocation, so context teardown is
    /// the final deterministic lifecycle boundary.
    pub(crate) fn finalize_suspended_for_shutdown(&mut self) {
        let pending = self
            .heap
            .live_values()
            .into_iter()
            .filter(|value| {
                matches!(
                    self.heap.get(*value),
                    Some(HeapObject::Generator(object))
                        if !object.running
                            && !object.closed
                            && !object.completed
                            && !object.finalizer_ran
                )
            })
            .collect::<Vec<_>>();
        for suspended in pending {
            let saved_raised = self.raised;
            let saved_exception = self.exception.clone();
            let _ = self.finalize_generator(suspended);
            self.raised = saved_raised;
            self.exception = saved_exception;
        }
    }

    pub(crate) fn release_buffer_lease_view(&mut self, lease: RValue) -> Result<(), String> {
        let should_finalize = match self.heap.get_mut(lease) {
            Some(HeapObject::BufferLease(lease)) => {
                if lease.released {
                    false
                } else {
                    lease.active_views = lease.active_views.saturating_sub(1);
                    lease.active_views == 0
                }
            }
            _ => return Err("memoryview has an invalid provider lease".to_owned()),
        };
        if should_finalize {
            self.finalize_buffer_lease(lease)
        } else {
            Ok(())
        }
    }

    pub(crate) fn finalize_buffer_lease(&mut self, lease: RValue) -> Result<(), String> {
        let (provider, exported_view) = match self.heap.get(lease) {
            Some(HeapObject::BufferLease(lease)) if !lease.released => {
                (lease.provider, lease.exported_view)
            }
            Some(HeapObject::BufferLease(_)) => return Ok(()),
            _ => return Err("memoryview has an invalid provider lease".to_owned()),
        };
        let Some(HeapObject::BufferLease(lease_object)) = self.heap.get_mut(lease) else {
            unreachable!("provider lease was checked above");
        };
        lease_object.released = true;
        lease_object.active_views = 0;

        let saved_raised = self.raised.take();
        let saved_exception = self.exception.take();
        let _callback_result = match self.special_method(provider, "__release_buffer__") {
            Ok(Some(callback)) => self
                .with_temporary_roots(&[lease, provider, exported_view, callback], |context| {
                    crate::call::invoke(context, callback, &[exported_view], &[]).map(|_| ())
                }),
            Ok(None) => Ok(()),
            Err(error) => Err(error),
        };
        // CPython reports release-callback failures as unraisable and still
        // makes memoryview.release() succeed. Preserve the caller's exception
        // state while dropping the native export obligation unconditionally.
        self.raised = saved_raised;
        self.exception = saved_exception;
        crate::operations::memoryview_release(self, exported_view)
    }

    fn discover_roots(&self) -> Vec<RValue> {
        let mut roots = Vec::new();
        let mut frame = self.roots;
        while !frame.is_null() {
            // SAFETY: generated code registers live frames and removes them
            // before their stack storage expires.
            let root = unsafe { &*frame };
            if !root.slots.is_null() {
                // SAFETY: the root ABI guarantees `len` initialized values.
                roots
                    .extend_from_slice(unsafe { std::slice::from_raw_parts(root.slots, root.len) });
            }
            frame = root.previous;
        }
        roots.extend_from_slice(&self.native_roots);
        roots.extend_from_slice(&self.context_roots);
        roots.extend(self.raised);
        roots.extend(self.handled.iter().copied());
        roots.extend(self.ambient_exception_context.iter().copied());
        roots.extend(self.module_namespaces.iter().copied());
        roots.extend(self.module_frame);
        for active in &self.active_calls {
            roots.push(active.function);
            active
                .receiver
                .into_iter()
                .for_each(|value| roots.push(value));
            active
                .generator
                .into_iter()
                .for_each(|value| roots.push(value));
            active.frame.into_iter().for_each(|value| roots.push(value));
            active
                .local_cells
                .iter()
                .map(|(_, cell)| *cell)
                .for_each(|value| roots.push(value));
            active
                .locals_snapshot
                .into_iter()
                .for_each(|value| roots.push(value));
            active
                .namespace
                .into_iter()
                .for_each(|value| roots.push(value));
            active
                .overlay_base
                .into_iter()
                .for_each(|value| roots.push(value));
            active
                .overlay_restore
                .iter()
                .filter_map(|(_, value)| *value)
                .for_each(|value| roots.push(value));
        }
        roots
    }

    pub(crate) fn with_temporary_roots<T>(
        &mut self,
        roots: &[RValue],
        operation: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let checkpoint = self.native_roots.len();
        self.native_roots.extend_from_slice(roots);
        #[cfg(panic = "unwind")]
        {
            let result = catch_unwind(AssertUnwindSafe(|| operation(self)));
            self.native_roots.truncate(checkpoint);
            match result {
                Ok(value) => value,
                Err(payload) => resume_unwind(payload),
            }
        }
        #[cfg(panic = "abort")]
        {
            let value = operation(self);
            self.native_roots.truncate(checkpoint);
            value
        }
    }

    pub(crate) fn install_async_root_runner(&mut self, runner: AsyncRootRunner) {
        self.async_root_runner = Some(runner);
    }

    pub(crate) fn run_async_root(&mut self, coroutine: RValue) -> Result<RValue, String> {
        if self.async_root_running {
            return self.raise_error("RuntimeError", "async_runtime.run() cannot be nested");
        }
        let Some(runner) = self.async_root_runner else {
            return self.raise_error(
                "RuntimeError",
                "async runtime backend is not linked into this executable",
            );
        };
        self.async_root_running = true;
        let result = runner(self, coroutine);
        self.async_root_running = false;
        result
    }

    pub(crate) fn heap_limit_is_unbounded(&self) -> bool {
        self.heap_limit_bytes.is_none()
    }

    pub(crate) fn set_heap_limit(&mut self, bytes: Option<usize>) -> Result<(), String> {
        if bytes.is_some_and(|limit| self.heap.live_bytes() > limit) {
            self.collect();
        }
        if bytes.is_some_and(|limit| self.heap.live_bytes() > limit) {
            return Err(HEAP_LIMIT_MESSAGE.to_owned());
        }
        self.heap_limit_bytes = bytes;
        Ok(())
    }

    pub(crate) fn enforce_heap_limit(&mut self) -> Result<(), String> {
        if self.heap_limit_bytes.is_none() {
            return Ok(());
        }
        self.heap.refresh_managed_bytes();
        if self
            .heap_limit_bytes
            .is_some_and(|limit| self.heap.live_bytes() > limit)
        {
            self.collect();
        }
        if self
            .heap_limit_bytes
            .is_some_and(|limit| self.heap.live_bytes() > limit)
        {
            return Err(HEAP_LIMIT_MESSAGE.to_owned());
        }
        Ok(())
    }

    pub(crate) fn preflight_managed_growth(&mut self, additional: usize) -> Result<(), String> {
        if additional == 0 {
            return Ok(());
        }
        self.heap.refresh_managed_bytes();
        let projected = self.heap.live_bytes().saturating_add(additional);
        if self.heap_limit_bytes.is_some_and(|limit| projected > limit) {
            self.collect();
            self.heap.refresh_managed_bytes();
        }
        if self
            .heap_limit_bytes
            .is_some_and(|limit| self.heap.live_bytes().saturating_add(additional) > limit)
        {
            eprintln!(
                "DBG heap-growth live={} additional={} limit={}",
                self.heap.live_bytes(),
                additional,
                self.heap_limit_bytes.unwrap_or(0)
            );
            return Err(HEAP_LIMIT_MESSAGE.to_owned());
        }
        Ok(())
    }

    #[must_use]
    pub fn stats(&self) -> HeapStats {
        self.heap.stats(self.next_collection_bytes)
    }

    pub(crate) fn fail(&mut self, message: impl Into<String>) {
        self.exception = Some(message.into());
    }

    pub(crate) fn push_active_call(&mut self, function: RValue, receiver: Option<RValue>) {
        self.active_calls.push(ActiveCall {
            function,
            receiver,
            generator: None,
            frame: None,
            local_cells: Vec::new(),
            native_locals: None,
            locals_snapshot: None,
            namespace: None,
            comprehension: false,
            transient_names: BTreeSet::new(),
            overlay_base: None,
            overlay_restore: Vec::new(),
        });
    }

    pub(crate) fn push_active_generator_call(
        &mut self,
        function: RValue,
        generator: RValue,
    ) -> Result<(), String> {
        let (local_cells, locals_snapshot) = match self.heap.get(generator) {
            Some(HeapObject::Generator(generator)) => {
                (generator.local_cells.clone(), generator.locals_snapshot)
            }
            _ => return Err("generator activation is invalid".to_owned()),
        };
        self.active_calls.push(ActiveCall {
            function,
            receiver: None,
            generator: Some(generator),
            frame: match self.heap.get(generator) {
                Some(HeapObject::Generator(generator)) => generator.frame,
                _ => None,
            },
            local_cells,
            native_locals: None,
            locals_snapshot,
            namespace: None,
            comprehension: false,
            transient_names: BTreeSet::new(),
            overlay_base: None,
            overlay_restore: Vec::new(),
        });
        Ok(())
    }

    pub(crate) fn pop_active_call(&mut self) {
        let Some(active) = self.active_calls.pop() else {
            return;
        };
        if let Some(base) = active.overlay_base {
            for (name, previous) in active.overlay_restore.iter().rev() {
                match previous {
                    Some(value) => self.direct_namespace_set(base, name, *value),
                    None => self.direct_namespace_delete(base, name),
                }
            }
        }
        if let Some(generator) = active.generator
            && let Some(HeapObject::Generator(object)) = self.heap.get_mut(generator)
        {
            object.local_cells = active.local_cells;
            object.locals_snapshot = active.locals_snapshot;
        }
    }

    pub(crate) fn configure_active_scope(
        &mut self,
        namespace: Option<RValue>,
        comprehension: bool,
    ) -> Result<(), String> {
        let active = self
            .active_calls
            .last_mut()
            .ok_or_else(|| "reflection scope has no active native activation".to_owned())?;
        active.namespace = namespace;
        active.comprehension = comprehension;
        Ok(())
    }

    pub(crate) fn register_active_native_locals(
        &mut self,
        values: *const RValue,
        len: usize,
    ) -> Result<(), String> {
        if len != 0 && values.is_null() {
            return Err("native local registration requires live storage".to_owned());
        }
        let active = self
            .active_calls
            .last_mut()
            .ok_or_else(|| "native locals have no active native activation".to_owned())?;
        active.native_locals = (len != 0).then_some(NativeLocals { values, len });
        Ok(())
    }

    pub(crate) fn register_active_local(&mut self, name: &str, cell: RValue) -> Result<(), String> {
        if !matches!(self.heap.get(cell), Some(HeapObject::Cell(_))) {
            return Err("reflection local registration requires a cell".to_owned());
        }
        let active = self
            .active_calls
            .last_mut()
            .ok_or_else(|| "reflection local has no active native activation".to_owned())?;
        if let Some((_, current)) = active
            .local_cells
            .iter_mut()
            .find(|(current, _)| current == name)
        {
            *current = cell;
        } else {
            active.local_cells.push((name.to_owned(), cell));
        }
        Ok(())
    }

    fn direct_namespace_set(&mut self, namespace: RValue, name: &str, value: RValue) {
        match self.heap.get_mut(namespace) {
            Some(HeapObject::Dictionary(dictionary)) => {
                dictionary.insert(name.to_owned(), value);
            }
            Some(HeapObject::ValueDictionary(_)) => {
                let _ = self.namespace_set(namespace, name, value);
            }
            _ => {}
        }
    }

    fn direct_namespace_delete(&mut self, namespace: RValue, name: &str) {
        match self.heap.get_mut(namespace) {
            Some(HeapObject::Dictionary(dictionary)) => {
                dictionary.remove(name);
            }
            Some(HeapObject::ValueDictionary(_)) => {
                let _ = self.namespace_delete(namespace, name);
            }
            _ => {}
        }
    }

    fn refresh_locals_at(&mut self, index: usize) -> Result<RValue, String> {
        let (namespace, comprehension) = {
            let active = self
                .active_calls
                .get(index)
                .ok_or_else(|| "active locals index is invalid".to_owned())?;
            (active.namespace, active.comprehension)
        };
        if let Some(namespace) = namespace {
            return Ok(namespace);
        }
        if comprehension {
            return self.refresh_comprehension_locals(index);
        }
        let snapshot = if let Some(existing) = self.active_calls[index].locals_snapshot {
            existing
        } else {
            let dictionary = self.allocate(HeapObject::Dictionary(DictionaryObject {
                entries: Vec::new(),
            }))?;
            self.active_calls[index].locals_snapshot = Some(dictionary);
            if let Some(generator) = self.active_calls[index].generator
                && let Some(HeapObject::Generator(object)) = self.heap.get_mut(generator)
            {
                object.locals_snapshot = Some(dictionary);
            }
            dictionary
        };
        let transient = std::mem::take(&mut self.active_calls[index].transient_names);
        for name in transient {
            self.direct_namespace_delete(snapshot, &name);
        }
        let local_cells = self.active_calls[index].local_cells.clone();
        for (name, cell) in local_cells {
            let value = match self.heap.get(cell) {
                Some(HeapObject::Cell(cell)) => cell.value,
                _ => None,
            };
            match value {
                Some(value) => self.direct_namespace_set(snapshot, &name, value),
                None => self.direct_namespace_delete(snapshot, &name),
            }
        }
        if let Some(native) = self.active_calls[index].native_locals {
            let function = self.active_calls[index].function;
            let native_names = match self.heap.get(function) {
                Some(HeapObject::Function(function)) => match self.heap.get(function.code) {
                    Some(HeapObject::Code(code)) => code
                        .local_names
                        .iter()
                        .filter(|name| !code.cell_names.iter().any(|cell| cell == *name))
                        .take(native.len)
                        .cloned()
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                },
                _ => Vec::new(),
            };
            for (slot, name) in native_names.into_iter().enumerate() {
                // SAFETY: compiled code registers this stack array only for the
                // lifetime of the matching active call and clears the call
                // before returning from the native activation.
                let value = unsafe { *native.values.add(slot) };
                if value.is_unbound() {
                    self.direct_namespace_delete(snapshot, &name);
                } else {
                    self.direct_namespace_set(snapshot, &name, value);
                }
            }
        }
        Ok(snapshot)
    }

    fn refresh_comprehension_locals(&mut self, index: usize) -> Result<RValue, String> {
        let base_index = (0..index)
            .rev()
            .find(|candidate| !self.active_calls[*candidate].comprehension);
        let (base, live_mapping, parent_index) = if let Some(parent) = base_index {
            if let Some(namespace) = self.active_calls[parent].namespace {
                (namespace, true, Some(parent))
            } else {
                (self.refresh_locals_at(parent)?, false, Some(parent))
            }
        } else {
            (
                self.globals()
                    .ok_or_else(|| "module globals are unavailable".to_owned())?,
                true,
                None,
            )
        };
        let overlay_start = base_index.map_or(0, |parent| parent + 1);
        let locals = self.active_calls[overlay_start..=index]
            .iter()
            .filter(|active| active.comprehension)
            .flat_map(|active| active.local_cells.iter().cloned())
            .collect::<Vec<_>>();
        for (name, cell) in locals {
            let value = match self.heap.get(cell) {
                Some(HeapObject::Cell(cell)) => cell.value,
                _ => None,
            };
            if live_mapping {
                let recorded = self.active_calls[index]
                    .overlay_restore
                    .iter()
                    .any(|(current, _)| current == &name);
                if !recorded {
                    let previous = self.namespace_value(base, &name);
                    self.active_calls[index]
                        .overlay_restore
                        .push((name.clone(), previous));
                    self.active_calls[index].overlay_base = Some(base);
                }
            } else if let Some(parent) = parent_index {
                self.active_calls[parent]
                    .transient_names
                    .insert(name.clone());
            }
            match value {
                Some(value) => self.direct_namespace_set(base, &name, value),
                None => self.direct_namespace_delete(base, &name),
            }
        }
        Ok(base)
    }

    pub(crate) fn current_locals(&mut self) -> Result<RValue, String> {
        if self.active_calls.is_empty() {
            self.initialize_kernel()?;
            return self
                .globals()
                .ok_or_else(|| "module globals are unavailable".to_owned());
        }
        self.refresh_locals_at(self.active_calls.len() - 1)
    }

    fn active_frame_default_line(&self, index: usize) -> u32 {
        self.active_calls
            .get(index)
            .and_then(|active| match self.heap.get(active.function) {
                Some(HeapObject::Function(function)) => Some(function.code),
                _ => None,
            })
            .and_then(|code| match self.heap.get(code) {
                Some(HeapObject::Code(code)) => Some(code.first_line),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn ensure_active_frame(&mut self, index: usize, line: u32) -> Result<RValue, String> {
        if let Some(frame) = self.active_calls.get(index).and_then(|active| active.frame) {
            if let Some(HeapObject::Frame(frame_object)) = self.heap.get_mut(frame) {
                frame_object.line = line;
            }
            return Ok(frame);
        }
        let function = self
            .active_calls
            .get(index)
            .map(|active| active.function)
            .ok_or_else(|| "active frame index is invalid".to_owned())?;
        let code = match self.heap.get(function) {
            Some(HeapObject::Function(function)) => function.code,
            _ => return Err("active frame function is invalid".to_owned()),
        };
        let globals = self
            .globals()
            .ok_or_else(|| "module globals are unavailable".to_owned())?;
        let back = if index == 0 {
            None
        } else {
            let caller_line = self.active_frame_default_line(index - 1);
            Some(self.ensure_active_frame(index - 1, caller_line)?)
        };
        let locals = self.refresh_locals_at(index)?;
        let generator = self.active_calls[index].generator;
        let roots = back
            .into_iter()
            .chain(generator)
            .chain([function, code, globals, locals])
            .collect::<Vec<_>>();
        let frame = self.with_temporary_roots(&roots, |context| {
            context.allocate(HeapObject::Frame(Box::new(FrameObject {
                code,
                globals,
                locals,
                back,
                line,
                generator,
            })))
        })?;
        self.active_calls[index].frame = Some(frame);
        if let Some(generator) = generator
            && let Some(HeapObject::Generator(generator)) = self.heap.get_mut(generator)
        {
            generator.frame = Some(frame);
        }
        Ok(frame)
    }

    fn ensure_module_frame(&mut self, filename: &str, line: u32) -> Result<RValue, String> {
        if let Some(frame) = self.module_frame {
            if let Some(HeapObject::Frame(frame_object)) = self.heap.get_mut(frame) {
                frame_object.line = line;
            }
            return Ok(frame);
        }
        let globals = self
            .globals()
            .ok_or_else(|| "module globals are unavailable".to_owned())?;
        let code = self.with_temporary_roots(&[globals], |context| {
            context.allocate(HeapObject::Code(CodeObject {
                dynamic_mode: None,
                flags_override: None,
                code_address: 0,
                kind: FunctionKind::Normal,
                name: "<module>".to_owned(),
                qualified_name: "<module>".to_owned(),
                parameters: Box::new([]),
                filename: filename.to_owned(),
                first_line: 1,
                local_names: Box::new([]),
                cell_names: Box::new([]),
                free_names: Box::new([]),
            }))
        })?;
        let frame = self.with_temporary_roots(&[globals, code], |context| {
            context.allocate(HeapObject::Frame(Box::new(FrameObject {
                code,
                globals,
                locals: globals,
                back: None,
                line,
                generator: None,
            })))
        })?;
        self.module_frame = Some(frame);
        Ok(frame)
    }

    pub(crate) fn namespace_names(&self, namespace: RValue) -> Vec<String> {
        match self.heap.get(namespace) {
            Some(HeapObject::Dictionary(dictionary)) => dictionary
                .entries
                .iter()
                .map(|(name, _)| name.clone())
                .collect(),
            Some(HeapObject::ValueDictionary(dictionary)) => dictionary
                .table
                .values()
                .filter_map(|(key, _)| match self.heap.get(*key) {
                    Some(HeapObject::String(name)) => Some(name.clone()),
                    _ => None,
                })
                .collect(),
            Some(HeapObject::MappingProxy(proxy)) => self.namespace_names(proxy.dictionary),
            _ => Vec::new(),
        }
    }

    pub(crate) fn default_dir_names(&mut self, value: RValue) -> Result<Vec<String>, String> {
        let mut names = BTreeSet::new();
        match self.heap.get(value) {
            Some(HeapObject::Module(module)) => {
                names.extend(self.namespace_names(module.namespace));
            }
            Some(HeapObject::Instance(instance)) => {
                if let Some(dictionary) = instance.dictionary {
                    names.extend(self.namespace_names(dictionary));
                }
                let class = instance.class;
                if let Some(HeapObject::Type(class)) = self.heap.get(class) {
                    for candidate in class.mro.iter().copied() {
                        if let Some(HeapObject::Type(candidate)) = self.heap.get(candidate) {
                            names.extend(self.namespace_names(candidate.namespace));
                        }
                    }
                }
            }
            Some(HeapObject::Type(class)) => {
                let class_mro = class.mro.to_vec();
                let metaclass = class.metaclass;
                for candidate in class_mro {
                    if let Some(HeapObject::Type(candidate)) = self.heap.get(candidate) {
                        names.extend(self.namespace_names(candidate.namespace));
                    }
                }
                if let Some(HeapObject::Type(meta)) = self.heap.get(metaclass) {
                    for candidate in meta.mro.iter().copied() {
                        if let Some(HeapObject::Type(candidate)) = self.heap.get(candidate) {
                            names.extend(self.namespace_names(candidate.namespace));
                        }
                    }
                }
            }
            _ => {
                let class = self.type_of(value)?;
                if let Some(HeapObject::Type(class)) = self.heap.get(class) {
                    for candidate in class.mro.iter().copied() {
                        if let Some(HeapObject::Type(candidate)) = self.heap.get(candidate) {
                            names.extend(self.namespace_names(candidate.namespace));
                        }
                    }
                }
            }
        }
        Ok(names.into_iter().collect())
    }

    pub(crate) fn zero_argument_super(&mut self) -> Result<RValue, String> {
        let active = self
            .active_calls
            .last()
            .cloned()
            .ok_or_else(|| "super(): no arguments".to_owned())?;
        let receiver = active
            .receiver
            .ok_or_else(|| "super(): no arguments".to_owned())?;
        let class_cell = match self.heap.get(active.function) {
            Some(HeapObject::Function(function)) => function.closure.and_then(|closure| match self
                .heap
                .get(closure)
            {
                Some(HeapObject::Tuple(cells)) => cells.last().copied(),
                _ => None,
            }),
            _ => None,
        }
        .ok_or_else(|| "super(): __class__ cell not found".to_owned())?;
        let class = match self.heap.get(class_cell) {
            Some(HeapObject::Cell(cell)) => cell.value,
            _ => None,
        }
        .ok_or_else(|| "super(): __class__ cell not found".to_owned())?;
        self.new_super(class, receiver)
    }

    pub(crate) fn initialize_kernel(&mut self) -> Result<(), String> {
        if self.kernel.is_some() {
            return Ok(());
        }
        let object_type = self.allocate_kernel_object(HeapObject::Type(TypeObject {
            name: "object".to_owned(),
            qualified_name: "object".to_owned(),
            metaclass: RValue::NONE,
            bases: Box::new([]),
            mro: Box::new([]),
            namespace: RValue::NONE,
            slot_names: Box::new([]),
            has_dictionary: false,
            has_weakref: false,
            flags: TYPE_FLAG_BUILTIN | TYPE_FLAG_INSTANTIABLE,
            version_tag: 0,
            layout: TypeLayout::Object,
        }))?;
        let type_type = self.allocate_kernel_object(HeapObject::Type(TypeObject {
            name: "type".to_owned(),
            qualified_name: "type".to_owned(),
            metaclass: RValue::NONE,
            bases: vec![object_type].into_boxed_slice(),
            mro: Box::new([]),
            namespace: RValue::NONE,
            slot_names: Box::new([]),
            has_dictionary: false,
            has_weakref: false,
            flags: TYPE_FLAG_BUILTIN,
            version_tag: 0,
            layout: TypeLayout::Object,
        }))?;
        if let Some(HeapObject::Type(object)) = self.heap.get_mut(object_type) {
            object.mro = vec![object_type].into_boxed_slice();
            object.metaclass = type_type;
        }
        if let Some(HeapObject::Type(object)) = self.heap.get_mut(type_type) {
            object.mro = vec![type_type, object_type].into_boxed_slice();
            object.metaclass = type_type;
        }

        let mut types = BTreeMap::from([("object", object_type), ("type", type_type)]);
        for (name, base) in [
            ("BaseException", "object"),
            ("Exception", "BaseException"),
            ("BaseExceptionGroup", "BaseException"),
            ("ExceptionGroup", "Exception"),
            ("TypeError", "Exception"),
            ("ValueError", "Exception"),
            ("RuntimeError", "Exception"),
            ("NameError", "Exception"),
            ("AttributeError", "Exception"),
            ("UnboundLocalError", "NameError"),
            ("ZeroDivisionError", "Exception"),
            ("IndexError", "Exception"),
            ("MemoryError", "Exception"),
        ] {
            let base_value = types[base];
            let mut mro = vec![RValue::NONE];
            let Some(HeapObject::Type(base_type)) = self.heap.get(base_value) else {
                return Err("kernel type hierarchy is corrupt".to_owned());
            };
            mro.extend_from_slice(&base_type.mro);
            let value = self.allocate_kernel_object(HeapObject::Type(TypeObject {
                name: name.to_owned(),
                qualified_name: name.to_owned(),
                metaclass: type_type,
                bases: vec![base_value].into_boxed_slice(),
                mro: mro.into_boxed_slice(),
                namespace: RValue::NONE,
                slot_names: Box::new([]),
                has_dictionary: false,
                has_weakref: false,
                flags: TYPE_FLAG_BUILTIN | TYPE_FLAG_EXCEPTION,
                version_tag: 0,
                layout: TypeLayout::Object,
            }))?;
            let Some(HeapObject::Type(object)) = self.heap.get_mut(value) else {
                return Err("new kernel type is missing".to_owned());
            };
            object.mro[0] = value;
            types.insert(name, value);
        }

        let globals = self.allocate_kernel_object(HeapObject::Dictionary(DictionaryObject {
            entries: Vec::new(),
        }))?;
        let builtins = self.allocate_kernel_object(HeapObject::Dictionary(DictionaryObject {
            entries: Vec::new(),
        }))?;
        let module_cache =
            self.allocate_kernel_object(HeapObject::Dictionary(DictionaryObject {
                entries: Vec::new(),
            }))?;
        self.kernel = Some(KernelRoots {
            types,
            globals,
            builtins,
            module_cache,
            emergency_memory_error: RValue::NONE,
        });
        self.ensure_builtin_type("NotImplementedType")?;
        let not_implemented = self.allocate_kernel_object(HeapObject::NotImplemented)?;
        self.publish_builtin("NotImplemented", not_implemented)?;
        let memory_error_type = self
            .builtin_type("MemoryError")
            .ok_or_else(|| "MemoryError type is missing from the kernel".to_owned())?;
        let message = self.allocate(HeapObject::String(HEAP_LIMIT_MESSAGE.to_owned()))?;
        let emergency_memory_error = self.new_exception(memory_error_type, &[message])?;
        self.context_roots.push(emergency_memory_error);
        self.kernel
            .as_mut()
            .expect("kernel was initialized above")
            .emergency_memory_error = emergency_memory_error;
        self.initialize_sys_module()?;
        self.initialize_builtins_module()?;
        self.ensure_builtin("__import__")?;
        Ok(())
    }

    fn allocate_kernel_object(&mut self, object: HeapObject) -> Result<RValue, String> {
        let value = self.allocate(object)?;
        self.context_roots.push(value);
        Ok(value)
    }

    pub(crate) fn builtin_type(&self, name: &str) -> Option<RValue> {
        self.kernel.as_ref()?.types.get(name).copied()
    }

    /// Materializes runtime type metadata on first use. Created types are
    /// kernel roots with stable identity; Python builtin-name publication is
    /// owned separately by `ensure_builtin` so internal types do not leak into
    /// the builtin namespace.
    pub(crate) fn ensure_builtin_type(&mut self, name: &str) -> Result<Option<RValue>, String> {
        self.initialize_kernel()?;
        if let Some(value) = self.builtin_type(name) {
            return Ok(Some(value));
        }
        let lazy_type = LAZY_BUILTIN_TYPES
            .iter()
            .copied()
            .find(|(type_name, _)| *type_name == name);
        let lazy_exception = LAZY_BUILTIN_EXCEPTIONS
            .iter()
            .copied()
            .find(|(type_name, _)| *type_name == name);
        let Some((type_name, base_name)) = lazy_type.or(lazy_exception) else {
            return Ok(None);
        };
        let is_exception = lazy_exception.is_some();
        let base = self
            .ensure_builtin_type(base_name)?
            .ok_or_else(|| format!("kernel base type `{base_name}` is missing"))?;
        let base_mro = match self.heap.get(base) {
            Some(HeapObject::Type(base_type)) => base_type.mro.to_vec(),
            _ => return Err("kernel type hierarchy is corrupt".to_owned()),
        };
        let mut mro = Vec::with_capacity(base_mro.len() + 1);
        mro.push(RValue::NONE);
        mro.extend(base_mro);
        let value = self.allocate_kernel_object(HeapObject::Type(TypeObject {
            name: type_name.to_owned(),
            qualified_name: type_name.to_owned(),
            metaclass: self
                .builtin_type("type")
                .ok_or_else(|| "type is missing from the kernel".to_owned())?,
            bases: vec![base].into_boxed_slice(),
            mro: mro.into_boxed_slice(),
            namespace: RValue::NONE,
            slot_names: Box::new([]),
            has_dictionary: matches!(
                type_name,
                "SourceFileLoader" | "NamespaceLoader" | "ModuleSpec"
            ),
            has_weakref: false,
            flags: TYPE_FLAG_BUILTIN
                | if is_exception { TYPE_FLAG_EXCEPTION } else { 0 }
                | if matches!(
                    type_name,
                    "SourceFileLoader" | "NamespaceLoader" | "ModuleSpec"
                ) {
                    TYPE_FLAG_INSTANTIABLE
                } else {
                    0
                },
            version_tag: 0,
            layout: match type_name {
                "list" => TypeLayout::List,
                "tuple" => TypeLayout::Tuple,
                "dict" => TypeLayout::Dictionary,
                "set" => TypeLayout::Set,
                "float" => TypeLayout::Float,
                "complex" => TypeLayout::Complex,
                "bytes" => TypeLayout::Bytes,
                "bytearray" => TypeLayout::ByteArray,
                "frozenset" => TypeLayout::FrozenSet,
                _ => TypeLayout::Object,
            },
        }))?;
        let Some(HeapObject::Type(type_object)) = self.heap.get_mut(value) else {
            return Err("new kernel type is missing".to_owned());
        };
        type_object.mro[0] = value;
        self.kernel
            .as_mut()
            .expect("kernel was initialized above")
            .types
            .insert(type_name, value);
        Ok(Some(value))
    }

    /// Returns a lazily published builtin type or native builtin function.
    pub(crate) fn ensure_builtin(&mut self, name: &str) -> Result<Option<RValue>, String> {
        self.initialize_kernel()?;
        if let Some(value) = self.lookup_builtin(name) {
            return Ok(Some(value));
        }
        if is_public_builtin_type_name(name) {
            let value = self
                .ensure_builtin_type(name)?
                .ok_or_else(|| format!("builtin type `{name}` is missing from the kernel"))?;
            self.publish_builtin(name, value)?;
            return Ok(Some(value));
        }
        match name {
            "__debug__" => {
                let value = RValue::boolean(true);
                self.publish_builtin(name, value)?;
                return Ok(Some(value));
            }
            "Ellipsis" => {
                self.ensure_builtin_type("ellipsis")?
                    .ok_or_else(|| "ellipsis type is missing from the kernel".to_owned())?;
                let value = self.allocate_kernel_object(HeapObject::Ellipsis)?;
                self.publish_builtin(name, value)?;
                return Ok(Some(value));
            }
            _ => {}
        }
        let kind = match name {
            "abs" => BuiltinFunctionKind::Abs,
            "all" => BuiltinFunctionKind::All,
            "any" => BuiltinFunctionKind::Any,
            "len" => BuiltinFunctionKind::Len,
            "print" => BuiltinFunctionKind::Print,
            "round" => BuiltinFunctionKind::Round,
            "isinstance" => BuiltinFunctionKind::IsInstance,
            "issubclass" => BuiltinFunctionKind::IsSubclass,
            "iter" => BuiltinFunctionKind::Iter,
            "next" => BuiltinFunctionKind::Next,
            "aiter" => BuiltinFunctionKind::AIter,
            "anext" => BuiltinFunctionKind::ANext,
            "compile" if self.dynamic_compilation_enabled() => BuiltinFunctionKind::Compile,
            "eval" if self.dynamic_compilation_enabled() => BuiltinFunctionKind::Eval,
            "exec" if self.dynamic_compilation_enabled() => BuiltinFunctionKind::Exec,
            "property" => BuiltinFunctionKind::Property,
            "staticmethod" => BuiltinFunctionKind::StaticMethod,
            "classmethod" => BuiltinFunctionKind::ClassMethod,
            "getattr" => BuiltinFunctionKind::GetAttr,
            "setattr" => BuiltinFunctionKind::SetAttr,
            "delattr" => BuiltinFunctionKind::DelAttr,
            "hasattr" => BuiltinFunctionKind::HasAttr,
            "callable" => BuiltinFunctionKind::Callable,
            "hash" => BuiltinFunctionKind::Hash,
            "repr" => BuiltinFunctionKind::Repr,
            "format" => BuiltinFunctionKind::Format,
            "reversed" => BuiltinFunctionKind::Reversed,
            "bin" => BuiltinFunctionKind::Bin,
            "hex" => BuiltinFunctionKind::Hex,
            "oct" => BuiltinFunctionKind::Oct,
            "chr" => BuiltinFunctionKind::Chr,
            "ord" => BuiltinFunctionKind::Ord,
            "divmod" => BuiltinFunctionKind::DivMod,
            "pow" => BuiltinFunctionKind::Pow,
            "sum" => BuiltinFunctionKind::Sum,
            "min" => BuiltinFunctionKind::Min,
            "max" => BuiltinFunctionKind::Max,
            "enumerate" => BuiltinFunctionKind::Enumerate,
            "zip" => BuiltinFunctionKind::Zip,
            "map" => BuiltinFunctionKind::Map,
            "filter" => BuiltinFunctionKind::Filter,
            "sorted" => BuiltinFunctionKind::Sorted,
            "id" => BuiltinFunctionKind::Id,
            "ascii" => BuiltinFunctionKind::Ascii,
            "dir" => BuiltinFunctionKind::Dir,
            "vars" => BuiltinFunctionKind::Vars,
            "globals" => BuiltinFunctionKind::Globals,
            "locals" => BuiltinFunctionKind::Locals,
            "__import__" => BuiltinFunctionKind::Import,
            _ => return Ok(None),
        };
        let value =
            self.allocate_kernel_object(HeapObject::BuiltinFunction(BuiltinFunctionObject {
                name: name.to_owned(),
                kind,
            }))?;
        self.publish_builtin(name, value)?;
        Ok(Some(value))
    }

    pub(crate) fn lookup_builtin(&self, name: &str) -> Option<RValue> {
        let builtins = self.builtins()?;
        match self.heap.get(builtins) {
            Some(HeapObject::Dictionary(dictionary)) => dictionary.get(name),
            _ => None,
        }
    }

    fn publish_builtin(&mut self, name: &str, value: RValue) -> Result<(), String> {
        let builtins = self
            .builtins()
            .ok_or_else(|| "builtins dictionary is missing from the kernel".to_owned())?;
        match self.heap.get_mut(builtins) {
            Some(HeapObject::Dictionary(dictionary)) => {
                dictionary.insert(name.to_owned(), value);
                Ok(())
            }
            _ => Err("kernel builtins object is corrupt".to_owned()),
        }
    }

    pub(crate) fn type_of(&mut self, value: RValue) -> Result<RValue, String> {
        self.initialize_kernel()?;
        let name = match value.tag {
            tag if tag == RTag::None as u32 => "NoneType",
            tag if tag == RTag::Bool as u32 => "bool",
            tag if tag == RTag::SmallInt as u32 => "int",
            tag if tag == RTag::Handle as u32 => match self.heap.get(value) {
                Some(HeapObject::NotImplemented) => "NotImplementedType",
                Some(HeapObject::Ellipsis) => "ellipsis",
                Some(HeapObject::Float(_)) => "float",
                Some(HeapObject::Complex { .. }) => "complex",
                Some(HeapObject::BigInt(_)) => "int",
                Some(HeapObject::String(_)) => "str",
                Some(HeapObject::Bytes(_)) => "bytes",
                Some(HeapObject::ByteArray(_)) => "bytearray",
                Some(HeapObject::Slice(_)) => "slice",
                Some(HeapObject::Tuple(_)) => "tuple",
                Some(HeapObject::List(_)) => "list",
                Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_)) => "dict",
                Some(HeapObject::Set(_)) => "set",
                Some(HeapObject::FrozenSet(_)) => "frozenset",
                Some(HeapObject::DictionaryView(view)) => match view.kind {
                    crate::object::DictionaryViewKind::Keys => "dict_keys",
                    crate::object::DictionaryViewKind::Values => "dict_values",
                    crate::object::DictionaryViewKind::Items => "dict_items",
                },
                Some(HeapObject::MappingProxy(_)) => "mappingproxy",
                Some(HeapObject::MemoryView(_)) => "memoryview",
                Some(HeapObject::Module(_)) => "module",
                Some(HeapObject::TypeParameter(parameter)) => match parameter.kind {
                    rimera_abi::RTypeParameterKind::TypeVar => "TypeVar",
                    rimera_abi::RTypeParameterKind::TypeVarTuple => "TypeVarTuple",
                    rimera_abi::RTypeParameterKind::ParamSpec => "ParamSpec",
                },
                Some(HeapObject::TypeAlias(_)) => "TypeAliasType",
                Some(HeapObject::Range(_)) => "range",
                Some(HeapObject::Iterator(_)) => "iterator",
                Some(HeapObject::Function(_)) => "function",
                Some(HeapObject::Code(_)) => "code",
                Some(HeapObject::Generator(generator)) => match generator.kind {
                    SuspendedKind::Generator => "generator",
                    SuspendedKind::Coroutine => "coroutine",
                    SuspendedKind::AsyncGenerator => "async_generator",
                },
                Some(HeapObject::AsyncNextAwaitable(_)) => "anext_awaitable",
                Some(HeapObject::AsyncGeneratorOperation(operation)) => match operation.kind {
                    crate::object::AsyncGeneratorOperationKind::Next
                    | crate::object::AsyncGeneratorOperationKind::Send => "async_generator_asend",
                    crate::object::AsyncGeneratorOperationKind::Throw
                    | crate::object::AsyncGeneratorOperationKind::Close => "async_generator_athrow",
                },
                Some(HeapObject::BoundMethod(_)) => "function",
                Some(HeapObject::CallArguments(_) | HeapObject::BufferLease(_)) => "object",
                Some(HeapObject::Property(_))
                | Some(HeapObject::StaticMethod(_))
                | Some(HeapObject::ClassMethod(_)) => "object",
                Some(HeapObject::PropertyMethod(_)) => "builtin_function_or_method",
                Some(HeapObject::MemberDescriptor(_)) => "object",
                Some(HeapObject::Super(_)) => "super",
                Some(HeapObject::BuiltinFunction(_)) => "builtin_function_or_method",
                Some(HeapObject::Type(type_object)) => return Ok(type_object.metaclass),
                Some(HeapObject::Instance(instance)) => return Ok(instance.class),
                Some(HeapObject::Exception(exception)) => return Ok(exception.exception_type),
                Some(HeapObject::Cell(_)) => "cell",
                Some(HeapObject::Traceback(_)) => "traceback",
                Some(HeapObject::Frame(_)) => "frame",
                Some(HeapObject::ValueArray(_)) => "object",
                None => return Err("value contains a stale heap handle".to_owned()),
            },
            _ => return Err("value has an unknown ABI tag".to_owned()),
        };
        self.ensure_builtin_type(name)?
            .ok_or_else(|| format!("kernel type `{name}` is missing"))
    }

    pub(crate) fn is_instance(
        &mut self,
        value: RValue,
        class_info: RValue,
    ) -> Result<bool, String> {
        let actual_type = self.type_of(value)?;
        self.is_subclass_of_class_info(actual_type, class_info, "isinstance")
    }

    /// Python-visible `isinstance` semantics. Internal runtime subtype checks
    /// deliberately keep using the structural helpers so user metaclass hooks
    /// cannot perturb exception matching, numeric dispatch, or class creation.
    pub(crate) fn is_instance_reflective(
        &mut self,
        value: RValue,
        class_info: RValue,
    ) -> Result<bool, String> {
        if let Some(HeapObject::Tuple(items)) = self.heap.get(class_info) {
            let items = items.to_vec();
            for item in items {
                if self.is_instance_reflective(value, item)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let Some(HeapObject::Type(_)) = self.heap.get(class_info) else {
            return Err(
                "isinstance() arg 2 must be a type, a tuple of types, or a union".to_owned(),
            );
        };
        let actual_type = self.type_of(value)?;
        // CPython bypasses a custom __instancecheck__ for exact instances.
        if actual_type == class_info {
            return Ok(true);
        }
        if let Some(method) = self.special_method(class_info, "__instancecheck__")? {
            let result = self.with_temporary_roots(&[value, class_info, method], |context| {
                crate::call::invoke(context, method, &[value], &[])
            })?;
            return self.with_temporary_roots(&[value, class_info, result], |context| {
                crate::operations::truthy(context, result)
            });
        }
        self.is_subclass(actual_type, class_info)
    }

    /// Python-visible `issubclass` semantics including custom metaclass
    /// `__subclasscheck__`, kept separate from structural runtime subtyping.
    pub(crate) fn is_subclass_reflective(
        &mut self,
        candidate: RValue,
        class_info: RValue,
    ) -> Result<bool, String> {
        if !matches!(self.heap.get(candidate), Some(HeapObject::Type(_))) {
            return Err("issubclass() arg 1 must be a class".to_owned());
        }
        if let Some(HeapObject::Tuple(items)) = self.heap.get(class_info) {
            let items = items.to_vec();
            for item in items {
                if self.is_subclass_reflective(candidate, item)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let Some(HeapObject::Type(_)) = self.heap.get(class_info) else {
            return Err(
                "issubclass() arg 2 must be a class, a tuple of classes, or a union".to_owned(),
            );
        };
        if let Some(method) = self.special_method(class_info, "__subclasscheck__")? {
            let result = self
                .with_temporary_roots(&[candidate, class_info, method], |context| {
                    crate::call::invoke(context, method, &[candidate], &[])
                })?;
            return self.with_temporary_roots(&[candidate, class_info, result], |context| {
                crate::operations::truthy(context, result)
            });
        }
        self.is_subclass(candidate, class_info)
    }

    pub(crate) fn is_subclass(
        &self,
        candidate: RValue,
        class_info: RValue,
    ) -> Result<bool, String> {
        if !matches!(self.heap.get(candidate), Some(HeapObject::Type(_))) {
            return Err("issubclass() arg 1 must be a class".to_owned());
        }
        self.is_subclass_of_class_info(candidate, class_info, "issubclass")
    }

    fn is_subclass_of_class_info(
        &self,
        candidate: RValue,
        class_info: RValue,
        builtin: &str,
    ) -> Result<bool, String> {
        if let Some(HeapObject::Tuple(items)) = self.heap.get(class_info) {
            for item in items.iter().copied() {
                if self.is_subclass_of_class_info(candidate, item, builtin)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let Some(HeapObject::Type(_)) = self.heap.get(class_info) else {
            return Err(match builtin {
                "isinstance" => {
                    "isinstance() arg 2 must be a type, a tuple of types, or a union".to_owned()
                }
                "issubclass" => {
                    "issubclass() arg 2 must be a class, a tuple of classes, or a union".to_owned()
                }
                _ => "invalid class information".to_owned(),
            });
        };
        let Some(HeapObject::Type(actual)) = self.heap.get(candidate) else {
            return Err("issubclass() arg 1 must be a class".to_owned());
        };
        Ok(actual.mro.contains(&class_info))
    }

    pub(crate) fn new_class(
        &mut self,
        name: &str,
        bases: &[RValue],
        namespace: RValue,
    ) -> Result<RValue, String> {
        self.new_class_with_metaclass(name, bases, namespace, None)
    }

    pub(crate) fn invoke_metaclass_constructor(
        &mut self,
        metaclass: RValue,
        name_value: RValue,
        bases_value: RValue,
        namespace: RValue,
        keywords: &[(String, RValue)],
    ) -> Result<RValue, String> {
        let name = match self.heap.get(name_value) {
            Some(HeapObject::String(name)) => name.clone(),
            _ => return Err("type.__new__() argument 1 must be str".to_owned()),
        };
        let bases = match self.heap.get(bases_value) {
            Some(HeapObject::Tuple(bases)) => bases.to_vec(),
            _ => return Err("type.__new__() argument 2 must be tuple".to_owned()),
        };
        let mut roots = vec![metaclass, name_value, bases_value, namespace];
        roots.extend(keywords.iter().map(|(_, value)| *value));
        self.with_temporary_roots(&roots, |context| {
            let created = if let Some(new_hook) = context.class_attribute(metaclass, "__new__") {
                let callable = context.descriptor_get(new_hook, Some(metaclass), metaclass)?;
                context.with_temporary_roots(&[callable], |context| {
                    crate::call::invoke(
                        context,
                        callable,
                        &[name_value, bases_value, namespace],
                        keywords,
                    )
                })?
            } else {
                context.new_class_with_metaclass(&name, &bases, namespace, Some(metaclass))?
            };

            context.with_temporary_roots(&[created], |context| {
                let created_type = context.type_of(created)?;
                if !context.is_subclass(created_type, metaclass)? {
                    return Ok(created);
                }
                let Some(init_hook) = context.class_attribute(metaclass, "__init__") else {
                    return Ok(created);
                };
                let callable = context.descriptor_get(init_hook, Some(created), metaclass)?;
                context.with_temporary_roots(&[callable], |context| {
                    crate::call::invoke(
                        context,
                        callable,
                        &[name_value, bases_value, namespace],
                        keywords,
                    )
                    .map(|_| created)
                })
            })
        })
    }

    pub(crate) fn new_class_with_metaclass(
        &mut self,
        name: &str,
        bases: &[RValue],
        namespace: RValue,
        requested_metaclass: Option<RValue>,
    ) -> Result<RValue, String> {
        self.initialize_kernel()?;
        let type_metaclass = self
            .builtin_type("type")
            .ok_or_else(|| "type is missing from the kernel".to_owned())?;
        // CPython's default `type.__new__` requires a dictionary. A custom
        // metaclass may receive an arbitrary mapping from `__prepare__` and
        // convert it before delegating here.
        if !matches!(
            self.heap.get(namespace),
            Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_))
        ) {
            return Err("type.__new__() argument 3 must be dict".to_owned());
        }
        if let Some(HeapObject::ValueDictionary(dictionary)) = self.heap.get(namespace)
            && dictionary
                .table
                .values()
                .any(|(key, _)| !matches!(self.heap.get(*key), Some(HeapObject::String(_))))
        {
            return Err("attribute name must be string".to_owned());
        }
        let bases = self.resolve_class_bases(bases, namespace)?;
        let object = self
            .builtin_type("object")
            .ok_or_else(|| "object type is missing from the kernel".to_owned())?;
        let effective_bases = if bases.is_empty() {
            vec![object]
        } else {
            bases
        };
        let mut metaclass = requested_metaclass.unwrap_or(type_metaclass);
        if !matches!(self.heap.get(metaclass), Some(HeapObject::Type(_))) {
            return Err("metaclass is not a type".to_owned());
        }
        for (index, base) in effective_bases.iter().copied().enumerate() {
            let Some(HeapObject::Type(base_type)) = self.heap.get(base) else {
                return Err("bases must be types".to_owned());
            };
            if base != object
                && base != type_metaclass
                && base_type.flags & TYPE_FLAG_BUILTIN != 0
                && base_type.flags & TYPE_FLAG_EXCEPTION == 0
                && !matches!(
                    base_type.name.as_str(),
                    "list"
                        | "tuple"
                        | "dict"
                        | "set"
                        | "float"
                        | "complex"
                        | "bytes"
                        | "bytearray"
                        | "frozenset"
                )
            {
                return Err(format!(
                    "type '{}' is not an acceptable base type",
                    base_type.name
                ));
            }
            if effective_bases[..index].contains(&base) {
                return Err(format!("duplicate base class {}", base_type.name));
            }
            let base_metaclass = base_type.metaclass;
            if self.is_subclass(metaclass, base_metaclass)? {
                continue;
            }
            if requested_metaclass.is_none() && self.is_subclass(base_metaclass, metaclass)? {
                metaclass = base_metaclass;
                continue;
            }
            return Err("metaclass conflict: the metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all its bases".to_owned());
        }
        let base_mro = self.compute_c3_mro(&effective_bases)?;
        if self.namespace_value(namespace, "__module__").is_none() {
            let module_name = crate::operations::string(self, "__main__")?;
            self.namespace_set(namespace, "__module__", module_name)?;
        }
        let (own_slots, declares_dictionary, declares_weakref) = self.class_slot_spec(namespace)?;
        let mut inherited_slots = Vec::new();
        let mut has_dictionary = false;
        let mut has_weakref = false;
        for base in &effective_bases {
            let Some(HeapObject::Type(base_type)) = self.heap.get(*base) else {
                return Err("bases must be types".to_owned());
            };
            for slot in base_type.slot_names.iter() {
                if !inherited_slots.contains(slot) {
                    inherited_slots.push(slot.clone());
                }
            }
            has_dictionary |= base_type.has_dictionary;
            has_weakref |= base_type.has_weakref;
        }
        for slot in &own_slots {
            if inherited_slots.contains(slot) {
                return Err(format!("__slots__ conflicts with class variable '{slot}'"));
            }
        }
        let own_slot_offset = inherited_slots.len();
        inherited_slots.extend(own_slots.iter().cloned());
        has_dictionary |=
            declares_dictionary || self.namespace_value(namespace, "__slots__").is_none();
        has_weakref |= declares_weakref;
        let effective_bases_for_hooks = effective_bases.clone();
        let layout = inherited_layout(&self.heap, &effective_bases)?;
        if self.namespace_value(namespace, "__eq__").is_some()
            && self.namespace_value(namespace, "__hash__").is_none()
        {
            // Python disables inherited identity hashing when a class provides
            // equality without opting into a compatible hash implementation.
            self.namespace_set(namespace, "__hash__", RValue::NONE)?;
        }
        let qualified_name = self
            .namespace_value(namespace, "__qualname__")
            .and_then(|value| match self.heap.get(value) {
                Some(HeapObject::String(value)) => Some(value.clone()),
                _ => None,
            })
            .unwrap_or_else(|| name.to_owned());
        let mut roots = effective_bases.clone();
        roots.extend([object, namespace, metaclass]);
        self.with_temporary_roots(&roots, |context| {
            let mut mro = Vec::with_capacity(base_mro.len() + 1);
            mro.push(RValue::NONE);
            mro.extend(base_mro);
            let value = context.allocate(HeapObject::Type(TypeObject {
                name: name.to_owned(),
                qualified_name: qualified_name.clone(),
                metaclass,
                bases: effective_bases.into_boxed_slice(),
                mro: mro.into_boxed_slice(),
                namespace,
                slot_names: inherited_slots.into_boxed_slice(),
                has_dictionary,
                has_weakref,
                flags: TYPE_FLAG_INSTANTIABLE,
                version_tag: 0,
                layout,
            }))?;
            let Some(HeapObject::Type(type_object)) = context.heap.get_mut(value) else {
                return Err("new class is missing from the heap".to_owned());
            };
            type_object.mro[0] = value;
            for (offset, slot) in own_slots.iter().enumerate() {
                let descriptor =
                    context.allocate(HeapObject::MemberDescriptor(MemberDescriptorObject {
                        owner: value,
                        index: own_slot_offset + offset,
                        name: slot.clone(),
                    }))?;
                context.namespace_set(namespace, slot, descriptor)?;
            }
            context.invoke_set_name_hooks(value, namespace)?;
            context.invoke_init_subclass_hooks(value, &effective_bases_for_hooks)?;
            Ok(value)
        })
    }

    fn resolve_class_bases(
        &mut self,
        bases: &[RValue],
        namespace: RValue,
    ) -> Result<Vec<RValue>, String> {
        if bases
            .iter()
            .all(|base| matches!(self.heap.get(*base), Some(HeapObject::Type(_))))
        {
            return Ok(bases.to_vec());
        }
        let original = self.with_temporary_roots(bases, |context| {
            context.allocate(HeapObject::Tuple(bases.to_vec().into_boxed_slice()))
        })?;
        let mut roots = bases.to_vec();
        roots.extend([original, namespace]);
        self.with_temporary_roots(&roots, |context| {
            let mut resolved = Vec::new();
            for base in bases.iter().copied() {
                if matches!(context.heap.get(base), Some(HeapObject::Type(_))) {
                    resolved.push(base);
                    continue;
                }
                let hook = context
                    .special_method(base, "__mro_entries__")?
                    .ok_or_else(|| "bases must be types".to_owned())?;
                let replacement = context
                    .with_temporary_roots(&[base, hook, original], |context| {
                        crate::call::invoke(context, hook, &[original], &[])
                    })?;
                let Some(HeapObject::Tuple(entries)) = context.heap.get(replacement) else {
                    return Err("__mro_entries__ must return a tuple".to_owned());
                };
                let entries = entries.to_vec();
                if entries
                    .iter()
                    .any(|entry| !matches!(context.heap.get(*entry), Some(HeapObject::Type(_))))
                {
                    return Err("__mro_entries__ returned a non-type base".to_owned());
                }
                resolved.extend(entries);
            }
            context.namespace_set(namespace, "__orig_bases__", original)?;
            Ok(resolved)
        })
    }

    fn compute_c3_mro(&self, bases: &[RValue]) -> Result<Vec<RValue>, String> {
        let mut sequences = Vec::with_capacity(bases.len() + 1);
        for base in bases {
            let Some(HeapObject::Type(base_type)) = self.heap.get(*base) else {
                return Err("bases must be types".to_owned());
            };
            sequences.push(base_type.mro.to_vec());
        }
        sequences.push(bases.to_vec());

        let mut result = Vec::new();
        loop {
            sequences.retain(|sequence| !sequence.is_empty());
            if sequences.is_empty() {
                return Ok(result);
            }
            let candidate = sequences
                .iter()
                .filter_map(|sequence| sequence.first())
                .find(|candidate| {
                    !sequences
                        .iter()
                        .any(|sequence| sequence.iter().skip(1).any(|item| item == *candidate))
                });
            let Some(candidate) = candidate.copied() else {
                let mut conflicts = Vec::new();
                for head in sequences.iter().filter_map(|sequence| sequence.first()) {
                    if !conflicts.contains(head) {
                        conflicts.push(*head);
                    }
                }
                let names = conflicts
                    .iter()
                    .map(|base| self.type_name(*base))
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(format!(
                    "Cannot create a consistent method resolution\norder (MRO) for bases {names}"
                ));
            };
            result.push(candidate);
            for sequence in &mut sequences {
                if sequence.first() == Some(&candidate) {
                    sequence.remove(0);
                }
            }
        }
    }

    fn compute_c3_mro_planned(
        &self,
        bases: &[RValue],
        planned: &[(RValue, Vec<RValue>)],
    ) -> Result<Vec<RValue>, String> {
        let mut sequences = Vec::with_capacity(bases.len() + 1);
        for base in bases {
            let mro = planned
                .iter()
                .find_map(|(value, mro)| (*value == *base).then_some(mro.clone()))
                .or_else(|| match self.heap.get(*base) {
                    Some(HeapObject::Type(object)) => Some(object.mro.to_vec()),
                    _ => None,
                })
                .ok_or_else(|| "bases must be types".to_owned())?;
            sequences.push(mro);
        }
        sequences.push(bases.to_vec());
        let mut result = Vec::new();
        loop {
            sequences.retain(|sequence| !sequence.is_empty());
            if sequences.is_empty() {
                return Ok(result);
            }
            let candidate = sequences
                .iter()
                .filter_map(|sequence| sequence.first())
                .find(|candidate| {
                    !sequences
                        .iter()
                        .any(|sequence| sequence.iter().skip(1).any(|item| item == *candidate))
                })
                .copied()
                .ok_or_else(|| {
                    "Cannot create a consistent method resolution order (MRO)".to_owned()
                })?;
            result.push(candidate);
            for sequence in &mut sequences {
                if sequence.first() == Some(&candidate) {
                    sequence.remove(0);
                }
            }
        }
    }

    fn invoke_set_name_hooks(&mut self, class: RValue, namespace: RValue) -> Result<(), String> {
        let entries = match self.heap.get(namespace) {
            Some(HeapObject::Dictionary(dictionary)) => dictionary.entries.clone(),
            Some(HeapObject::ValueDictionary(dictionary)) => dictionary
                .table
                .values()
                .filter_map(|(key, value)| match self.heap.get(*key) {
                    Some(HeapObject::String(name)) => Some((name.clone(), *value)),
                    _ => None,
                })
                .collect(),
            _ => return Ok(()),
        };
        let mut roots = vec![class, namespace];
        roots.extend(entries.iter().map(|(_, value)| *value));
        self.with_temporary_roots(&roots, |context| {
            for (name, value) in entries {
                let Some(hook) = context.special_method(value, "__set_name__")? else {
                    continue;
                };
                let name_value = crate::operations::string(context, &name)?;
                context.with_temporary_roots(&[class, value, hook, name_value], |context| {
                    crate::call::invoke(context, hook, &[class, name_value], &[]).map(|_| ())
                })?;
            }
            Ok(())
        })
    }

    fn invoke_init_subclass_hooks(
        &mut self,
        class: RValue,
        bases: &[RValue],
    ) -> Result<(), String> {
        self.with_temporary_roots(bases, |context| {
            for base in bases.iter().copied() {
                let Some(hook) = context.class_attribute(base, "__init_subclass__") else {
                    continue;
                };
                let hook = context.descriptor_get(hook, None, base)?;
                context.with_temporary_roots(&[class, base, hook], |context| {
                    crate::call::invoke(context, hook, &[class], &[]).map(|_| ())
                })?;
            }
            Ok(())
        })
    }

    /// Extracts the permanent slot layout declaration from a class namespace.
    /// The layout is intentionally resolved before the type is published so a
    /// failed declaration cannot leave a partially constructed class behind.
    fn class_slot_spec(&self, namespace: RValue) -> Result<(Vec<String>, bool, bool), String> {
        let Some(value) = self.namespace_value(namespace, "__slots__") else {
            return Ok((Vec::new(), false, false));
        };
        let names_from_values = |values: &[RValue]| {
            values
                .iter()
                .map(|value| match self.heap.get(*value) {
                    Some(HeapObject::String(name)) => Ok(name.clone()),
                    _ => Err("__slots__ items must be strings".to_owned()),
                })
                .collect::<Result<Vec<_>, _>>()
        };
        let mut names = match self.heap.get(value) {
            Some(HeapObject::String(name)) => vec![name.clone()],
            Some(HeapObject::Tuple(values)) => names_from_values(values)?,
            Some(HeapObject::List(values)) => names_from_values(values)?,
            _ => return Err("__slots__ must be a string or iterable of strings".to_owned()),
        };
        let mut has_dictionary = false;
        let mut has_weakref = false;
        names.retain(|name| match name.as_str() {
            "__dict__" => {
                has_dictionary = true;
                false
            }
            "__weakref__" => {
                has_weakref = true;
                false
            }
            _ => true,
        });
        for (index, name) in names.iter().enumerate() {
            if names[..index].contains(name) {
                return Err(format!("__slots__ has duplicate entry '{name}'"));
            }
        }
        Ok((names, has_dictionary, has_weakref))
    }

    pub(crate) fn new_super(
        &mut self,
        start_type: RValue,
        receiver: RValue,
    ) -> Result<RValue, String> {
        if !matches!(self.heap.get(start_type), Some(HeapObject::Type(_))) {
            return Err("super() argument 1 must be a type".to_owned());
        }
        let receiver_type = if matches!(self.heap.get(receiver), Some(HeapObject::Type(_))) {
            receiver
        } else {
            self.type_of(receiver)?
        };
        let valid = matches!(
            self.heap.get(receiver_type),
            Some(HeapObject::Type(receiver_type)) if receiver_type.mro.contains(&start_type)
        );
        if !valid {
            return Err("super(type, obj): obj must be an instance or subtype of type".to_owned());
        }
        self.with_temporary_roots(&[start_type, receiver, receiver_type], |context| {
            context.allocate(HeapObject::Super(SuperObject {
                start_type,
                receiver,
                receiver_type,
            }))
        })
    }

    pub(crate) fn new_instance(&mut self, class: RValue) -> Result<RValue, String> {
        let Some(HeapObject::Type(type_object)) = self.heap.get(class) else {
            return Err("object is not callable".to_owned());
        };
        if type_object.flags & TYPE_FLAG_INSTANTIABLE == 0 {
            return Err("object is not callable".to_owned());
        }
        let has_dictionary = type_object.has_dictionary;
        let slot_count = type_object.slot_names.len();
        let layout = type_object.layout;
        self.with_temporary_roots(&[class], |context| {
            let dictionary = if has_dictionary {
                Some(context.allocate(HeapObject::Dictionary(DictionaryObject {
                    entries: Vec::new(),
                }))?)
            } else {
                None
            };
            let roots = dictionary
                .into_iter()
                .chain(std::iter::once(class))
                .collect::<Vec<_>>();
            context.with_temporary_roots(&roots, |context| {
                let storage = match layout {
                    TypeLayout::List => Some(context.allocate(HeapObject::List(Vec::new()))?),
                    TypeLayout::Tuple => {
                        Some(context.allocate(HeapObject::Tuple(Vec::new().into_boxed_slice()))?)
                    }
                    TypeLayout::Dictionary => {
                        Some(context.allocate(HeapObject::ValueDictionary(Default::default()))?)
                    }
                    TypeLayout::Set => Some(
                        context.allocate(HeapObject::Set(crate::object::SetObject::default()))?,
                    ),
                    TypeLayout::Float => Some(context.allocate(HeapObject::Float(0.0))?),
                    TypeLayout::Complex => Some(context.allocate(HeapObject::Complex {
                        real: 0.0,
                        imag: 0.0,
                    })?),
                    TypeLayout::Bytes => Some(context.allocate(HeapObject::Bytes(Vec::new()))?),
                    TypeLayout::ByteArray => Some(context.allocate(HeapObject::ByteArray(
                        crate::object::ByteArrayObject {
                            bytes: Vec::new(),
                            exports: 0,
                        },
                    ))?),
                    TypeLayout::FrozenSet => Some(
                        context
                            .allocate(HeapObject::FrozenSet(crate::object::SetObject::default()))?,
                    ),
                    _ => None,
                };
                let storage_roots = storage.into_iter().collect::<Vec<_>>();
                context.with_temporary_roots(&storage_roots, |context| {
                    context.allocate(HeapObject::Instance(InstanceObject {
                        class,
                        dictionary,
                        slots: vec![None; slot_count].into_boxed_slice(),
                        storage,
                    }))
                })
            })
        })
    }

    pub(crate) fn initialize_instance_storage(
        &mut self,
        instance: RValue,
        positional: &[RValue],
        keywords: &[(String, RValue)],
    ) -> Result<Option<RValue>, String> {
        let storage = match self.heap.get(instance) {
            Some(HeapObject::Instance(object)) => object.storage,
            _ => return Err("object is not an instance".to_owned()),
        };
        let Some(storage) = storage else {
            return Ok(None);
        };
        let constructor = match self.heap.get(storage) {
            Some(HeapObject::List(_)) => "list",
            Some(HeapObject::Tuple(_)) => "tuple",
            Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_)) => "dict",
            Some(HeapObject::Set(_)) => "set",
            Some(HeapObject::FrozenSet(_)) => "frozenset",
            Some(HeapObject::Float(_)) => "float",
            Some(HeapObject::Complex { .. }) => "complex",
            Some(HeapObject::Bytes(_)) => "bytes",
            Some(HeapObject::ByteArray(_)) => "bytearray",
            _ => return Ok(Some(storage)),
        };
        let mut roots = vec![instance, storage];
        roots.extend_from_slice(positional);
        roots.extend(keywords.iter().map(|(_, value)| *value));
        self.with_temporary_roots(&roots, |context| {
            let replacement = crate::call::invoke_builtin_constructor(
                context,
                constructor,
                positional,
                keywords,
            )?;
            context.with_temporary_roots(&[replacement], |context| {
                context.replace_instance_storage(instance, replacement)?;
                Ok(Some(replacement))
            })
        })
    }

    fn replace_instance_storage(
        &mut self,
        instance: RValue,
        storage: RValue,
    ) -> Result<(), String> {
        let Some(HeapObject::Instance(object)) = self.heap.get_mut(instance) else {
            return Err("object is not an instance".to_owned());
        };
        object.storage = Some(storage);
        Ok(())
    }

    pub(crate) fn new_generator(
        &mut self,
        function: RValue,
        bound: &[RValue],
    ) -> Result<RValue, String> {
        self.new_suspended(function, bound, SuspendedKind::Generator)
    }

    pub(crate) fn new_coroutine(
        &mut self,
        function: RValue,
        bound: &[RValue],
    ) -> Result<RValue, String> {
        self.new_suspended(function, bound, SuspendedKind::Coroutine)
    }

    pub(crate) fn new_async_generator(
        &mut self,
        function: RValue,
        bound: &[RValue],
    ) -> Result<RValue, String> {
        self.new_suspended(function, bound, SuspendedKind::AsyncGenerator)
    }

    pub(crate) fn new_generator_rooted(
        &mut self,
        function: RValue,
        bound: &[RValue],
    ) -> Result<RValue, String> {
        self.new_suspended_rooted(function, bound, SuspendedKind::Generator)
    }

    /// Exact-call fast lane for a coroutine whose FunctionObject metadata was
    /// already read by the caller dispatcher. The callable/arguments are rooted
    /// by the generated caller, so avoid a second heap lookup on every creation.
    pub(crate) fn new_coroutine_known_rooted(
        &mut self,
        function: RValue,
        bound: &[RValue],
        resume_address: usize,
        persistent_slot_count: usize,
        first_line: u32,
    ) -> Result<RValue, String> {
        self.new_suspended_known_rooted(
            function,
            bound,
            SuspendedKind::Coroutine,
            resume_address,
            persistent_slot_count,
            first_line,
        )
    }

    pub(crate) fn new_async_generator_rooted(
        &mut self,
        function: RValue,
        bound: &[RValue],
    ) -> Result<RValue, String> {
        self.new_suspended_rooted(function, bound, SuspendedKind::AsyncGenerator)
    }

    fn new_suspended(
        &mut self,
        function: RValue,
        bound: &[RValue],
        kind: SuspendedKind,
    ) -> Result<RValue, String> {
        let mut roots = Vec::with_capacity(bound.len() + 1);
        roots.push(function);
        roots.extend_from_slice(bound);
        self.with_temporary_roots(&roots, |context| {
            context.new_suspended_rooted(function, bound, kind)
        })
    }

    fn new_suspended_rooted(
        &mut self,
        function: RValue,
        bound: &[RValue],
        kind: SuspendedKind,
    ) -> Result<RValue, String> {
        let (resume_address, persistent_slot_count, first_line) = match self.heap.get(function) {
            Some(HeapObject::Function(function)) => match (function.fast_call.kind(), kind) {
                (
                    FunctionKind::Generator {
                        persistent_slot_count,
                    },
                    SuspendedKind::Generator,
                )
                | (
                    FunctionKind::Coroutine {
                        persistent_slot_count,
                    },
                    SuspendedKind::Coroutine,
                )
                | (
                    FunctionKind::AsyncGenerator {
                        persistent_slot_count,
                    },
                    SuspendedKind::AsyncGenerator,
                ) => (
                    function.fast_call.code_address,
                    persistent_slot_count,
                    function.fast_call.first_line,
                ),
                _ => {
                    return Err(match kind {
                        SuspendedKind::Generator => "function is not a generator".to_owned(),
                        SuspendedKind::Coroutine => {
                            "function is not a coroutine function".to_owned()
                        }
                        SuspendedKind::AsyncGenerator => {
                            "function is not an async generator function".to_owned()
                        }
                    });
                }
            },
            _ => return Err("function is not a suspended function".to_owned()),
        };
        self.new_suspended_known_rooted(
            function,
            bound,
            kind,
            resume_address,
            persistent_slot_count,
            first_line,
        )
    }

    fn new_suspended_known_rooted(
        &mut self,
        function: RValue,
        bound: &[RValue],
        kind: SuspendedKind,
        resume_address: usize,
        persistent_slot_count: usize,
        first_line: u32,
    ) -> Result<RValue, String> {
        self.enable_generator_finalizer();
        let mut slots = vec![None; persistent_slot_count.max(bound.len())];
        for (slot, value) in slots.iter_mut().zip(bound.iter().copied()) {
            *slot = Some(value);
        }
        let suspended_object = HeapObject::Generator(GeneratorObject {
            kind,
            function,
            name_override: None,
            qualified_name_override: None,
            resume_address,
            state: 0,
            current_line: first_line,
            slots: slots.into_boxed_slice(),
            local_cells: Vec::new(),
            locals_snapshot: None,
            frame: None,
            delegate: None,
            handled: Box::new([]),
            raised: None,
            started: false,
            running: false,
            async_operation_active: false,
            closed: false,
            completed: false,
            finalizer_ran: false,
            return_value: None,
        });
        let suspended = if self.heap_limit_bytes.is_none() {
            // Coroutine/generator creation is a hot path. With no managed heap
            // ceiling to enforce, avoid rescanning the full heap for a fresh
            // suspended object whose initial managed size is already known.
            // Threshold collection refreshes exact accounting before GC.
            let size = suspended_object.managed_size();
            if self.heap.cached_live_bytes().saturating_add(size) > self.next_collection_bytes {
                self.collect();
            }
            self.heap.allocate_cached(suspended_object)
        } else {
            self.allocate(suspended_object)?
        };
        // Preserve the existing eager generator reflection contract. Native
        // coroutines take the lean path and only pay for a dict/frame when
        // Python actually observes `cr_frame`, `locals()`, traceback state,
        // or another reflection boundary.
        if kind == SuspendedKind::Generator {
            self.with_temporary_roots(&[suspended], |context| {
                context.ensure_suspended_frame(suspended)?;
                Ok(suspended)
            })
        } else {
            Ok(suspended)
        }
    }

    fn ensure_suspended_frame(&mut self, suspended: RValue) -> Result<RValue, String> {
        let (existing, function, current_line, local_cells, locals_snapshot, slots, terminal) =
            match self.heap.get(suspended) {
                Some(HeapObject::Generator(object)) => (
                    object.frame,
                    object.function,
                    object.current_line,
                    object.local_cells.clone(),
                    object.locals_snapshot,
                    object.slots.to_vec(),
                    object.completed || object.closed,
                ),
                _ => return Err("object is not a suspended native activation".to_owned()),
            };
        if let Some(frame) = existing {
            return Ok(frame);
        }
        if terminal {
            return Ok(RValue::NONE);
        }
        let (code, globals) = match self.heap.get(function) {
            Some(HeapObject::Function(function)) => (function.code, function.globals),
            _ => return Err("suspended function is invalid".to_owned()),
        };
        let parameter_names = match self.heap.get(code) {
            Some(HeapObject::Code(code)) => code
                .parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>(),
            _ => return Err("suspended function has an invalid code object".to_owned()),
        };
        let mut roots = vec![suspended, function, code, globals];
        roots.extend(slots.iter().flatten().copied());
        roots.extend(local_cells.iter().map(|(_, cell)| *cell));
        roots.extend(locals_snapshot);
        self.with_temporary_roots(&roots, |context| {
            let locals = if let Some(locals) = locals_snapshot {
                locals
            } else {
                context.allocate(HeapObject::Dictionary(DictionaryObject {
                    entries: Vec::new(),
                }))?
            };
            if locals_snapshot.is_none() {
                for (name, value) in parameter_names
                    .iter()
                    .zip(slots.iter().take(parameter_names.len()).copied().flatten())
                {
                    context.direct_namespace_set(locals, name, value);
                }
            }
            for (name, cell) in &local_cells {
                let value = match context.heap.get(*cell) {
                    Some(HeapObject::Cell(cell)) => cell.value,
                    _ => None,
                };
                match value {
                    Some(value) => context.direct_namespace_set(locals, name, value),
                    None => context.direct_namespace_delete(locals, name),
                }
            }
            context.enforce_heap_limit()?;
            let frame = context.allocate(HeapObject::Frame(Box::new(FrameObject {
                code,
                globals,
                locals,
                back: None,
                line: current_line,
                generator: Some(suspended),
            })))?;
            let Some(HeapObject::Generator(object)) = context.heap.get_mut(suspended) else {
                return Err("suspended activation disappeared while publishing frame".to_owned());
            };
            object.locals_snapshot = Some(locals);
            object.frame = Some(frame);
            Ok(frame)
        })
    }

    pub(crate) fn set_generator_frame_line(
        &mut self,
        generator: RValue,
        line: u32,
    ) -> Result<(), String> {
        let frame = match self.heap.get_mut(generator) {
            Some(HeapObject::Generator(generator)) => {
                generator.current_line = line;
                generator.frame
            }
            _ => return Err("object is not a generator".to_owned()),
        };
        if let Some(frame) = frame {
            let Some(HeapObject::Frame(frame)) = self.heap.get_mut(frame) else {
                return Err("generator frame is invalid".to_owned());
            };
            frame.line = line;
        }
        Ok(())
    }

    fn detach_generator_frame(&mut self, generator: RValue) -> Result<(), String> {
        let frame = match self.heap.get(generator) {
            Some(HeapObject::Generator(generator)) => generator.frame,
            _ => return Err("object is not a generator".to_owned()),
        };
        if let Some(frame) = frame {
            let Some(HeapObject::Frame(frame_object)) = self.heap.get_mut(frame) else {
                return Err("generator frame is invalid".to_owned());
            };
            frame_object.generator = None;
        }
        let Some(HeapObject::Generator(generator_object)) = self.heap.get_mut(generator) else {
            return Err("object is not a generator".to_owned());
        };
        generator_object.frame = None;
        Ok(())
    }

    pub(crate) fn resume_generator(
        &mut self,
        generator: RValue,
        operation: RGeneratorOperation,
        input: RValue,
    ) -> Result<GeneratorResume, String> {
        // Fresh close is the coroutine-creation benchmark's lifecycle path and
        // Python-visible `coro.close()` is required to be cheap. An unstarted
        // suspended object with no materialized frame has no body/cleanup to
        // execute, so close it in-place before resolving function globals or
        // constructing activation/exception bookkeeping. Materialized frames
        // and every started/terminal/error case retain the full path below.
        if matches!(operation, RGeneratorOperation::Close)
            && let Some(HeapObject::Generator(object)) = self.heap.get_mut(generator)
            && !object.started
            && !object.running
            && !object.closed
            && !object.completed
            && object.frame.is_none()
        {
            object.closed = true;
            object.completed = true;
            object.slots.fill(None);
            object.local_cells.clear();
            object.locals_snapshot = None;
            return Ok(GeneratorResume {
                value: RValue::NONE,
                outcome: RGeneratorOutcome::Returned,
            });
        }

        let (
            suspended_kind,
            function,
            globals,
            resume_address,
            started,
            running,
            closed,
            completed,
            saved_handled,
            saved_raised,
        ) = match self.heap.get(generator) {
            Some(HeapObject::Generator(generator)) => {
                let globals = match self.heap.get(generator.function) {
                    Some(HeapObject::Function(function)) => function.globals,
                    _ => return Err("generator function is invalid".to_owned()),
                };
                (
                    generator.kind,
                    generator.function,
                    globals,
                    generator.resume_address,
                    generator.started,
                    generator.running,
                    generator.closed,
                    generator.completed,
                    generator.handled.to_vec(),
                    generator.raised,
                )
            }
            _ => return Err("object is not a generator".to_owned()),
        };
        if running {
            return self.raise_error(
                "ValueError",
                match suspended_kind {
                    SuspendedKind::Generator => "generator already executing",
                    SuspendedKind::Coroutine => "coroutine already executing",
                    SuspendedKind::AsyncGenerator => "async generator already executing",
                },
            );
        }
        if completed || closed {
            if suspended_kind == SuspendedKind::Coroutine
                && matches!(
                    operation,
                    RGeneratorOperation::Next | RGeneratorOperation::Send
                )
            {
                return self.raise_error("RuntimeError", "cannot reuse already awaited coroutine");
            }
            if matches!(operation, RGeneratorOperation::Throw) {
                self.raise_value(input, None, false)?;
                return Err("generator raised an exception".to_owned());
            }
            return Ok(GeneratorResume {
                value: RValue::NONE,
                outcome: RGeneratorOutcome::Returned,
            });
        }
        if !started && matches!(operation, RGeneratorOperation::Send) && input != RValue::NONE {
            return self.raise_error(
                "TypeError",
                match suspended_kind {
                    SuspendedKind::Generator => {
                        "can't send non-None value to a just-started generator"
                    }
                    SuspendedKind::Coroutine => {
                        "can't send non-None value to a just-started coroutine"
                    }
                    SuspendedKind::AsyncGenerator => {
                        "can't send non-None value to a just-started async generator"
                    }
                },
            );
        }
        if !started && matches!(operation, RGeneratorOperation::Close) {
            self.detach_generator_frame(generator)?;
            if let Some(HeapObject::Generator(object)) = self.heap.get_mut(generator) {
                object.closed = true;
                object.completed = true;
                object.slots.fill(None);
                object.local_cells.clear();
                object.locals_snapshot = None;
            }
            return Ok(GeneratorResume {
                value: RValue::NONE,
                outcome: RGeneratorOutcome::Returned,
            });
        }
        if !started && matches!(operation, RGeneratorOperation::Throw) {
            self.detach_generator_frame(generator)?;
            if let Some(HeapObject::Generator(object)) = self.heap.get_mut(generator) {
                object.completed = true;
                object.slots.fill(None);
                object.local_cells.clear();
                object.locals_snapshot = None;
            }
            self.raise_value(input, None, false)?;
            return Err("generator raised an exception".to_owned());
        }

        // CPython's currently handled/pending exception remains the implicit
        // chaining context while execution enters a nested coroutine/generator.
        // The suspended object owns its own handled stack, so keep the caller's
        // exception only as ambient chaining state rather than making bare
        // `raise` inside the callee see the caller's handler.
        let caller_exception_context = self.handled.last().copied().or(self.raised);
        let previous_raised = std::mem::replace(&mut self.raised, saved_raised);
        let previous_exception = self.exception.take();
        self.exception = self
            .raised
            .and_then(|exception| self.exception_message(exception));

        let injected = match operation {
            RGeneratorOperation::Throw => Some(input),
            RGeneratorOperation::Close => {
                Some(self.with_temporary_roots(&[generator], |context| {
                    context.new_builtin_exception("GeneratorExit", &[])
                })?)
            }
            RGeneratorOperation::Next | RGeneratorOperation::Send => None,
        };
        if let Some(injected) = injected {
            self.with_temporary_roots(&[generator, injected], |context| {
                context.raise_value(injected, None, false)
            })?;
        }

        let previous_handled = std::mem::replace(&mut self.handled, saved_handled);
        let ambient_checkpoint = self.ambient_exception_context.len();
        if let Some(exception) = caller_exception_context {
            self.ambient_exception_context.push(exception);
        }
        let Some(HeapObject::Generator(object)) = self.heap.get_mut(generator) else {
            self.ambient_exception_context.truncate(ambient_checkpoint);
            self.handled = previous_handled;
            self.raised = previous_raised;
            self.exception = previous_exception;
            return Err("object is not a generator".to_owned());
        };
        object.running = true;
        object.started = true;
        let mut output = RValue::NONE;
        let mut outcome = RGeneratorOutcome::Returned;
        // SAFETY: generator functions enter the heap only through the ABI
        // constructor, which validates the stable generator-resume signature.
        let resume: RNativeGeneratorResume = unsafe { std::mem::transmute(resume_address) };
        self.push_active_generator_call(function, generator)?;
        let switched_globals = self.globals() != Some(globals);
        if switched_globals {
            self.push_module_namespace(globals);
        }
        let status = unsafe {
            resume(
                std::ptr::from_mut(self).cast(),
                &raw const generator,
                operation,
                &raw const input,
                &raw mut output,
                &raw mut outcome,
            )
        };
        let active_index = self.active_calls.len().saturating_sub(1);
        // Reflection storage is lazily materialized for native coroutines. If
        // nobody observed locals/frame state during this activation, keep the
        // resume path free of dictionary allocation and synchronization work.
        let refresh_result = if self
            .active_calls
            .get(active_index)
            .is_some_and(|active| active.locals_snapshot.is_some() || active.frame.is_some())
        {
            self.refresh_locals_at(active_index).map(Some)
        } else {
            Ok(None)
        };
        if switched_globals {
            self.pop_module_namespace(globals);
        }
        self.pop_active_call();
        self.ambient_exception_context.truncate(ambient_checkpoint);
        refresh_result?;
        let saved_after = std::mem::replace(&mut self.handled, previous_handled);

        let mut effective_status = status;
        if status == RStatus::Exception
            && matches!(operation, RGeneratorOperation::Close)
            && self.exception_type_name(self.raised.unwrap_or(RValue::NONE))
                == Some("GeneratorExit")
        {
            self.consume_exception_type("GeneratorExit");
            effective_status = RStatus::Ok;
            outcome = RGeneratorOutcome::Returned;
            output = RValue::NONE;
        } else if status == RStatus::Exception
            && self.exception_type_name(self.raised.unwrap_or(RValue::NONE))
                == Some("StopIteration")
        {
            let original = self.raised.expect("StopIteration was just observed");
            self.with_temporary_roots(&[generator, original], |context| {
                let message = crate::operations::string(context, "generator raised StopIteration")?;
                let replacement = context.new_builtin_exception("RuntimeError", &[message])?;
                let Some(HeapObject::Exception(object)) = context.heap.get_mut(replacement) else {
                    return Err("replacement generator exception is invalid".to_owned());
                };
                object.cause = Some(original);
                object.context = Some(original);
                object.suppress_context = true;
                context.raised = Some(replacement);
                context.exception = Some("generator raised StopIteration".to_owned());
                Ok(())
            })?;
        }

        let suspended_raised = self.raised;
        let terminal = effective_status == RStatus::Exception
            || (effective_status == RStatus::Ok && outcome == RGeneratorOutcome::Returned);
        if let Some(HeapObject::Generator(object)) = self.heap.get_mut(generator) {
            object.running = false;
            object.handled = saved_after.into_boxed_slice();
            object.raised = (effective_status == RStatus::Ok
                && matches!(
                    outcome,
                    RGeneratorOutcome::Yielded | RGeneratorOutcome::Suspended
                ))
            .then_some(suspended_raised)
            .flatten();
            if effective_status == RStatus::Ok && outcome == RGeneratorOutcome::Returned {
                object.completed = true;
                object.closed |= matches!(operation, RGeneratorOperation::Close);
                object.return_value = Some(output);
                object.delegate = None;
                object.slots.fill(None);
                object.local_cells.clear();
                object.locals_snapshot = None;
            } else if effective_status == RStatus::Exception {
                object.completed = true;
                object.return_value = None;
                object.delegate = None;
                object.slots.fill(None);
                object.local_cells.clear();
                object.locals_snapshot = None;
            }
        }
        if terminal {
            self.detach_generator_frame(generator)?;
        }
        if effective_status == RStatus::Ok {
            self.raised = previous_raised;
            self.exception = previous_exception;
        }
        match effective_status {
            RStatus::Ok => Ok(GeneratorResume {
                value: output,
                outcome,
            }),
            RStatus::Exception => Err("generator raised an exception".to_owned()),
            RStatus::InvalidArgument => Err("generator resume arguments are invalid".to_owned()),
            RStatus::AbiMismatch => Err("generator uses an incompatible Rimera ABI".to_owned()),
        }
    }

    pub(crate) fn namespace_new(&mut self) -> Result<RValue, String> {
        self.allocate(HeapObject::Dictionary(DictionaryObject {
            entries: Vec::new(),
        }))
    }

    pub(crate) fn ensure_annotations(&mut self, namespace: Option<RValue>) -> Result<(), String> {
        self.initialize_kernel()?;
        let namespace = namespace
            .or_else(|| self.globals())
            .ok_or_else(|| "annotation namespace is unavailable".to_owned())?;
        if self.namespace_value(namespace, "__annotations__").is_some() {
            return Ok(());
        }
        if !matches!(
            self.heap.get(namespace),
            Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_))
        ) {
            match self.namespace_get(namespace, "__annotations__") {
                Ok(_) => return Ok(()),
                Err(_) if self.consume_exception_type("KeyError") => {}
                Err(error) => return Err(error),
            }
        }
        let annotations = crate::operations::dictionary(self, &[], &[])?;
        self.with_temporary_roots(&[namespace, annotations], |context| {
            context.namespace_set(namespace, "__annotations__", annotations)
        })
    }

    pub(crate) fn namespace_set(
        &mut self,
        namespace: RValue,
        name: &str,
        value: RValue,
    ) -> Result<(), String> {
        if let Some(HeapObject::Dictionary(dictionary)) = self.heap.get(namespace) {
            let growth = dictionary.managed_growth_for_insert(name);
            let mut roots = dictionary
                .entries
                .iter()
                .map(|(_, value)| *value)
                .collect::<Vec<_>>();
            roots.extend([namespace, value]);
            self.with_temporary_roots(&roots, |context| context.preflight_managed_growth(growth))?;
            let Some(HeapObject::Dictionary(dictionary)) = self.heap.get_mut(namespace) else {
                return Err("class namespace is not a dictionary".to_owned());
            };
            dictionary.insert(name.to_owned(), value);
            self.heap.refresh_managed_bytes();
            return Ok(());
        }
        let existing = match self.heap.get(namespace) {
            Some(HeapObject::ValueDictionary(dictionary)) => dictionary
                .table
                .snapshot()
                .into_iter()
                .find_map(|(index, (key, _))| {
                    matches!(self.heap.get(key), Some(HeapObject::String(key)) if key == name)
                        .then_some(index)
                }),
            _ => {
                return self.with_temporary_roots(&[namespace, value], |context| {
                    let key = crate::operations::string(context, name)?;
                    context
                        .invoke_special_method(namespace, "__setitem__", &[key, value])?
                        .ok_or_else(|| {
                            "class namespace does not support item assignment".to_owned()
                        })
                        .map(|_| ())
                });
            }
        };
        if let Some(index) = existing {
            let Some(HeapObject::ValueDictionary(dictionary)) = self.heap.get_mut(namespace) else {
                return Err("class namespace is not a dictionary".to_owned());
            };
            dictionary
                .table
                .update(index, |(_, current)| *current = value);
            return Ok(());
        }
        self.with_temporary_roots(&[namespace, value], |context| {
            let key = crate::operations::string(context, name)?;
            crate::operations::item_set(context, namespace, key, value)
        })
    }

    pub(crate) fn namespace_get(
        &mut self,
        namespace: RValue,
        name: &str,
    ) -> Result<RValue, String> {
        if let Some(value) = self.namespace_value(namespace, name) {
            return Ok(value);
        }
        self.with_temporary_roots(&[namespace], |context| {
            let key = crate::operations::string(context, name)?;
            context
                .invoke_special_method(namespace, "__getitem__", &[key])?
                .ok_or_else(|| "class namespace does not support item access".to_owned())
        })
    }

    pub(crate) fn namespace_delete(&mut self, namespace: RValue, name: &str) -> Result<(), String> {
        if let Some(HeapObject::Dictionary(dictionary)) = self.heap.get_mut(namespace) {
            if dictionary.remove(name).is_some() {
                return Ok(());
            }
            return Err(format!("class namespace has no attribute '{name}'"));
        }
        let index = match self.heap.get(namespace) {
            Some(HeapObject::ValueDictionary(dictionary)) => dictionary
                .table
                .snapshot()
                .into_iter()
                .find_map(|(index, (key, _))| {
                    matches!(self.heap.get(key), Some(HeapObject::String(key)) if key == name)
                        .then_some(index)
                }),
            _ => {
                return self.with_temporary_roots(&[namespace], |context| {
                    let key = crate::operations::string(context, name)?;
                    context
                        .invoke_special_method(namespace, "__delitem__", &[key])?
                        .ok_or_else(|| "class namespace does not support item deletion".to_owned())
                        .map(|_| ())
                });
            }
        };
        let Some(index) = index else {
            return Err(format!("class namespace has no attribute '{name}'"));
        };
        let Some(HeapObject::ValueDictionary(dictionary)) = self.heap.get_mut(namespace) else {
            return Err("class namespace is not a dictionary".to_owned());
        };
        dictionary.table.remove(index);
        Ok(())
    }

    /// Resolves a class-scope name. Missing mapping keys intentionally fall
    /// through to module globals and builtins, matching Python class scope.
    pub(crate) fn class_name_get(
        &mut self,
        namespace: RValue,
        name: &str,
    ) -> Result<RValue, String> {
        if let Some(value) = self.namespace_value(namespace, name) {
            return Ok(value);
        }
        if !matches!(
            self.heap.get(namespace),
            Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_))
        ) {
            match self.namespace_get(namespace, name) {
                Ok(value) => return Ok(value),
                Err(error) if self.consume_exception_type("KeyError") => {}
                Err(error) => return Err(error),
            }
        }
        if let Some(value) = self
            .globals()
            .and_then(|globals| self.namespace_value(globals, name))
        {
            return Ok(value);
        }
        match self.execution_builtin(name)? {
            Some(value) => Ok(value),
            None => self.raise_error("NameError", format!("name '{name}' is not defined")),
        }
    }

    pub(crate) fn execution_builtin(&mut self, name: &str) -> Result<Option<RValue>, String> {
        if let Some(namespace) = self
            .globals()
            .and_then(|globals| self.namespace_value(globals, "__builtins__"))
        {
            let namespace = match self.heap.get(namespace) {
                Some(HeapObject::Module(module)) => module.namespace,
                _ => namespace,
            };
            if Some(namespace) != self.builtins() {
                return Ok(self.namespace_value(namespace, name));
            }
        }
        if let Some(value) = self.lookup_builtin(name) {
            return Ok(Some(value));
        }
        self.ensure_builtin(name)
    }

    /// Resolves a class-body free name. Python still consults the prepared
    /// class namespace first, but a miss falls back to the enclosing cell
    /// rather than globals/builtins.
    pub(crate) fn class_free_get(
        &mut self,
        namespace: RValue,
        cell: RValue,
        name: &str,
    ) -> Result<RValue, String> {
        if let Some(value) = self.namespace_value(namespace, name) {
            return Ok(value);
        }
        if !matches!(
            self.heap.get(namespace),
            Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_))
        ) {
            match self.namespace_get(namespace, name) {
                Ok(value) => return Ok(value),
                Err(error) if self.consume_exception_type("KeyError") => {}
                Err(error) => return Err(error),
            }
        }
        match self.heap.get(cell) {
            Some(HeapObject::Cell(cell)) => cell.value.ok_or_else(|| {
                format!(
                    "cannot access free variable '{name}' where it is not associated with a value in enclosing scope"
                )
            }),
            _ => Err("class free-name lookup received an invalid closure cell".to_owned()),
        }
    }

    fn code_var_names(code: &CodeObject) -> Vec<String> {
        let mut names = Vec::new();
        for kind in [
            rimera_abi::RParameterKind::PositionalOnly,
            rimera_abi::RParameterKind::PositionalOrKeyword,
            rimera_abi::RParameterKind::KeywordOnly,
            rimera_abi::RParameterKind::VarArgs,
            rimera_abi::RParameterKind::VarKeywords,
        ] {
            names.extend(
                code.parameters
                    .iter()
                    .filter(|parameter| parameter.kind == kind)
                    .map(|parameter| parameter.name.clone()),
            );
        }
        for local in code.local_names.iter() {
            if !names.contains(local) && !code.cell_names.contains(local) {
                names.push(local.clone());
            }
        }
        names
    }

    fn code_flags(code: &CodeObject) -> u32 {
        if let Some(flags) = code.flags_override {
            return flags;
        }
        if code.code_address == 0 {
            return 0;
        }
        let mut flags = 0x01 | 0x02; // CO_OPTIMIZED | CO_NEWLOCALS
        if code
            .parameters
            .iter()
            .any(|parameter| parameter.kind == rimera_abi::RParameterKind::VarArgs)
        {
            flags |= 0x04;
        }
        if code
            .parameters
            .iter()
            .any(|parameter| parameter.kind == rimera_abi::RParameterKind::VarKeywords)
        {
            flags |= 0x08;
        }
        if code.qualified_name.contains(".<locals>.") {
            flags |= 0x10;
        }
        if matches!(code.kind, FunctionKind::Generator { .. }) {
            flags |= 0x20;
        }
        flags
    }

    fn reflection_string_tuple(&mut self, names: &[String]) -> Result<RValue, String> {
        let mut values = Vec::with_capacity(names.len());
        for name in names {
            let value = self.with_temporary_roots(&values, |context| {
                crate::operations::string(context, name)
            })?;
            values.push(value);
        }
        self.with_temporary_roots(&values, |context| {
            crate::operations::tuple(context, &values)
        })
    }

    pub(crate) fn attribute_get(&mut self, receiver: RValue, name: &str) -> Result<RValue, String> {
        if let Some(HeapObject::Module(module)) = self.heap.get(receiver) {
            let namespace = module.namespace;
            if name == "__dict__" {
                return Ok(namespace);
            }
            return self
                .namespace_value(namespace, name)
                .ok_or_else(|| {
                    if module.state == ModuleState::Initializing {
                        format!("partially initialized module '{}' has no attribute '{name}' (most likely due to a circular import)", module.name)
                    } else {
                        format!("module '{}' has no attribute '{name}'", module.name)
                    }
                });
        }
        if let Some(HeapObject::Function(function)) = self.heap.get(receiver) {
            let function_name = function.name.clone();
            let qualified_name = function.qualified_name.clone();
            let code = function.code;
            let closure = function.closure;
            let defaults = function.defaults;
            let keyword_defaults = function.keyword_defaults;
            let annotations = function.annotations;
            let type_params = function.type_params;
            return match name {
                "__name__" => crate::operations::string(self, &function_name),
                "__qualname__" => crate::operations::string(self, &qualified_name),
                "__code__" => Ok(code),
                "__closure__" => Ok(closure.unwrap_or(RValue::NONE)),
                "__defaults__" => Ok(defaults.unwrap_or(RValue::NONE)),
                "__kwdefaults__" => Ok(keyword_defaults.unwrap_or(RValue::NONE)),
                "__type_params__" => match type_params {
                    Some(type_params) => Ok(type_params),
                    None => self.allocate(HeapObject::Tuple(Box::new([]))),
                },
                "__annotations__" => {
                    if let Some(annotations) = annotations {
                        return Ok(annotations);
                    }
                    let annotations = crate::operations::dictionary(self, &[], &[])?;
                    let Some(HeapObject::Function(function)) = self.heap.get_mut(receiver) else {
                        return Err("function disappeared while creating annotations".to_owned());
                    };
                    function.annotations = Some(annotations);
                    Ok(annotations)
                }
                _ => Err(format!("'function' object has no attribute '{name}'")),
            };
        }
        if let Some(HeapObject::TypeParameter(parameter)) = self.heap.get(receiver) {
            let name_value = parameter.name.clone();
            return match name {
                "__name__" => crate::operations::string(self, &name_value),
                "__bound__" => Ok(RValue::NONE),
                "__constraints__" => self.allocate(HeapObject::Tuple(Box::new([]))),
                _ => Err(format!(
                    "'{}' object has no attribute '{name}'",
                    match parameter.kind {
                        rimera_abi::RTypeParameterKind::TypeVar => "TypeVar",
                        rimera_abi::RTypeParameterKind::TypeVarTuple => "TypeVarTuple",
                        rimera_abi::RTypeParameterKind::ParamSpec => "ParamSpec",
                    }
                )),
            };
        }
        if let Some(HeapObject::TypeAlias(alias)) = self.heap.get(receiver) {
            let alias = alias.clone();
            return match name {
                "__name__" => crate::operations::string(self, &alias.name),
                "__type_params__" => Ok(alias.type_params),
                "__value__" => Ok(alias.value),
                _ => Err(format!("'TypeAliasType' object has no attribute '{name}'")),
            };
        }
        if let Some(HeapObject::Code(code)) = self.heap.get(receiver) {
            let code = code.clone();
            return match name {
                "co_name" => crate::operations::string(self, &code.name),
                "co_qualname" => crate::operations::string(self, &code.qualified_name),
                "co_filename" => crate::operations::string(self, &code.filename),
                "co_firstlineno" => Ok(RValue::small_int(i64::from(code.first_line))),
                "co_argcount" => Ok(RValue::small_int(
                    code.parameters
                        .iter()
                        .filter(|parameter| {
                            matches!(
                                parameter.kind,
                                rimera_abi::RParameterKind::PositionalOnly
                                    | rimera_abi::RParameterKind::PositionalOrKeyword
                            )
                        })
                        .count() as i64,
                )),
                "co_posonlyargcount" => Ok(RValue::small_int(
                    code.parameters
                        .iter()
                        .filter(|parameter| {
                            parameter.kind == rimera_abi::RParameterKind::PositionalOnly
                        })
                        .count() as i64,
                )),
                "co_kwonlyargcount" => Ok(RValue::small_int(
                    code.parameters
                        .iter()
                        .filter(|parameter| {
                            parameter.kind == rimera_abi::RParameterKind::KeywordOnly
                        })
                        .count() as i64,
                )),
                "co_nlocals" => Ok(RValue::small_int(Self::code_var_names(&code).len() as i64)),
                "co_varnames" => self.reflection_string_tuple(&Self::code_var_names(&code)),
                "co_cellvars" => self.reflection_string_tuple(&code.cell_names),
                "co_freevars" => self.reflection_string_tuple(&code.free_names),
                "co_flags" => Ok(RValue::small_int(i64::from(Self::code_flags(&code)))),
                _ => Err(format!("'code' object has no attribute '{name}'")),
            };
        }
        if let Some(HeapObject::Cell(cell)) = self.heap.get(receiver) {
            return match name {
                "cell_contents" => cell
                    .value
                    .ok_or_else(|| "Cell is empty".to_owned())
                    .or_else(|message| self.raise_error("ValueError", message)),
                _ => Err(format!("'cell' object has no attribute '{name}'")),
            };
        }
        if let Some(HeapObject::Exception(exception)) = self.heap.get(receiver) {
            let exception_type = exception.exception_type;
            let arguments = exception.arguments;
            let dictionary = exception.dictionary;
            let traceback = exception.traceback;
            let cause = exception.cause;
            let context_value = exception.context;
            let suppress_context = exception.suppress_context;
            match name {
                "args" => return Ok(arguments),
                "value" if self.type_name(exception_type) == "StopIteration" => {
                    let value = match self.heap.get(arguments) {
                        Some(HeapObject::Tuple(arguments)) => {
                            arguments.first().copied().unwrap_or(RValue::NONE)
                        }
                        _ => RValue::NONE,
                    };
                    return Ok(value);
                }
                "__traceback__" => return Ok(traceback.unwrap_or(RValue::NONE)),
                "__cause__" => return Ok(cause.unwrap_or(RValue::NONE)),
                "__context__" => return Ok(context_value.unwrap_or(RValue::NONE)),
                "__suppress_context__" => return Ok(RValue::boolean(suppress_context)),
                "with_traceback" => {
                    return self.bound_builtin_method(
                        receiver,
                        "BaseException.with_traceback",
                        BuiltinFunctionKind::ExceptionWithTraceback,
                    );
                }
                "__dict__" => return self.exception_dictionary(receiver),
                _ => {}
            }
            if let Some((_, descriptor)) = self.class_attribute_with_owner(exception_type, name)
                && self.descriptor_is_data(descriptor)?
            {
                return self.descriptor_get(descriptor, Some(receiver), exception_type);
            }
            if let Some(dictionary) = dictionary
                && let Some(value) = self.namespace_value(dictionary, name)
            {
                return Ok(value);
            }
            if let Some(value) = self.class_attribute(exception_type, name) {
                return self.descriptor_get(value, Some(receiver), exception_type);
            }
            return Err(format!(
                "'{}' object has no attribute '{name}'",
                self.type_name(exception_type)
            ));
        }
        if let Some(HeapObject::Generator(generator)) = self.heap.get(receiver) {
            let function = generator.function;
            let name_override = generator.name_override.clone();
            let qualified_name_override = generator.qualified_name_override.clone();
            let (function_name, function_qualified_name, code) = match self.heap.get(function) {
                Some(HeapObject::Function(function)) => (
                    function.name.clone(),
                    function.qualified_name.clone(),
                    Some(function.code),
                ),
                _ => (String::new(), String::new(), None),
            };
            let generator_name = name_override.unwrap_or(function_name);
            let qualified_name = qualified_name_override.unwrap_or(function_qualified_name);
            let kind = generator.kind;
            let frame = generator.frame;
            let running = generator.running;
            let async_running = generator.running || generator.async_operation_active;
            let terminal = generator.completed || generator.closed;
            let suspended = generator.started
                && !generator.running
                && !generator.completed
                && !generator.closed;
            let delegate = generator.delegate;
            return match kind {
                SuspendedKind::Generator => match name {
                    "__name__" => crate::operations::string(self, &generator_name),
                    "__qualname__" => crate::operations::string(self, &qualified_name),
                    "gi_code" => {
                        code.ok_or_else(|| "generator function has no code object".to_owned())
                    }
                    "gi_frame" => Ok(frame.unwrap_or(RValue::NONE)),
                    "gi_running" => Ok(RValue::boolean(running)),
                    "gi_suspended" => Ok(RValue::boolean(suspended)),
                    "gi_yieldfrom" => Ok(delegate.unwrap_or(RValue::NONE)),
                    "__iter__" => self.bound_builtin_method(
                        receiver,
                        "generator.__iter__",
                        BuiltinFunctionKind::GeneratorIter,
                    ),
                    "__next__" => self.bound_builtin_method(
                        receiver,
                        "generator.__next__",
                        BuiltinFunctionKind::GeneratorNext,
                    ),
                    "send" => self.bound_builtin_method(
                        receiver,
                        "generator.send",
                        BuiltinFunctionKind::GeneratorSend,
                    ),
                    "throw" => self.bound_builtin_method(
                        receiver,
                        "generator.throw",
                        BuiltinFunctionKind::GeneratorThrow,
                    ),
                    "close" => self.bound_builtin_method(
                        receiver,
                        "generator.close",
                        BuiltinFunctionKind::GeneratorClose,
                    ),
                    _ => Err(format!("'generator' object has no attribute '{name}'")),
                },
                SuspendedKind::Coroutine => match name {
                    "__name__" => crate::operations::string(self, &generator_name),
                    "__qualname__" => crate::operations::string(self, &qualified_name),
                    "cr_code" => {
                        code.ok_or_else(|| "coroutine function has no code object".to_owned())
                    }
                    "cr_frame" => {
                        if terminal {
                            Ok(RValue::NONE)
                        } else if let Some(frame) = frame {
                            Ok(frame)
                        } else {
                            self.ensure_suspended_frame(receiver)
                        }
                    }
                    "cr_running" => Ok(RValue::boolean(running)),
                    "cr_suspended" => Ok(RValue::boolean(suspended)),
                    "cr_await" => Ok(delegate.unwrap_or(RValue::NONE)),
                    "send" => self.bound_builtin_method(
                        receiver,
                        "coroutine.send",
                        BuiltinFunctionKind::GeneratorSend,
                    ),
                    "throw" => self.bound_builtin_method(
                        receiver,
                        "coroutine.throw",
                        BuiltinFunctionKind::GeneratorThrow,
                    ),
                    "close" => self.bound_builtin_method(
                        receiver,
                        "coroutine.close",
                        BuiltinFunctionKind::GeneratorClose,
                    ),
                    _ => Err(format!("'coroutine' object has no attribute '{name}'")),
                },
                SuspendedKind::AsyncGenerator => match name {
                    "__name__" => crate::operations::string(self, &generator_name),
                    "__qualname__" => crate::operations::string(self, &qualified_name),
                    "ag_code" => {
                        code.ok_or_else(|| "async generator function has no code object".to_owned())
                    }
                    "ag_frame" => {
                        if terminal {
                            Ok(RValue::NONE)
                        } else if let Some(frame) = frame {
                            Ok(frame)
                        } else {
                            // Match coroutine/generator reflection without
                            // charging unobserved async generators for frame
                            // metadata. The first `ag_frame` read publishes
                            // one traced frame and every later read reuses it.
                            self.ensure_suspended_frame(receiver)
                        }
                    }
                    "ag_running" => Ok(RValue::boolean(async_running)),
                    "ag_await" => Ok(delegate.unwrap_or(RValue::NONE)),
                    "__aiter__" => self.bound_builtin_method(
                        receiver,
                        "async_generator.__aiter__",
                        BuiltinFunctionKind::AsyncGeneratorIter,
                    ),
                    "__anext__" => self.bound_builtin_method(
                        receiver,
                        "async_generator.__anext__",
                        BuiltinFunctionKind::AsyncGeneratorNext,
                    ),
                    "asend" => self.bound_builtin_method(
                        receiver,
                        "async_generator.asend",
                        BuiltinFunctionKind::AsyncGeneratorSend,
                    ),
                    "athrow" => self.bound_builtin_method(
                        receiver,
                        "async_generator.athrow",
                        BuiltinFunctionKind::AsyncGeneratorThrow,
                    ),
                    "aclose" => self.bound_builtin_method(
                        receiver,
                        "async_generator.aclose",
                        BuiltinFunctionKind::AsyncGeneratorClose,
                    ),
                    _ => Err(format!(
                        "'async_generator' object has no attribute '{name}'"
                    )),
                },
            };
        }
        if let Some(HeapObject::AsyncGeneratorOperation(operation)) = self.heap.get(receiver) {
            let type_name = match operation.kind {
                crate::object::AsyncGeneratorOperationKind::Next
                | crate::object::AsyncGeneratorOperationKind::Send => "async_generator_asend",
                crate::object::AsyncGeneratorOperationKind::Throw
                | crate::object::AsyncGeneratorOperationKind::Close => "async_generator_athrow",
            };
            return match name {
                "__await__" | "__iter__" => self.bound_builtin_method(
                    receiver,
                    &format!("{type_name}.__await__"),
                    BuiltinFunctionKind::AsyncGeneratorIter,
                ),
                "__next__" => self.bound_builtin_method(
                    receiver,
                    &format!("{type_name}.__next__"),
                    BuiltinFunctionKind::AsyncGeneratorOperationNext,
                ),
                "send" => self.bound_builtin_method(
                    receiver,
                    &format!("{type_name}.send"),
                    BuiltinFunctionKind::AsyncGeneratorOperationSend,
                ),
                "throw" => self.bound_builtin_method(
                    receiver,
                    &format!("{type_name}.throw"),
                    BuiltinFunctionKind::AsyncGeneratorOperationThrow,
                ),
                "close" => self.bound_builtin_method(
                    receiver,
                    &format!("{type_name}.close"),
                    BuiltinFunctionKind::AsyncGeneratorOperationClose,
                ),
                _ => Err(format!("'{type_name}' object has no attribute '{name}'")),
            };
        }
        if let Some(HeapObject::Traceback(traceback)) = self.heap.get(receiver) {
            let line = traceback.line;
            let frame = traceback.frame;
            let next = traceback.next;
            return match name {
                "tb_frame" => Ok(frame.unwrap_or(RValue::NONE)),
                "tb_lineno" => crate::operations::store_integer(self, line.into()),
                "tb_next" => Ok(next.unwrap_or(RValue::NONE)),
                _ => Err(format!("'traceback' object has no attribute '{name}'")),
            };
        }
        if let Some(HeapObject::Frame(frame)) = self.heap.get(receiver) {
            let code = frame.code;
            let globals = frame.globals;
            let locals = frame.locals;
            let back = frame.back;
            let line = frame.line;
            let generator = frame.generator;
            if name == "f_locals"
                && let Some(generator) = generator
                && let Some(index) = self
                    .active_calls
                    .iter()
                    .position(|active| active.generator == Some(generator))
            {
                let refreshed = self.refresh_locals_at(index)?;
                if refreshed != locals {
                    let Some(HeapObject::Frame(frame)) = self.heap.get_mut(receiver) else {
                        return Err("frame disappeared while refreshing locals".to_owned());
                    };
                    frame.locals = refreshed;
                    return Ok(refreshed);
                }
            }
            return match name {
                "f_code" => Ok(code),
                "f_globals" => Ok(globals),
                "f_locals" => Ok(locals),
                "f_back" => Ok(back.unwrap_or(RValue::NONE)),
                "f_lineno" => crate::operations::store_integer(self, line.into()),
                _ => Err(format!("'frame' object has no attribute '{name}'")),
            };
        }
        match self.heap.get(receiver) {
            Some(HeapObject::Float(_)) => match name {
                "real" => Ok(receiver),
                "imag" => crate::operations::float(self, 0.0),
                "conjugate" => self.bound_builtin_method(
                    receiver,
                    "float.conjugate",
                    BuiltinFunctionKind::FloatConjugate,
                ),
                "is_integer" => self.bound_builtin_method(
                    receiver,
                    "float.is_integer",
                    BuiltinFunctionKind::FloatIsInteger,
                ),
                "as_integer_ratio" => self.bound_builtin_method(
                    receiver,
                    "float.as_integer_ratio",
                    BuiltinFunctionKind::FloatAsIntegerRatio,
                ),
                "hex" => {
                    self.bound_builtin_method(receiver, "float.hex", BuiltinFunctionKind::FloatHex)
                }
                _ => Err(format!("'float' object has no attribute '{name}'")),
            },
            Some(HeapObject::Complex { real, imag }) => {
                let (real, imag) = (*real, *imag);
                match name {
                    "real" => crate::operations::float(self, real),
                    "imag" => crate::operations::float(self, imag),
                    "conjugate" => self.bound_builtin_method(
                        receiver,
                        "complex.conjugate",
                        BuiltinFunctionKind::ComplexConjugate,
                    ),
                    _ => Err(format!("'complex' object has no attribute '{name}'")),
                }
            }
            Some(HeapObject::List(_)) => match name {
                "append" => self.bound_builtin_method(
                    receiver,
                    "list.append",
                    BuiltinFunctionKind::ListAppend,
                ),
                "extend" => self.bound_builtin_method(
                    receiver,
                    "list.extend",
                    BuiltinFunctionKind::ListExtend,
                ),
                "insert" => self.bound_builtin_method(
                    receiver,
                    "list.insert",
                    BuiltinFunctionKind::ListInsert,
                ),
                "pop" => {
                    self.bound_builtin_method(receiver, "list.pop", BuiltinFunctionKind::ListPop)
                }
                "remove" => self.bound_builtin_method(
                    receiver,
                    "list.remove",
                    BuiltinFunctionKind::ListRemove,
                ),
                "clear" => self.bound_builtin_method(
                    receiver,
                    "list.clear",
                    BuiltinFunctionKind::ListClear,
                ),
                "copy" => {
                    self.bound_builtin_method(receiver, "list.copy", BuiltinFunctionKind::ListCopy)
                }
                "count" => self.bound_builtin_method(
                    receiver,
                    "list.count",
                    BuiltinFunctionKind::ListCount,
                ),
                "index" => self.bound_builtin_method(
                    receiver,
                    "list.index",
                    BuiltinFunctionKind::ListIndex,
                ),
                "reverse" => self.bound_builtin_method(
                    receiver,
                    "list.reverse",
                    BuiltinFunctionKind::ListReverse,
                ),
                "sort" => {
                    self.bound_builtin_method(receiver, "list.sort", BuiltinFunctionKind::ListSort)
                }
                _ => Err(format!("'list' object has no attribute '{name}'")),
            },
            Some(HeapObject::ValueDictionary(_) | HeapObject::Dictionary(_)) => match name {
                "keys" => {
                    self.bound_builtin_method(receiver, "dict.keys", BuiltinFunctionKind::DictKeys)
                }
                "values" => self.bound_builtin_method(
                    receiver,
                    "dict.values",
                    BuiltinFunctionKind::DictValues,
                ),
                "items" => self.bound_builtin_method(
                    receiver,
                    "dict.items",
                    BuiltinFunctionKind::DictItems,
                ),
                "get" => {
                    self.bound_builtin_method(receiver, "dict.get", BuiltinFunctionKind::DictGet)
                }
                "setdefault" => self.bound_builtin_method(
                    receiver,
                    "dict.setdefault",
                    BuiltinFunctionKind::DictSetDefault,
                ),
                "pop" => {
                    self.bound_builtin_method(receiver, "dict.pop", BuiltinFunctionKind::DictPop)
                }
                "popitem" => self.bound_builtin_method(
                    receiver,
                    "dict.popitem",
                    BuiltinFunctionKind::DictPopItem,
                ),
                "update" => self.bound_builtin_method(
                    receiver,
                    "dict.update",
                    BuiltinFunctionKind::DictUpdate,
                ),
                "clear" => self.bound_builtin_method(
                    receiver,
                    "dict.clear",
                    BuiltinFunctionKind::DictClear,
                ),
                "copy" => {
                    self.bound_builtin_method(receiver, "dict.copy", BuiltinFunctionKind::DictCopy)
                }
                _ => Err(format!("'dict' object has no attribute '{name}'")),
            },
            Some(HeapObject::Set(_) | HeapObject::FrozenSet(_)) => match name {
                "add" if matches!(self.heap.get(receiver), Some(HeapObject::Set(_))) => {
                    self.bound_builtin_method(receiver, "set.add", BuiltinFunctionKind::SetAdd)
                }
                "discard" if matches!(self.heap.get(receiver), Some(HeapObject::Set(_))) => self
                    .bound_builtin_method(receiver, "set.discard", BuiltinFunctionKind::SetDiscard),
                "remove" if matches!(self.heap.get(receiver), Some(HeapObject::Set(_))) => self
                    .bound_builtin_method(receiver, "set.remove", BuiltinFunctionKind::SetRemove),
                "pop" if matches!(self.heap.get(receiver), Some(HeapObject::Set(_))) => {
                    self.bound_builtin_method(receiver, "set.pop", BuiltinFunctionKind::SetPop)
                }
                "clear" if matches!(self.heap.get(receiver), Some(HeapObject::Set(_))) => {
                    self.bound_builtin_method(receiver, "set.clear", BuiltinFunctionKind::SetClear)
                }
                "update" if matches!(self.heap.get(receiver), Some(HeapObject::Set(_))) => self
                    .bound_builtin_method(receiver, "set.update", BuiltinFunctionKind::SetUpdate),
                "intersection_update"
                    if matches!(self.heap.get(receiver), Some(HeapObject::Set(_))) =>
                {
                    self.bound_builtin_method(
                        receiver,
                        "set.intersection_update",
                        BuiltinFunctionKind::SetIntersectionUpdate,
                    )
                }
                "difference_update"
                    if matches!(self.heap.get(receiver), Some(HeapObject::Set(_))) =>
                {
                    self.bound_builtin_method(
                        receiver,
                        "set.difference_update",
                        BuiltinFunctionKind::SetDifferenceUpdate,
                    )
                }
                "symmetric_difference_update"
                    if matches!(self.heap.get(receiver), Some(HeapObject::Set(_))) =>
                {
                    self.bound_builtin_method(
                        receiver,
                        "set.symmetric_difference_update",
                        BuiltinFunctionKind::SetSymmetricDifferenceUpdate,
                    )
                }
                "copy" => {
                    self.bound_builtin_method(receiver, "set.copy", BuiltinFunctionKind::SetCopy)
                }
                "union" => {
                    self.bound_builtin_method(receiver, "set.union", BuiltinFunctionKind::SetUnion)
                }
                "intersection" => self.bound_builtin_method(
                    receiver,
                    "set.intersection",
                    BuiltinFunctionKind::SetIntersection,
                ),
                "difference" => self.bound_builtin_method(
                    receiver,
                    "set.difference",
                    BuiltinFunctionKind::SetDifference,
                ),
                "symmetric_difference" => self.bound_builtin_method(
                    receiver,
                    "set.symmetric_difference",
                    BuiltinFunctionKind::SetSymmetricDifference,
                ),
                "isdisjoint" => self.bound_builtin_method(
                    receiver,
                    "set.isdisjoint",
                    BuiltinFunctionKind::SetIsDisjoint,
                ),
                "issubset" => self.bound_builtin_method(
                    receiver,
                    "set.issubset",
                    BuiltinFunctionKind::SetIsSubset,
                ),
                "issuperset" => self.bound_builtin_method(
                    receiver,
                    "set.issuperset",
                    BuiltinFunctionKind::SetIsSuperset,
                ),
                _ => {
                    let type_name =
                        if matches!(self.heap.get(receiver), Some(HeapObject::FrozenSet(_))) {
                            "frozenset"
                        } else {
                            "set"
                        };
                    Err(format!("'{type_name}' object has no attribute '{name}'"))
                }
            },
            Some(HeapObject::Slice(slice)) => {
                let slice = slice.clone();
                match name {
                    "start" => Ok(slice.start.unwrap_or(RValue::NONE)),
                    "stop" => Ok(slice.stop.unwrap_or(RValue::NONE)),
                    "step" => Ok(slice.step.unwrap_or(RValue::NONE)),
                    "indices" => self.bound_builtin_method(
                        receiver,
                        "slice.indices",
                        BuiltinFunctionKind::SliceIndices,
                    ),
                    _ => Err(format!("'slice' object has no attribute '{name}'")),
                }
            }
            Some(HeapObject::Range(range)) => {
                let range = range.clone();
                match name {
                    "start" => crate::operations::store_integer(self, range.start),
                    "stop" => crate::operations::store_integer(self, range.stop),
                    "step" => crate::operations::store_integer(self, range.step),
                    "count" => self.bound_builtin_method(
                        receiver,
                        "range.count",
                        BuiltinFunctionKind::RangeCount,
                    ),
                    "index" => self.bound_builtin_method(
                        receiver,
                        "range.index",
                        BuiltinFunctionKind::RangeIndex,
                    ),
                    _ => Err(format!("'range' object has no attribute '{name}'")),
                }
            }
            Some(HeapObject::DictionaryView(view)) => {
                let view = view.clone();
                match name {
                    "mapping" => self.with_temporary_roots(&[view.dictionary], |context| {
                        context.allocate(HeapObject::MappingProxy(
                            crate::object::MappingProxyObject {
                                dictionary: view.dictionary,
                            },
                        ))
                    }),
                    "isdisjoint"
                        if matches!(
                            view.kind,
                            crate::object::DictionaryViewKind::Keys
                                | crate::object::DictionaryViewKind::Items
                        ) =>
                    {
                        self.bound_builtin_method(
                            receiver,
                            "dict_view.isdisjoint",
                            BuiltinFunctionKind::SetIsDisjoint,
                        )
                    }
                    _ => Err(format!("'dict view' object has no attribute '{name}'")),
                }
            }
            Some(HeapObject::MemoryView(_)) => match name {
                "release" => self.bound_builtin_method(
                    receiver,
                    "memoryview.release",
                    BuiltinFunctionKind::MemoryViewRelease,
                ),
                "tobytes" => self.bound_builtin_method(
                    receiver,
                    "memoryview.tobytes",
                    BuiltinFunctionKind::MemoryViewToBytes,
                ),
                "tolist" => self.bound_builtin_method(
                    receiver,
                    "memoryview.tolist",
                    BuiltinFunctionKind::MemoryViewToList,
                ),
                "toreadonly" => self.bound_builtin_method(
                    receiver,
                    "memoryview.toreadonly",
                    BuiltinFunctionKind::MemoryViewToReadOnly,
                ),
                "hex" => self.bound_builtin_method(
                    receiver,
                    "memoryview.hex",
                    BuiltinFunctionKind::MemoryViewHex,
                ),
                "cast" => self.bound_builtin_method(
                    receiver,
                    "memoryview.cast",
                    BuiltinFunctionKind::MemoryViewCast,
                ),
                "obj" | "format" | "itemsize" | "ndim" | "shape" | "strides" | "suboffsets"
                | "readonly" | "nbytes" | "c_contiguous" | "f_contiguous" | "contiguous" => {
                    self.memoryview_metadata(receiver, name)
                }
                _ => Err(format!("'memoryview' object has no attribute '{name}'")),
            },
            Some(HeapObject::Property(_)) => {
                let kind = match name {
                    "getter" => PropertyMethodKind::Getter,
                    "setter" => PropertyMethodKind::Setter,
                    "deleter" => PropertyMethodKind::Deleter,
                    _ => return Err(format!("'property' object has no attribute '{name}'")),
                };
                self.property_method(receiver, kind)
            }
            Some(HeapObject::Super(object)) => {
                let object = object.clone();
                let mro = match self.heap.get(object.receiver_type) {
                    Some(HeapObject::Type(receiver_type)) => receiver_type.mro.to_vec(),
                    _ => return Err("super object has an invalid receiver type".to_owned()),
                };
                let start = mro
                    .iter()
                    .position(|candidate| *candidate == object.start_type)
                    .ok_or_else(|| {
                        "super(type, obj): obj must be an instance or subtype of type".to_owned()
                    })?;
                for candidate in &mro[start + 1..] {
                    let (value, builtin_storage_init) = match self.heap.get(*candidate) {
                        Some(HeapObject::Type(class)) => {
                            let value = self.namespace_value(class.namespace, name);
                            let builtin_storage_init = value.is_none()
                                && name == "__init__"
                                && class.flags & TYPE_FLAG_BUILTIN != 0
                                && matches!(
                                    class.layout,
                                    TypeLayout::List
                                        | TypeLayout::Dictionary
                                        | TypeLayout::Set
                                        | TypeLayout::ByteArray
                                );
                            (value, builtin_storage_init)
                        }
                        _ => (None, false),
                    };
                    if let Some(value) = value {
                        return self.descriptor_get(
                            value,
                            Some(object.receiver),
                            object.receiver_type,
                        );
                    }
                    if builtin_storage_init {
                        return self.bound_builtin_method(
                            object.receiver,
                            &format!("{}.__init__", self.type_name(*candidate)),
                            BuiltinFunctionKind::BuiltinStorageInit,
                        );
                    }
                }
                Err(format!("'super' object has no attribute '{name}'"))
            }
            Some(HeapObject::Instance(instance)) => {
                let dictionary = instance.dictionary;
                let class = instance.class;
                let storage = instance.storage;
                if let Some(hook) = self.class_attribute(class, "__getattribute__") {
                    let callable = self.descriptor_get(hook, Some(receiver), class)?;
                    let name_value = self
                        .with_temporary_roots(&[receiver, callable], |context| {
                            crate::operations::string(context, name)
                        })?;
                    let result = self
                        .with_temporary_roots(&[receiver, callable, name_value], |context| {
                            crate::call::invoke(context, callable, &[name_value], &[])
                        });
                    match result {
                        Ok(value) => return Ok(value),
                        Err(error)
                            if self.raised.is_some_and(|raised| {
                                self.exception_type_name(raised) == Some("AttributeError")
                            }) =>
                        {
                            let original_raised = self.raised.take();
                            let original_message = self.exception.take();
                            if let Some(fallback) = self.class_attribute(class, "__getattr__") {
                                let callable =
                                    self.descriptor_get(fallback, Some(receiver), class)?;
                                let name_value = self
                                    .with_temporary_roots(&[receiver, callable], |context| {
                                        crate::operations::string(context, name)
                                    })?;
                                return self.with_temporary_roots(
                                    &[receiver, callable, name_value],
                                    |context| {
                                        crate::call::invoke(context, callable, &[name_value], &[])
                                    },
                                );
                            }
                            self.raised = original_raised;
                            self.exception = original_message;
                            return Err(error);
                        }
                        Err(error) => return Err(error),
                    }
                }
                if let Some((_, descriptor)) = self.class_attribute_with_owner(class, name)
                    && self.descriptor_is_data(descriptor)?
                {
                    return self.descriptor_get(descriptor, Some(receiver), class);
                }
                if name == "__dict__"
                    && let Some(dictionary) = dictionary
                {
                    return Ok(dictionary);
                }
                if let Some(dictionary) = dictionary
                    && let Some(value) = self.namespace_value(dictionary, name)
                {
                    return Ok(value);
                }
                if let Some(value) = self.class_attribute(class, name) {
                    return self.descriptor_get(value, Some(receiver), class);
                }
                if let Some(storage) = storage {
                    match self.attribute_get(storage, name) {
                        Ok(value) => return Ok(value),
                        Err(error) if error.contains("has no attribute") => {}
                        Err(error) => return Err(error),
                    }
                }
                if let Some(hook) = self.class_attribute(class, "__getattr__") {
                    let callable = self.descriptor_get(hook, Some(receiver), class)?;
                    let name_value = crate::operations::string(self, name)?;
                    return self
                        .with_temporary_roots(&[receiver, callable, name_value], |context| {
                            crate::call::invoke(context, callable, &[name_value], &[])
                        });
                }
                Err(format!(
                    "'{}' object has no attribute '{name}'",
                    self.type_name(class)
                ))
            }
            Some(HeapObject::Type(class)) => {
                let class_name = class.name.clone();
                let qualified_name = class.qualified_name.clone();
                let bases = class.bases.to_vec();
                let metaclass = class.metaclass;
                if let Some(hook) = self.class_attribute(metaclass, "__getattribute__") {
                    let callable = self.descriptor_get(hook, Some(receiver), metaclass)?;
                    let name_value = crate::operations::string(self, name)?;
                    let result = self
                        .with_temporary_roots(&[receiver, callable, name_value], |context| {
                            crate::call::invoke(context, callable, &[name_value], &[])
                        });
                    match result {
                        Ok(value) => return Ok(value),
                        Err(error)
                            if self.raised.is_some_and(|raised| {
                                self.exception_type_name(raised) == Some("AttributeError")
                            }) =>
                        {
                            let original_raised = self.raised.take();
                            let original_message = self.exception.take();
                            if let Some(fallback) = self.class_attribute(metaclass, "__getattr__") {
                                let callable =
                                    self.descriptor_get(fallback, Some(receiver), metaclass)?;
                                let name_value = crate::operations::string(self, name)?;
                                return self.with_temporary_roots(
                                    &[receiver, callable, name_value],
                                    |context| {
                                        crate::call::invoke(context, callable, &[name_value], &[])
                                    },
                                );
                            }
                            self.raised = original_raised;
                            self.exception = original_message;
                            return Err(error);
                        }
                        Err(error) => return Err(error),
                    }
                }
                if let Some((_, descriptor)) = self.class_attribute_with_owner(metaclass, name)
                    && self.descriptor_is_data(descriptor)?
                {
                    return self.descriptor_get(descriptor, Some(receiver), metaclass);
                }
                if name == "__name__" {
                    return crate::operations::string(self, &class_name);
                }
                if name == "__qualname__" {
                    return crate::operations::string(self, &qualified_name);
                }
                if name == "__bases__" {
                    return self.with_temporary_roots(&bases, |context| {
                        context.allocate(HeapObject::Tuple(bases.clone().into_boxed_slice()))
                    });
                }
                if name == "__mro__" {
                    let mro = match self.heap.get(receiver) {
                        Some(HeapObject::Type(class)) => class.mro.to_vec(),
                        _ => unreachable!("type receiver was checked"),
                    };
                    return self.with_temporary_roots(&mro, |context| {
                        context.allocate(HeapObject::Tuple(mro.clone().into_boxed_slice()))
                    });
                }
                if name == "__dict__" {
                    let namespace = match self.heap.get(receiver) {
                        Some(HeapObject::Type(class)) => class.namespace,
                        _ => unreachable!("type receiver was checked"),
                    };
                    return self.with_temporary_roots(&[namespace], |context| {
                        context.allocate(HeapObject::MappingProxy(
                            crate::object::MappingProxyObject {
                                dictionary: namespace,
                            },
                        ))
                    });
                }
                let value = if let Some(value) = self.class_attribute(receiver, name) {
                    value
                } else if name == "fromhex"
                    && self.builtin_type("float").is_some_and(|float_type| {
                        self.is_subclass(receiver, float_type).unwrap_or(false)
                    })
                {
                    let function =
                        self.allocate(HeapObject::BuiltinFunction(BuiltinFunctionObject {
                            name: "float.fromhex".to_owned(),
                            kind: BuiltinFunctionKind::FloatFromHex,
                        }))?;
                    return self.bind_method(function, receiver);
                } else if name == "__prepare__" {
                    let function =
                        self.allocate(HeapObject::BuiltinFunction(BuiltinFunctionObject {
                            name: "__prepare__".to_owned(),
                            kind: BuiltinFunctionKind::TypePrepare,
                        }))?;
                    return self.bind_method(function, receiver);
                } else if let Some(value) = self.class_attribute(metaclass, name) {
                    return self.descriptor_get(value, Some(receiver), metaclass);
                } else {
                    if let Some(fallback) = self.class_attribute(metaclass, "__getattr__") {
                        let callable = self.descriptor_get(fallback, Some(receiver), metaclass)?;
                        let name_value = crate::operations::string(self, name)?;
                        return self
                            .with_temporary_roots(&[receiver, callable, name_value], |context| {
                                crate::call::invoke(context, callable, &[name_value], &[])
                            });
                    }
                    return Err(format!(
                        "type object '{class_name}' has no attribute '{name}'"
                    ));
                };
                self.descriptor_get(value, None, receiver)
            }
            _ => {
                let type_name = self
                    .type_of(receiver)
                    .ok()
                    .map_or_else(|| "object".to_owned(), |value| self.type_name(value));
                Err(format!("'{type_name}' object has no attribute '{name}'"))
            }
        }
    }

    fn exception_dictionary(&mut self, receiver: RValue) -> Result<RValue, String> {
        if let Some(HeapObject::Exception(exception)) = self.heap.get(receiver)
            && let Some(dictionary) = exception.dictionary
        {
            return Ok(dictionary);
        }
        let dictionary = self.with_temporary_roots(&[receiver], |context| {
            crate::operations::dictionary(context, &[], &[])
        })?;
        let Some(HeapObject::Exception(exception)) = self.heap.get_mut(receiver) else {
            return Err("exception disappeared while creating __dict__".to_owned());
        };
        exception.dictionary = Some(dictionary);
        Ok(dictionary)
    }

    fn bound_builtin_method(
        &mut self,
        receiver: RValue,
        name: &str,
        kind: BuiltinFunctionKind,
    ) -> Result<RValue, String> {
        self.with_temporary_roots(&[receiver], |context| {
            let function =
                context.allocate(HeapObject::BuiltinFunction(BuiltinFunctionObject {
                    name: name.to_owned(),
                    kind,
                }))?;
            context.bind_method(function, receiver)
        })
    }

    fn memoryview_metadata(&mut self, receiver: RValue, name: &str) -> Result<RValue, String> {
        let (exporter, format, item_size, shape, strides, suboffsets, readonly, released) =
            match self.heap.get(receiver) {
                Some(HeapObject::MemoryView(view)) => (
                    view.exporter,
                    view.format.clone(),
                    view.item_size,
                    view.shape.to_vec(),
                    view.strides.to_vec(),
                    view.suboffsets.to_vec(),
                    view.readonly,
                    view.released,
                ),
                _ => return Err("memoryview receiver is invalid".to_owned()),
            };
        if released {
            return Err("operation forbidden on released memoryview object".to_owned());
        }
        let c_contiguous = {
            let mut expected = item_size as isize;
            let mut contiguous = true;
            for axis in (0..shape.len()).rev() {
                if shape[axis] > 1 && strides.get(axis).copied() != Some(expected) {
                    contiguous = false;
                    break;
                }
                expected = expected.saturating_mul(shape[axis] as isize);
            }
            contiguous
        };
        let f_contiguous = {
            let mut expected = item_size as isize;
            let mut contiguous = true;
            for (axis, dimension) in shape.iter().copied().enumerate() {
                if dimension > 1 && strides.get(axis).copied() != Some(expected) {
                    contiguous = false;
                    break;
                }
                expected = expected.saturating_mul(dimension as isize);
            }
            contiguous
        };
        match name {
            "obj" => Ok(exporter),
            "format" => crate::operations::string(self, &format),
            "itemsize" => crate::operations::store_integer(self, item_size.into()),
            "ndim" => crate::operations::store_integer(self, shape.len().into()),
            "readonly" => Ok(RValue::boolean(readonly)),
            "nbytes" => crate::operations::store_integer(
                self,
                shape
                    .iter()
                    .product::<usize>()
                    .saturating_mul(item_size)
                    .into(),
            ),
            "shape" => self.usize_tuple(&shape),
            "strides" => self.isize_tuple(&strides),
            "suboffsets" => self.isize_tuple(&suboffsets),
            "c_contiguous" => Ok(RValue::boolean(c_contiguous)),
            "f_contiguous" => Ok(RValue::boolean(f_contiguous)),
            "contiguous" => Ok(RValue::boolean(c_contiguous || f_contiguous)),
            _ => unreachable!("memoryview metadata name is selected by attribute_get"),
        }
    }

    fn usize_tuple(&mut self, values: &[usize]) -> Result<RValue, String> {
        let values = values
            .iter()
            .map(|value| crate::operations::store_integer(self, (*value).into()))
            .collect::<Result<Vec<_>, _>>()?;
        crate::operations::tuple(self, &values)
    }

    fn isize_tuple(&mut self, values: &[isize]) -> Result<RValue, String> {
        let values = values
            .iter()
            .map(|value| crate::operations::store_integer(self, (*value).into()))
            .collect::<Result<Vec<_>, _>>()?;
        crate::operations::tuple(self, &values)
    }

    pub(crate) fn attribute_set(
        &mut self,
        receiver: RValue,
        name: &str,
        value: RValue,
    ) -> Result<(), String> {
        if let Some(HeapObject::Module(module)) = self.heap.get(receiver) {
            let namespace = module.namespace;
            if name == "__dict__" {
                return self.raise_error("AttributeError", "readonly attribute");
            }
            return self.namespace_set(namespace, name, value);
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Function(_))) {
            enum FunctionAttributeUpdate {
                Name(String),
                QualifiedName(String),
                Code(RValue, FastCallMetadata),
                Defaults(Option<RValue>),
                KeywordDefaults(Option<RValue>),
                Annotations(Option<RValue>),
                TypeParameters(RValue),
            }
            let (function_name, closure_count) = match self.heap.get(receiver) {
                Some(HeapObject::Function(function)) => {
                    let count = function
                        .closure
                        .and_then(|closure| match self.heap.get(closure) {
                            Some(HeapObject::Tuple(values)) => Some(values.len()),
                            _ => None,
                        })
                        .unwrap_or(0);
                    (function.name.clone(), count)
                }
                _ => unreachable!("function receiver was checked"),
            };
            let update = match name {
                "__name__" => match self.heap.get(value) {
                    Some(HeapObject::String(value)) => FunctionAttributeUpdate::Name(value.clone()),
                    _ => {
                        return self
                            .raise_error("TypeError", "__name__ must be set to a string object");
                    }
                },
                "__qualname__" => match self.heap.get(value) {
                    Some(HeapObject::String(value)) => {
                        FunctionAttributeUpdate::QualifiedName(value.clone())
                    }
                    _ => {
                        return self.raise_error(
                            "TypeError",
                            "__qualname__ must be set to a string object",
                        );
                    }
                },
                "__code__" => {
                    let (free_count, fast_call) = match self.heap.get(value) {
                        Some(HeapObject::Code(code)) if code.code_address != 0 => {
                            (code.free_names.len(), code.fast_call_metadata())
                        }
                        _ => {
                            return self
                                .raise_error("TypeError", "__code__ must be set to a code object");
                        }
                    };
                    if closure_count != free_count {
                        return self.raise_error(
                            "ValueError",
                            format!(
                                "{}() requires a code object with {} free vars, not {}",
                                function_name, closure_count, free_count
                            ),
                        );
                    }
                    FunctionAttributeUpdate::Code(value, fast_call)
                }
                "__closure__" => {
                    return self.raise_error("AttributeError", "readonly attribute");
                }
                "__defaults__" => {
                    let defaults = if value == RValue::NONE {
                        None
                    } else if matches!(self.heap.get(value), Some(HeapObject::Tuple(_))) {
                        Some(value)
                    } else {
                        return self.raise_error(
                            "TypeError",
                            "__defaults__ must be set to a tuple object",
                        );
                    };
                    FunctionAttributeUpdate::Defaults(defaults)
                }
                "__kwdefaults__" => {
                    let defaults = if value == RValue::NONE {
                        None
                    } else if matches!(self.heap.get(value), Some(HeapObject::ValueDictionary(_))) {
                        Some(value)
                    } else {
                        return self.raise_error(
                            "TypeError",
                            "__kwdefaults__ must be set to a dict object",
                        );
                    };
                    FunctionAttributeUpdate::KeywordDefaults(defaults)
                }
                "__type_params__" => {
                    if !matches!(self.heap.get(value), Some(HeapObject::Tuple(_))) {
                        return self
                            .raise_error("TypeError", "__type_params__ must be set to a tuple");
                    }
                    FunctionAttributeUpdate::TypeParameters(value)
                }
                "__annotations__" => {
                    let annotations = if value == RValue::NONE {
                        None
                    } else if matches!(self.heap.get(value), Some(HeapObject::ValueDictionary(_))) {
                        Some(value)
                    } else {
                        return self.raise_error(
                            "TypeError",
                            "__annotations__ must be set to a dict object",
                        );
                    };
                    FunctionAttributeUpdate::Annotations(annotations)
                }
                _ => return Err(format!("'function' object has no attribute '{name}'")),
            };
            let (before_size, mut candidate) = match self.heap.get(receiver) {
                Some(HeapObject::Function(function)) => (
                    HeapObject::Function(function.clone()).managed_size(),
                    function.clone(),
                ),
                _ => return Err("function disappeared while setting metadata".to_owned()),
            };
            match update {
                FunctionAttributeUpdate::Name(value) => candidate.name = value,
                FunctionAttributeUpdate::QualifiedName(value) => candidate.qualified_name = value,
                FunctionAttributeUpdate::Code(value, fast_call) => {
                    candidate.code = value;
                    candidate.fast_call = fast_call;
                }
                FunctionAttributeUpdate::Defaults(value) => candidate.defaults = value,
                FunctionAttributeUpdate::KeywordDefaults(value) => {
                    candidate.keyword_defaults = value
                }
                FunctionAttributeUpdate::Annotations(value) => candidate.annotations = value,
                FunctionAttributeUpdate::TypeParameters(value) => {
                    candidate.type_params = Some(value)
                }
            }
            let after_size = HeapObject::Function(candidate.clone()).managed_size();
            let growth = after_size.saturating_sub(before_size);
            self.with_temporary_roots(&[receiver, value], |context| {
                context.preflight_managed_growth(growth)
            })?;
            let Some(HeapObject::Function(function)) = self.heap.get_mut(receiver) else {
                return Err("function disappeared while setting metadata".to_owned());
            };
            *function = candidate;
            self.heap.refresh_managed_bytes();
            return Ok(());
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Code(_))) {
            return self.raise_error("AttributeError", "readonly attribute");
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Frame(_))) {
            return self.raise_error("AttributeError", "readonly attribute");
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Generator(_))) {
            match name {
                "__name__" | "__qualname__" => {
                    let text = match self.heap.get(value) {
                        Some(HeapObject::String(value)) => value.clone(),
                        _ => {
                            return self.raise_error(
                                "TypeError",
                                if name == "__name__" {
                                    "__name__ must be set to a string object"
                                } else {
                                    "__qualname__ must be set to a string object"
                                },
                            );
                        }
                    };
                    let (before_size, mut candidate) = match self.heap.get(receiver) {
                        Some(HeapObject::Generator(generator)) => (
                            HeapObject::Generator(generator.clone()).managed_size(),
                            generator.clone(),
                        ),
                        _ => unreachable!("generator receiver was checked"),
                    };
                    if name == "__name__" {
                        candidate.name_override = Some(text);
                    } else {
                        candidate.qualified_name_override = Some(text);
                    }
                    let after_size = HeapObject::Generator(candidate.clone()).managed_size();
                    let growth = after_size.saturating_sub(before_size);
                    self.with_temporary_roots(&[receiver, value], |context| {
                        context.preflight_managed_growth(growth)
                    })?;
                    let Some(HeapObject::Generator(generator)) = self.heap.get_mut(receiver) else {
                        unreachable!("generator receiver was checked");
                    };
                    *generator = candidate;
                    self.heap.refresh_managed_bytes();
                    return Ok(());
                }
                "gi_code" | "gi_frame" | "gi_running" | "gi_suspended" | "gi_yieldfrom" => {
                    return self.raise_error(
                        "AttributeError",
                        format!("attribute '{name}' of 'generator' objects is not writable"),
                    );
                }
                _ => return Err(format!("'generator' object has no attribute '{name}'")),
            }
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Cell(_))) {
            if name != "cell_contents" {
                return Err(format!("'cell' object has no attribute '{name}'"));
            }
            let Some(HeapObject::Cell(cell)) = self.heap.get_mut(receiver) else {
                unreachable!("cell receiver was checked");
            };
            cell.value = Some(value);
            return Ok(());
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Traceback(_))) {
            match name {
                "tb_frame" => {
                    return self.raise_error(
                        "AttributeError",
                        "attribute 'tb_frame' of 'traceback' objects is not writable",
                    );
                }
                "tb_lineno" => {
                    return self.raise_error(
                        "AttributeError",
                        "attribute 'tb_lineno' of 'traceback' objects is not writable",
                    );
                }
                "tb_next" => {
                    if value != RValue::NONE
                        && !matches!(self.heap.get(value), Some(HeapObject::Traceback(_)))
                    {
                        return self
                            .raise_error("TypeError", "tb_next must be a traceback or None");
                    }
                    let Some(HeapObject::Traceback(traceback)) = self.heap.get_mut(receiver) else {
                        unreachable!("traceback receiver was checked");
                    };
                    traceback.next = (value != RValue::NONE).then_some(value);
                    return Ok(());
                }
                _ => {
                    return Err(format!("'traceback' object has no attribute '{name}'"));
                }
            }
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Exception(_))) {
            match name {
                "args" => {
                    let values = match crate::operations::collect_iterable(self, value) {
                        Ok(values) => values,
                        Err(error) if self.raised.is_some() => return Err(error),
                        Err(error) => return self.raise_error("TypeError", error),
                    };
                    let arguments = self.with_temporary_roots(
                        &values
                            .iter()
                            .copied()
                            .chain([receiver, value])
                            .collect::<Vec<_>>(),
                        |context| crate::operations::tuple(context, &values),
                    )?;
                    let Some(HeapObject::Exception(exception)) = self.heap.get_mut(receiver) else {
                        unreachable!("exception receiver was checked");
                    };
                    exception.arguments = arguments;
                    return Ok(());
                }
                "__traceback__" => {
                    if value != RValue::NONE
                        && !matches!(self.heap.get(value), Some(HeapObject::Traceback(_)))
                    {
                        return self
                            .raise_error("TypeError", "__traceback__ must be a traceback or None");
                    }
                    let Some(HeapObject::Exception(exception)) = self.heap.get_mut(receiver) else {
                        unreachable!("exception receiver was checked");
                    };
                    exception.traceback = (value != RValue::NONE).then_some(value);
                    return Ok(());
                }
                "__cause__" | "__context__" => {
                    if value != RValue::NONE
                        && !matches!(self.heap.get(value), Some(HeapObject::Exception(_)))
                    {
                        return self.raise_error(
                            "TypeError",
                            if name == "__cause__" {
                                "exception cause must be None or derive from BaseException"
                            } else {
                                "exception context must be None or derive from BaseException"
                            },
                        );
                    }
                    let Some(HeapObject::Exception(exception)) = self.heap.get_mut(receiver) else {
                        unreachable!("exception receiver was checked");
                    };
                    let metadata = (value != RValue::NONE).then_some(value);
                    if name == "__cause__" {
                        exception.cause = metadata;
                        exception.suppress_context = true;
                    } else {
                        exception.context = metadata;
                    }
                    return Ok(());
                }
                "__suppress_context__" => {
                    let suppress = if value == RValue::boolean(true) {
                        true
                    } else if value == RValue::boolean(false) {
                        false
                    } else {
                        return self.raise_error("TypeError", "attribute value type must be bool");
                    };
                    let Some(HeapObject::Exception(exception)) = self.heap.get_mut(receiver) else {
                        unreachable!("exception receiver was checked");
                    };
                    exception.suppress_context = suppress;
                    return Ok(());
                }
                "__dict__" => {
                    if !matches!(self.heap.get(value), Some(HeapObject::ValueDictionary(_))) {
                        return self
                            .raise_error("TypeError", "__dict__ must be set to a dictionary");
                    }
                    let Some(HeapObject::Exception(exception)) = self.heap.get_mut(receiver) else {
                        unreachable!("exception receiver was checked");
                    };
                    exception.dictionary = Some(value);
                    return Ok(());
                }
                _ => {}
            }
            let class = match self.heap.get(receiver) {
                Some(HeapObject::Exception(exception)) => exception.exception_type,
                _ => unreachable!("exception receiver was checked"),
            };
            if let Some((_, descriptor)) = self.class_attribute_with_owner(class, name)
                && self.descriptor_is_data(descriptor)?
            {
                return self.descriptor_set(descriptor, receiver, value);
            }
            let dictionary = self.exception_dictionary(receiver)?;
            return self.namespace_set(dictionary, name, value);
        }
        let (namespace, class_receiver, class) = match self.heap.get(receiver) {
            Some(HeapObject::Instance(instance)) => (instance.dictionary, false, instance.class),
            Some(HeapObject::Type(class)) => (Some(class.namespace), true, receiver),
            _ => {
                let type_name = self.type_of(receiver).ok().map_or_else(
                    || "object".to_owned(),
                    |type_value| self.type_name(type_value),
                );
                return Err(format!("'{type_name}' object has no attribute '{name}'"));
            }
        };
        if class_receiver {
            let metaclass = match self.heap.get(receiver) {
                Some(HeapObject::Type(class)) => class.metaclass,
                _ => unreachable!("class receiver was checked"),
            };
            if let Some(hook) = self.class_attribute(metaclass, "__setattr__") {
                let callable = self.descriptor_get(hook, Some(receiver), metaclass)?;
                let name_value = crate::operations::string(self, name)?;
                return self.with_temporary_roots(
                    &[receiver, callable, name_value, value],
                    |context| {
                        crate::call::invoke(context, callable, &[name_value, value], &[])
                            .map(|_| ())
                    },
                );
            }
        }
        if !class_receiver && let Some(hook) = self.class_attribute(class, "__setattr__") {
            let callable = self.descriptor_get(hook, Some(receiver), class)?;
            let name_value = crate::operations::string(self, name)?;
            return self.with_temporary_roots(
                &[receiver, callable, name_value, value],
                |context| {
                    crate::call::invoke(context, callable, &[name_value, value], &[]).map(|_| ())
                },
            );
        }
        if class_receiver && matches!(name, "__name__" | "__qualname__") {
            let class_name = match self.heap.get(receiver) {
                Some(HeapObject::Type(class)) => class.name.clone(),
                _ => unreachable!("class receiver was checked"),
            };
            let text = match self.heap.get(value) {
                Some(HeapObject::String(text)) => text.clone(),
                _ => {
                    let value_type = self.type_of(value)?;
                    let value_type_name = self.type_name(value_type);
                    return self.raise_error(
                        "TypeError",
                        format!(
                            "can only assign string to {class_name}.{name}, not '{value_type_name}'"
                        ),
                    );
                }
            };
            let (before_size, mut candidate) = match self.heap.get(receiver) {
                Some(HeapObject::Type(class)) => (
                    HeapObject::Type(class.clone()).managed_size(),
                    class.clone(),
                ),
                _ => unreachable!("class receiver was checked"),
            };
            if name == "__name__" {
                candidate.name = text;
            } else {
                candidate.qualified_name = text;
            }
            let after_size = HeapObject::Type(candidate.clone()).managed_size();
            let growth = after_size.saturating_sub(before_size);
            self.with_temporary_roots(&[receiver, value], |context| {
                context.preflight_managed_growth(growth)
            })?;
            let Some(HeapObject::Type(class)) = self.heap.get_mut(receiver) else {
                unreachable!("class receiver was checked");
            };
            *class = candidate;
            self.heap.refresh_managed_bytes();
            self.invalidate_type_observers(receiver);
            return Ok(());
        }
        if class_receiver && name == "__mro__" {
            return self.raise_error(
                "AttributeError",
                "attribute '__mro__' of 'type' objects is not writable",
            );
        }
        if class_receiver && name == "__bases__" {
            return match self.set_class_bases(receiver, value) {
                Ok(()) => Ok(()),
                Err(error) if self.raised.is_some() => Err(error),
                Err(error) => self.raise_error("TypeError", error),
            };
        }
        if !class_receiver
            && let Some((_, descriptor)) = self.class_attribute_with_owner(class, name)
            && self.descriptor_is_data(descriptor)?
        {
            return self.descriptor_set(descriptor, receiver, value);
        }
        let Some(namespace) = namespace else {
            return Err(format!(
                "'{}' object has no attribute '{name}'",
                self.type_name(class)
            ));
        };
        self.namespace_set(namespace, name, value)?;
        if class_receiver {
            self.invalidate_type_observers(receiver);
        }
        Ok(())
    }

    pub(crate) fn attribute_delete(&mut self, receiver: RValue, name: &str) -> Result<(), String> {
        if let Some(HeapObject::Module(module)) = self.heap.get(receiver) {
            let namespace = module.namespace;
            if name == "__dict__" {
                return self.raise_error("AttributeError", "readonly attribute");
            }
            return self.namespace_delete(namespace, name);
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Function(_))) {
            if name == "__closure__" {
                return self.raise_error("AttributeError", "readonly attribute");
            }
            if name == "__code__" {
                return self.raise_error("TypeError", "__code__ must be set to a code object");
            }
            if matches!(name, "__name__" | "__qualname__") {
                return self.raise_error(
                    "TypeError",
                    if name == "__name__" {
                        "__name__ must be set to a string object"
                    } else {
                        "__qualname__ must be set to a string object"
                    },
                );
            }
            let Some(HeapObject::Function(function)) = self.heap.get_mut(receiver) else {
                return Err("function disappeared while deleting metadata".to_owned());
            };
            match name {
                "__defaults__" => function.defaults = None,
                "__kwdefaults__" => function.keyword_defaults = None,
                "__annotations__" => function.annotations = None,
                "__type_params__" => {
                    return self.raise_error("TypeError", "__type_params__ must be set to a tuple");
                }
                _ => return Err(format!("'function' object has no attribute '{name}'")),
            }
            return Ok(());
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Code(_))) {
            return self.raise_error("AttributeError", "readonly attribute");
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Frame(_))) {
            return self.raise_error("AttributeError", "readonly attribute");
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Generator(_))) {
            if matches!(name, "__name__" | "__qualname__") {
                return self.raise_error(
                    "TypeError",
                    if name == "__name__" {
                        "__name__ must be set to a string object"
                    } else {
                        "__qualname__ must be set to a string object"
                    },
                );
            }
            if matches!(
                name,
                "gi_code" | "gi_frame" | "gi_running" | "gi_suspended" | "gi_yieldfrom"
            ) {
                return self.raise_error(
                    "AttributeError",
                    format!("attribute '{name}' of 'generator' objects is not writable"),
                );
            }
            return Err(format!("'generator' object has no attribute '{name}'"));
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Cell(_))) {
            if name != "cell_contents" {
                return Err(format!("'cell' object has no attribute '{name}'"));
            }
            let Some(HeapObject::Cell(cell)) = self.heap.get_mut(receiver) else {
                unreachable!("cell receiver was checked");
            };
            cell.value = None;
            return Ok(());
        }
        if matches!(self.heap.get(receiver), Some(HeapObject::Exception(_))) {
            let protected = match name {
                "args" => Some("args may not be deleted"),
                "__traceback__" => Some("__traceback__ may not be deleted"),
                "__cause__" => Some("__cause__ may not be deleted"),
                "__context__" => Some("__context__ may not be deleted"),
                "__suppress_context__" => Some("can't delete numeric/char attribute"),
                "__dict__" => Some("cannot delete __dict__"),
                _ => None,
            };
            if let Some(message) = protected {
                return self.raise_error("TypeError", message);
            }
            let (class, dictionary) = match self.heap.get(receiver) {
                Some(HeapObject::Exception(exception)) => {
                    (exception.exception_type, exception.dictionary)
                }
                _ => unreachable!("exception receiver was checked"),
            };
            if let Some((_, descriptor)) = self.class_attribute_with_owner(class, name)
                && self.descriptor_is_data(descriptor)?
            {
                return self.descriptor_delete(descriptor, receiver);
            }
            if let Some(dictionary) = dictionary {
                return self.namespace_delete(dictionary, name);
            }
            return Err(format!(
                "'{}' object has no attribute '{name}'",
                self.type_name(class)
            ));
        }
        let (namespace, label, class_receiver, class) = match self.heap.get(receiver) {
            Some(HeapObject::Instance(instance)) => (
                instance.dictionary,
                format!("'{}' object", self.type_name(instance.class)),
                false,
                instance.class,
            ),
            Some(HeapObject::Type(class)) => (
                Some(class.namespace),
                format!("type object '{}'", class.name),
                true,
                receiver,
            ),
            _ => {
                let type_name = self
                    .type_of(receiver)
                    .ok()
                    .map_or_else(|| "object".to_owned(), |value| self.type_name(value));
                return Err(format!("'{type_name}' object has no attribute '{name}'"));
            }
        };
        if class_receiver {
            let metaclass = match self.heap.get(receiver) {
                Some(HeapObject::Type(class)) => class.metaclass,
                _ => unreachable!("class receiver was checked"),
            };
            if let Some(hook) = self.class_attribute(metaclass, "__delattr__") {
                let callable = self.descriptor_get(hook, Some(receiver), metaclass)?;
                let name_value = crate::operations::string(self, name)?;
                return self.with_temporary_roots(&[receiver, callable, name_value], |context| {
                    crate::call::invoke(context, callable, &[name_value], &[]).map(|_| ())
                });
            }
        }
        if !class_receiver && let Some(hook) = self.class_attribute(class, "__delattr__") {
            let callable = self.descriptor_get(hook, Some(receiver), class)?;
            let name_value = crate::operations::string(self, name)?;
            return self.with_temporary_roots(&[receiver, callable, name_value], |context| {
                crate::call::invoke(context, callable, &[name_value], &[]).map(|_| ())
            });
        }
        if class_receiver && name == "__mro__" {
            return self.raise_error(
                "AttributeError",
                "attribute '__mro__' of 'type' objects is not writable",
            );
        }
        if class_receiver && matches!(name, "__name__" | "__qualname__" | "__module__") {
            let class_name = match self.heap.get(receiver) {
                Some(HeapObject::Type(class)) => class.name.clone(),
                _ => unreachable!("class receiver was checked"),
            };
            return self.raise_error(
                "TypeError",
                format!("cannot delete '{name}' attribute of immutable type '{class_name}'"),
            );
        }
        if !class_receiver
            && let Some((_, descriptor)) = self.class_attribute_with_owner(class, name)
            && self.descriptor_is_data(descriptor)?
        {
            return self.descriptor_delete(descriptor, receiver);
        }
        let removed = match namespace.and_then(|namespace| self.heap.get_mut(namespace)) {
            Some(HeapObject::Dictionary(dictionary)) => dictionary.remove(name).is_some(),
            _ => false,
        };
        if removed {
            if class_receiver {
                self.invalidate_type_observers(receiver);
            }
            Ok(())
        } else {
            Err(format!("{label} has no attribute '{name}'"))
        }
    }

    pub(crate) fn namespace_value(&self, namespace: RValue, name: &str) -> Option<RValue> {
        match self.heap.get(namespace) {
            Some(HeapObject::Dictionary(dictionary)) => dictionary.get(name),
            Some(HeapObject::ValueDictionary(dictionary)) => {
                dictionary
                    .table
                    .values()
                    .find_map(|(key, value)| match self.heap.get(*key) {
                        Some(HeapObject::String(key)) if key == name => Some(*value),
                        _ => None,
                    })
            }
            _ => None,
        }
    }

    fn class_attribute(&self, class: RValue, name: &str) -> Option<RValue> {
        self.class_attribute_with_owner(class, name)
            .map(|(_, value)| value)
    }

    fn class_attribute_with_owner(&self, class: RValue, name: &str) -> Option<(RValue, RValue)> {
        let mro = match self.heap.get(class) {
            Some(HeapObject::Type(class)) => class.mro.to_vec(),
            _ => return None,
        };
        mro.into_iter()
            .find_map(|candidate| match self.heap.get(candidate) {
                Some(HeapObject::Type(class)) => self
                    .namespace_value(class.namespace, name)
                    .map(|value| (candidate, value)),
                _ => None,
            })
    }

    pub(crate) fn special_method(
        &mut self,
        value: RValue,
        name: &str,
    ) -> Result<Option<RValue>, String> {
        // Native suspended objects expose protocol slots through the runtime
        // rather than materializing Python methods in their builtin type
        // namespaces. Special-method lookup must still observe those slots,
        // while continuing to ignore arbitrary instance shadowing.
        if matches!(
            self.heap.get(value),
            Some(HeapObject::Generator(generator))
                if generator.kind == SuspendedKind::AsyncGenerator
                    && matches!(name, "__aiter__" | "__anext__")
        ) {
            return self.attribute_get(value, name).map(Some);
        }
        let class = self.type_of(value)?;
        let Some((_, method)) = self.class_attribute_with_owner(class, name) else {
            return Ok(None);
        };
        self.descriptor_get(method, Some(value), class).map(Some)
    }

    /// Slot-presence query used by reflection helpers such as `callable()`.
    /// It deliberately does not invoke descriptor binding.
    pub(crate) fn has_special_method_slot(
        &mut self,
        value: RValue,
        name: &str,
    ) -> Result<bool, String> {
        if matches!(
            self.heap.get(value),
            Some(HeapObject::Generator(generator))
                if generator.kind == SuspendedKind::AsyncGenerator
                    && matches!(name, "__aiter__" | "__anext__")
        ) {
            return Ok(true);
        }
        let class = self.type_of(value)?;
        Ok(self.class_attribute_with_owner(class, name).is_some())
    }

    pub(crate) fn invoke_special_method(
        &mut self,
        receiver: RValue,
        name: &str,
        arguments: &[RValue],
    ) -> Result<Option<RValue>, String> {
        let Some(method) = self.special_method(receiver, name)? else {
            return Ok(None);
        };
        let mut roots = Vec::with_capacity(arguments.len() + 2);
        roots.extend([receiver, method]);
        roots.extend_from_slice(arguments);
        self.with_temporary_roots(&roots, |context| {
            crate::call::invoke(context, method, arguments, &[]).map(Some)
        })
    }

    pub(crate) fn generic_binary(
        &mut self,
        op: u8,
        left: RValue,
        right: RValue,
    ) -> Result<Option<RValue>, String> {
        let (direct, reflected, symbol) = match op {
            0 => ("__add__", "__radd__", "+"),
            1 => ("__sub__", "__rsub__", "-"),
            2 => ("__mul__", "__rmul__", "*"),
            3 => ("__floordiv__", "__rfloordiv__", "//"),
            4 => ("__mod__", "__rmod__", "%"),
            5 => ("__truediv__", "__rtruediv__", "/"),
            6 => ("__pow__", "__rpow__", "**"),
            7 => ("__lshift__", "__rlshift__", "<<"),
            8 => ("__rshift__", "__rrshift__", ">>"),
            9 => ("__and__", "__rand__", "&"),
            10 => ("__xor__", "__rxor__", "^"),
            11 => ("__or__", "__ror__", "|"),
            12 => ("__matmul__", "__rmatmul__", "@"),
            _ => return Err("unknown binary operation".to_owned()),
        };
        let not_implemented = self.lookup_builtin("NotImplemented");
        let left_type = self.type_of(left)?;
        let right_type = self.type_of(right)?;
        let reflected_first = left_type != right_type
            && self.is_subclass(right_type, left_type)?
            && self
                .class_attribute_with_owner(right_type, reflected)
                .is_some_and(|(owner, _)| owner != left_type);
        let candidates = if reflected_first {
            [(right, left, reflected), (left, right, direct)]
        } else {
            [(left, right, direct), (right, left, reflected)]
        };
        let mut invoked = Vec::new();
        for (receiver, argument, method_name) in candidates {
            let Some(method) = self.special_method(receiver, method_name)? else {
                continue;
            };
            if invoked.contains(&method) {
                continue;
            }
            invoked.push(method);
            let result = self
                .with_temporary_roots(&[left, right, receiver, argument, method], |context| {
                    crate::call::invoke(context, method, &[argument], &[])
                })?;
            if Some(result) != not_implemented {
                return Ok(Some(result));
            }
        }
        let left_name = self.type_name(left_type);
        let right_name = self.type_name(right_type);
        self.raise_error(
            "TypeError",
            format!("unsupported operand type(s) for {symbol}: '{left_name}' and '{right_name}'"),
        )
    }

    pub(crate) fn generic_divmod(&mut self, left: RValue, right: RValue) -> Result<RValue, String> {
        let not_implemented = self.lookup_builtin("NotImplemented");
        let left_type = self.type_of(left)?;
        let right_type = self.type_of(right)?;
        let reflected_first = left_type != right_type
            && self.is_subclass(right_type, left_type)?
            && self
                .class_attribute_with_owner(right_type, "__rdivmod__")
                .is_some_and(|(owner, _)| owner != left_type);
        let candidates = if reflected_first {
            [(right, left, "__rdivmod__"), (left, right, "__divmod__")]
        } else {
            [(left, right, "__divmod__"), (right, left, "__rdivmod__")]
        };
        let mut invoked = Vec::new();
        for (receiver, argument, method_name) in candidates {
            let Some(method) = self.special_method(receiver, method_name)? else {
                continue;
            };
            if invoked.contains(&method) {
                continue;
            }
            invoked.push(method);
            let result = self
                .with_temporary_roots(&[left, right, receiver, argument, method], |context| {
                    crate::call::invoke(context, method, &[argument], &[])
                })?;
            if Some(result) != not_implemented {
                return Ok(result);
            }
        }
        self.raise_error(
            "TypeError",
            format!(
                "unsupported operand type(s) for divmod(): '{}' and '{}'",
                self.type_name(left_type),
                self.type_name(right_type)
            ),
        )
    }

    pub(crate) fn generic_ternary_power(
        &mut self,
        base: RValue,
        exponent: RValue,
        modulus: RValue,
    ) -> Result<RValue, String> {
        let base_type = self.type_of(base)?;
        let exponent_type = self.type_of(exponent)?;
        let modulus_type = self.type_of(modulus)?;
        let Some(method) = self.special_method(base, "__pow__")? else {
            return self.raise_error(
                "TypeError",
                format!(
                    "unsupported operand type(s) for ** or pow(): '{}', '{}' and '{}'",
                    self.type_name(base_type),
                    self.type_name(exponent_type),
                    self.type_name(modulus_type)
                ),
            );
        };
        let result = self.with_temporary_roots(&[base, exponent, modulus, method], |context| {
            crate::call::invoke(context, method, &[exponent, modulus], &[])
        })?;
        if Some(result) == self.lookup_builtin("NotImplemented") {
            return self.raise_error(
                "TypeError",
                format!(
                    "unsupported operand type(s) for ** or pow(): '{}', '{}' and '{}'",
                    self.type_name(base_type),
                    self.type_name(exponent_type),
                    self.type_name(modulus_type)
                ),
            );
        }
        Ok(result)
    }

    /// Returns an in-place protocol result when the receiver implements it.
    /// `NotImplemented` deliberately falls through to normal binary dispatch.
    pub(crate) fn generic_inplace(
        &mut self,
        op: u8,
        left: RValue,
        right: RValue,
    ) -> Result<Option<RValue>, String> {
        let method_name = match op {
            0 => "__iadd__",
            1 => "__isub__",
            2 => "__imul__",
            3 => "__ifloordiv__",
            4 => "__imod__",
            5 => "__itruediv__",
            6 => "__ipow__",
            7 => "__ilshift__",
            8 => "__irshift__",
            9 => "__iand__",
            10 => "__ixor__",
            11 => "__ior__",
            12 => "__imatmul__",
            _ => return Err("unknown binary operation".to_owned()),
        };
        let Some(method) = self.special_method(left, method_name)? else {
            return Ok(None);
        };
        let not_implemented = self.lookup_builtin("NotImplemented");
        let result = self.with_temporary_roots(&[left, right, method], |context| {
            crate::call::invoke(context, method, &[right], &[])
        })?;
        Ok((Some(result) != not_implemented).then_some(result))
    }

    pub(crate) fn generic_compare(
        &mut self,
        op: u8,
        left: RValue,
        right: RValue,
    ) -> Result<RValue, String> {
        if op == 8 {
            return Ok(RValue::boolean(left == right));
        }
        if op == 9 {
            return Ok(RValue::boolean(left != right));
        }
        let (direct, reflected, symbol) = match op {
            0 => ("__eq__", "__eq__", "=="),
            1 => ("__ne__", "__ne__", "!="),
            2 => ("__lt__", "__gt__", "<"),
            3 => ("__le__", "__ge__", "<="),
            4 => ("__gt__", "__lt__", ">"),
            5 => ("__ge__", "__le__", ">="),
            _ => return Err("unknown comparison operation".to_owned()),
        };
        let not_implemented = self.lookup_builtin("NotImplemented");
        let left_type = self.type_of(left)?;
        let right_type = self.type_of(right)?;
        let reflected_first = left_type != right_type
            && self.is_subclass(right_type, left_type)?
            && self
                .class_attribute_with_owner(right_type, reflected)
                .is_some_and(|(owner, _)| owner != left_type);
        let candidates = if reflected_first {
            [(right, left, reflected), (left, right, direct)]
        } else {
            [(left, right, direct), (right, left, reflected)]
        };
        let mut invoked = Vec::new();
        for (receiver, argument, method_name) in candidates {
            let Some(method) = self.special_method(receiver, method_name)? else {
                continue;
            };
            if invoked.contains(&method) {
                continue;
            }
            invoked.push(method);
            let result = self
                .with_temporary_roots(&[left, right, receiver, argument, method], |context| {
                    crate::call::invoke(context, method, &[argument], &[])
                })?;
            if Some(result) != not_implemented {
                return Ok(result);
            }
        }
        match op {
            0 => Ok(RValue::boolean(left == right)),
            1 => {
                let equal = self.generic_compare(0, left, right)?;
                let equal = crate::operations::truthy(self, equal)?;
                Ok(RValue::boolean(!equal))
            }
            _ => {
                let left_name = self.type_of(left).map(|value| self.type_name(value))?;
                let right_name = self.type_of(right).map(|value| self.type_name(value))?;
                self.raise_error(
                    "TypeError",
                    format!(
                        "'{}' not supported between instances of '{left_name}' and '{right_name}'",
                        symbol
                    ),
                )
            }
        }
    }

    pub(crate) fn generic_unary(&mut self, op: u8, operand: RValue) -> Result<RValue, String> {
        let (method_name, symbol) = match op {
            0 => ("__pos__", "+"),
            1 => ("__neg__", "-"),
            2 => ("__invert__", "~"),
            _ => return Err("unknown unary operation".to_owned()),
        };
        let Some(method) = self.special_method(operand, method_name)? else {
            let type_name = self.type_of(operand).map(|value| self.type_name(value))?;
            return self.raise_error(
                "TypeError",
                format!("bad operand type for unary {symbol}: '{type_name}'"),
            );
        };
        self.with_temporary_roots(&[operand, method], |context| {
            crate::call::invoke(context, method, &[], &[])
        })
    }

    fn descriptor_is_data(&mut self, value: RValue) -> Result<bool, String> {
        if matches!(
            self.heap.get(value),
            Some(HeapObject::Property(_) | HeapObject::MemberDescriptor(_))
        ) {
            return Ok(true);
        }
        Ok(self.special_method(value, "__set__")?.is_some()
            || self.special_method(value, "__delete__")?.is_some())
    }

    fn descriptor_get(
        &mut self,
        descriptor: RValue,
        instance: Option<RValue>,
        owner: RValue,
    ) -> Result<RValue, String> {
        match self.heap.get(descriptor) {
            Some(HeapObject::Function(_)) => {
                return instance.map_or(Ok(descriptor), |receiver| {
                    self.bind_method(descriptor, receiver)
                });
            }
            Some(HeapObject::StaticMethod(object)) => return Ok(object.callable),
            Some(HeapObject::ClassMethod(object)) => {
                return self.bind_method(object.callable, owner);
            }
            Some(HeapObject::Property(object)) => {
                let Some(receiver) = instance else {
                    return Ok(descriptor);
                };
                let getter = object
                    .getter
                    .ok_or_else(|| "property has no getter".to_owned())?;
                return crate::call::invoke(self, getter, &[receiver], &[]);
            }
            Some(HeapObject::MemberDescriptor(object)) => {
                let object = object.clone();
                let Some(receiver) = instance else {
                    return Ok(descriptor);
                };
                let Some(HeapObject::Instance(receiver)) = self.heap.get(receiver) else {
                    return Err("member descriptor requires an instance".to_owned());
                };
                if !matches!(self.heap.get(receiver.class), Some(HeapObject::Type(class)) if class.mro.contains(&object.owner))
                {
                    return Err(format!(
                        "descriptor '{}' for '{}' objects doesn't apply",
                        object.name,
                        self.type_name(object.owner)
                    ));
                }
                return receiver
                    .slots
                    .get(object.index)
                    .and_then(|value| *value)
                    .ok_or_else(|| {
                        format!(
                            "'{}' object has no attribute '{}'",
                            self.type_name(receiver.class),
                            object.name
                        )
                    });
            }
            _ => {}
        }
        let Some(getter) = self.special_method(descriptor, "__get__")? else {
            return Ok(descriptor);
        };
        crate::call::invoke(
            self,
            getter,
            &[instance.unwrap_or(RValue::NONE), owner],
            &[],
        )
    }

    fn descriptor_set(
        &mut self,
        descriptor: RValue,
        instance: RValue,
        value: RValue,
    ) -> Result<(), String> {
        if let Some(HeapObject::Property(object)) = self.heap.get(descriptor) {
            let setter = object
                .setter
                .ok_or_else(|| "property has no setter".to_owned())?;
            crate::call::invoke(self, setter, &[instance, value], &[])?;
            return Ok(());
        }
        if let Some(HeapObject::MemberDescriptor(object)) = self.heap.get(descriptor) {
            let object = object.clone();
            let Some(HeapObject::Instance(receiver)) = self.heap.get_mut(instance) else {
                return Err("member descriptor requires an instance".to_owned());
            };
            let Some(slot) = receiver.slots.get_mut(object.index) else {
                return Err("member descriptor layout is invalid".to_owned());
            };
            *slot = Some(value);
            return Ok(());
        }
        let setter = self
            .special_method(descriptor, "__set__")?
            .ok_or_else(|| "attribute is read-only".to_owned())?;
        crate::call::invoke(self, setter, &[instance, value], &[])?;
        Ok(())
    }

    fn descriptor_delete(&mut self, descriptor: RValue, instance: RValue) -> Result<(), String> {
        if let Some(HeapObject::Property(object)) = self.heap.get(descriptor) {
            let deleter = object
                .deleter
                .ok_or_else(|| "property has no deleter".to_owned())?;
            crate::call::invoke(self, deleter, &[instance], &[])?;
            return Ok(());
        }
        if let Some(HeapObject::MemberDescriptor(object)) = self.heap.get(descriptor) {
            let object = object.clone();
            let class_name = match self.heap.get(instance) {
                Some(HeapObject::Instance(receiver)) => self.type_name(receiver.class),
                _ => return Err("member descriptor requires an instance".to_owned()),
            };
            let Some(HeapObject::Instance(receiver)) = self.heap.get_mut(instance) else {
                return Err("member descriptor requires an instance".to_owned());
            };
            let Some(slot) = receiver.slots.get_mut(object.index) else {
                return Err("member descriptor layout is invalid".to_owned());
            };
            if slot.take().is_none() {
                return Err(format!(
                    "'{class_name}' object has no attribute '{}'",
                    object.name
                ));
            }
            return Ok(());
        }
        let deleter = self
            .special_method(descriptor, "__delete__")?
            .ok_or_else(|| "attribute cannot be deleted".to_owned())?;
        crate::call::invoke(self, deleter, &[instance], &[])?;
        Ok(())
    }

    pub(crate) fn new_property(
        &mut self,
        getter: Option<RValue>,
        setter: Option<RValue>,
        deleter: Option<RValue>,
    ) -> Result<RValue, String> {
        let roots = [getter, setter, deleter]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        self.with_temporary_roots(&roots, |context| {
            context.allocate(HeapObject::Property(PropertyObject {
                getter,
                setter,
                deleter,
            }))
        })
    }

    pub(crate) fn new_static_method(&mut self, callable: RValue) -> Result<RValue, String> {
        self.with_temporary_roots(&[callable], |context| {
            context.allocate(HeapObject::StaticMethod(StaticMethodObject { callable }))
        })
    }

    pub(crate) fn new_class_method(&mut self, callable: RValue) -> Result<RValue, String> {
        self.with_temporary_roots(&[callable], |context| {
            context.allocate(HeapObject::ClassMethod(ClassMethodObject { callable }))
        })
    }

    pub(crate) fn property_method(
        &mut self,
        property: RValue,
        kind: PropertyMethodKind,
    ) -> Result<RValue, String> {
        self.with_temporary_roots(&[property], |context| {
            context.allocate(HeapObject::PropertyMethod(PropertyMethodObject {
                property,
                kind,
            }))
        })
    }

    pub(crate) fn property_replace(
        &mut self,
        property: RValue,
        kind: PropertyMethodKind,
        callable: RValue,
    ) -> Result<RValue, String> {
        let Some(HeapObject::Property(existing)) = self.heap.get(property) else {
            return Err("property method receiver is invalid".to_owned());
        };
        let mut next = existing.clone();
        match kind {
            PropertyMethodKind::Getter => next.getter = Some(callable),
            PropertyMethodKind::Setter => next.setter = Some(callable),
            PropertyMethodKind::Deleter => next.deleter = Some(callable),
        }
        self.new_property(next.getter, next.setter, next.deleter)
    }

    fn bind_method(&mut self, function: RValue, receiver: RValue) -> Result<RValue, String> {
        if !matches!(
            self.heap.get(function),
            Some(HeapObject::Function(_) | HeapObject::BuiltinFunction(_))
        ) {
            return Ok(function);
        }
        self.with_temporary_roots(&[function, receiver], |context| {
            context.allocate(HeapObject::BoundMethod(BoundMethodObject {
                function,
                receiver,
            }))
        })
    }

    pub(crate) fn type_name(&self, type_value: RValue) -> String {
        match self.heap.get(type_value) {
            Some(HeapObject::Type(class)) => class.name.clone(),
            _ => "object".to_owned(),
        }
    }

    fn invalidate_type_observers(&mut self, type_value: RValue) {
        // Attribute/method/type-relation observations may be cached by the
        // mutated class itself, any subclass inheriting through it, or a class
        // whose metaclass inherits a mutated metaclass. Keep one version-tag
        // invalidation owner even though today's lookup path is deliberately
        // dynamic and does not yet retain a separate attribute cache.
        let affected = self
            .heap
            .live_values()
            .into_iter()
            .filter(|candidate| {
                let Some(HeapObject::Type(class)) = self.heap.get(*candidate) else {
                    return false;
                };
                if *candidate == type_value || class.mro.contains(&type_value) {
                    return true;
                }
                matches!(
                    self.heap.get(class.metaclass),
                    Some(HeapObject::Type(metaclass)) if metaclass.mro.contains(&type_value)
                )
            })
            .collect::<Vec<_>>();
        for value in affected {
            if let Some(HeapObject::Type(class)) = self.heap.get_mut(value) {
                class.version_tag = class.version_tag.wrapping_add(1);
            }
        }
    }

    fn set_class_bases(&mut self, class_value: RValue, value: RValue) -> Result<(), String> {
        let bases = match self.heap.get(value) {
            Some(HeapObject::Tuple(values)) => values.to_vec(),
            _ => return Err("can only assign tuple to __bases__, not other value".to_owned()),
        };
        if bases.is_empty() {
            return Err("can only assign non-empty tuple to __bases__".to_owned());
        }
        let (metaclass, old_slots, old_layout) = match self.heap.get(class_value) {
            Some(HeapObject::Type(class)) => {
                (class.metaclass, class.slot_names.to_vec(), class.layout)
            }
            _ => return Err("__bases__ assignment requires a type object".to_owned()),
        };
        if bases.contains(&class_value) {
            return Err("a __bases__ item causes an inheritance cycle".to_owned());
        }
        for (index, base) in bases.iter().copied().enumerate() {
            let Some(HeapObject::Type(base_type)) = self.heap.get(base) else {
                return Err("__bases__ items must be types".to_owned());
            };
            if bases[..index].contains(&base) {
                return Err(format!("duplicate base class {}", base_type.name));
            }
            if base_type.mro.contains(&class_value) {
                return Err("a __bases__ item causes an inheritance cycle".to_owned());
            }
            if !self.is_subclass(metaclass, base_type.metaclass)? {
                return Err("__bases__ assignment: metaclass conflict".to_owned());
            }
            if base_type.flags & TYPE_FLAG_BUILTIN != 0 {
                return Err(format!(
                    "__bases__ assignment: '{}' object layout differs from the new base",
                    base_type.name
                ));
            }
        }
        let mro_tail = self.compute_c3_mro(&bases)?;
        let mut new_slots = Vec::new();
        for base in &bases {
            let Some(HeapObject::Type(base_type)) = self.heap.get(*base) else {
                return Err("__bases__ items must be types".to_owned());
            };
            for slot in &base_type.slot_names {
                if !new_slots.contains(slot) {
                    new_slots.push(slot.clone());
                }
            }
        }
        if new_slots != old_slots {
            return Err("__bases__ assignment: object layout differs from the new base".to_owned());
        }
        if inherited_layout(&self.heap, &bases)? != old_layout {
            return Err("__bases__ assignment: object layout differs from the new base".to_owned());
        }
        let mut new_mro = Vec::with_capacity(mro_tail.len() + 1);
        new_mro.push(class_value);
        new_mro.extend(mro_tail);
        // Plan every descendant before touching the hierarchy. A C3 error in
        // a grandchild must leave the original bases and MROs observable.
        let mut descendants = Vec::new();
        self.collect_descendants(class_value, &mut descendants);
        let mut planned = vec![(class_value, new_mro)];
        for descendant in descendants {
            let descendant_bases = match self.heap.get(descendant) {
                Some(HeapObject::Type(class)) => class.bases.to_vec(),
                _ => continue,
            };
            let mut mro = vec![descendant];
            mro.extend(self.compute_c3_mro_planned(&descendant_bases, &planned)?);
            planned.push((descendant, mro));
        }
        if let Some(HeapObject::Type(class)) = self.heap.get_mut(class_value) {
            class.bases = bases.into_boxed_slice();
        } else {
            return Err("__bases__ assignment requires a type object".to_owned());
        }
        for (value, mro) in planned {
            if let Some(HeapObject::Type(class)) = self.heap.get_mut(value) {
                class.mro = mro.into_boxed_slice();
                class.version_tag = class.version_tag.wrapping_add(1);
            }
        }
        Ok(())
    }

    fn collect_descendants(&self, parent: RValue, output: &mut Vec<RValue>) {
        let children = self
            .heap
            .live_values()
            .into_iter()
            .filter(|candidate| matches!(self.heap.get(*candidate), Some(HeapObject::Type(class)) if class.bases.contains(&parent)))
            .collect::<Vec<_>>();
        for child in children {
            output.push(child);
            self.collect_descendants(child, output);
        }
    }

    pub(crate) fn globals(&self) -> Option<RValue> {
        self.module_namespaces
            .last()
            .copied()
            .or_else(|| self.kernel.as_ref().map(|kernel| kernel.globals))
    }

    fn global_lookup_cache_slot(callsite: usize) -> usize {
        // Generated constant-data addresses are stable for the executable. A
        // direct-mapped cache intentionally trades collisions for a tiny,
        // allocation-free hot path.
        (callsite.rotate_right(4) ^ (callsite >> 11)) & (GLOBAL_LOOKUP_CACHE_LEN - 1)
    }

    pub(crate) fn cached_global_lookup(&self, callsite: usize, name: &[u8]) -> Option<RValue> {
        let namespace = self.globals()?;
        let cached = self.global_lookup_cache[Self::global_lookup_cache_slot(callsite)];
        if !cached.valid || cached.callsite != callsite || cached.namespace != namespace {
            return None;
        }
        let Some(HeapObject::Dictionary(dictionary)) = self.heap.get(namespace) else {
            return None;
        };
        let (key, value) = dictionary.entries.get(cached.entry_index)?;
        // Never cache the value itself: rebinding an existing global replaces
        // the value in-place, so this read always observes current Python state.
        (key.as_bytes() == name).then_some(*value)
    }

    pub(crate) fn remember_global_lookup(
        &mut self,
        callsite: usize,
        namespace: RValue,
        entry_index: usize,
    ) {
        let slot = Self::global_lookup_cache_slot(callsite);
        self.global_lookup_cache[slot] = GlobalLookupCacheEntry {
            callsite,
            namespace,
            entry_index,
            valid: true,
        };
    }

    pub(crate) fn push_module_namespace(&mut self, namespace: RValue) {
        self.module_namespaces.push(namespace);
    }

    pub(crate) fn pop_module_namespace(&mut self, expected: RValue) {
        let popped = self.module_namespaces.pop();
        debug_assert_eq!(popped, Some(expected));
    }

    fn module_cache(&self) -> Result<RValue, String> {
        self.kernel
            .as_ref()
            .map(|kernel| kernel.module_cache)
            .ok_or_else(|| "module cache is unavailable before kernel initialization".to_owned())
    }

    fn cached_module(&self, name: &str) -> Option<RValue> {
        let cache = self.kernel.as_ref()?.module_cache;
        match self.heap.get(cache) {
            Some(HeapObject::Dictionary(dictionary)) => dictionary.get(name),
            _ => None,
        }
    }

    fn cache_module(&mut self, name: &str, value: RValue) -> Result<(), String> {
        let cache = self.module_cache()?;
        let growth = match self.heap.get(cache) {
            Some(HeapObject::Dictionary(dictionary)) => dictionary.managed_growth_for_insert(name),
            _ => return Err("module cache is corrupt".to_owned()),
        };
        self.with_temporary_roots(&[cache, value], |context| {
            context.preflight_managed_growth(growth)
        })?;
        let Some(HeapObject::Dictionary(dictionary)) = self.heap.get_mut(cache) else {
            return Err("module cache is corrupt".to_owned());
        };
        dictionary.insert(name.to_owned(), value);
        self.heap.refresh_managed_bytes();
        Ok(())
    }

    fn remove_cached_module_if(&mut self, name: &str, expected: RValue) {
        let Ok(cache) = self.module_cache() else {
            return;
        };
        let Some(HeapObject::Dictionary(dictionary)) = self.heap.get_mut(cache) else {
            return;
        };
        if dictionary.get(name) == Some(expected) {
            dictionary.remove(name);
            self.heap.refresh_managed_bytes();
        }
    }

    fn initialize_sys_module(&mut self) -> Result<(), String> {
        if self.cached_module("sys").is_some() {
            return Ok(());
        }
        let cache = self.module_cache()?;
        let namespace = self.allocate(HeapObject::Dictionary(DictionaryObject {
            entries: Vec::new(),
        }))?;
        self.with_temporary_roots(&[cache, namespace], |context| {
            let name = crate::operations::string(context, "sys")?;
            let package = crate::operations::string(context, "")?;
            context.with_temporary_roots(&[cache, namespace, name, package], |context| {
                context.namespace_set(namespace, "__name__", name)?;
                context.namespace_set(namespace, "__package__", package)?;
                context.namespace_set(namespace, "__loader__", RValue::NONE)?;
                context.namespace_set(namespace, "__spec__", RValue::NONE)?;
                context.namespace_set(namespace, "modules", cache)?;
                let module = context.allocate(HeapObject::Module(ModuleObject {
                    name: "sys".to_owned(),
                    namespace,
                    state: ModuleState::Created,
                }))?;
                context.with_temporary_roots(&[cache, namespace, module], |context| {
                    context.transition_module(module, ModuleState::Initializing)?;
                    context.cache_module("sys", module)?;
                    context.transition_module(module, ModuleState::Ready)
                })
            })
        })
    }

    fn initialize_builtins_module(&mut self) -> Result<(), String> {
        if self.cached_module("builtins").is_some() {
            return Ok(());
        }
        let namespace = self
            .builtins()
            .ok_or_else(|| "builtins namespace is unavailable".to_owned())?;
        self.with_temporary_roots(&[namespace], |context| {
            let name = crate::operations::string(context, "builtins")?;
            let package = crate::operations::string(context, "")?;
            context.with_temporary_roots(&[namespace, name, package], |context| {
                context.namespace_set(namespace, "__name__", name)?;
                context.namespace_set(namespace, "__package__", package)?;
                context.namespace_set(namespace, "__loader__", RValue::NONE)?;
                context.namespace_set(namespace, "__spec__", RValue::NONE)?;
                let module = context.allocate(HeapObject::Module(ModuleObject {
                    name: "builtins".to_owned(),
                    namespace,
                    state: ModuleState::Created,
                }))?;
                context.with_temporary_roots(&[namespace, module], |context| {
                    context.transition_module(module, ModuleState::Initializing)?;
                    context.cache_module("builtins", module)?;
                    context.transition_module(module, ModuleState::Ready)
                })
            })
        })
    }

    pub(crate) fn register_source_module(
        &mut self,
        name: &str,
        filename: &str,
        package: &str,
        is_package: bool,
        initializer: RNativeModuleInitializer,
    ) -> Result<(), String> {
        if name.is_empty() {
            return Err("module definition requires a non-empty canonical name".to_owned());
        }
        let definition = NativeModuleDefinition::Source {
            filename: filename.to_owned(),
            package: package.to_owned(),
            is_package,
            initializer,
        };
        self.module_registry.register(name, definition)
    }

    pub(crate) fn register_namespace_module(
        &mut self,
        name: &str,
        search_locations: &[&str],
    ) -> Result<(), String> {
        if name.is_empty() || search_locations.is_empty() {
            return Err(
                "namespace module definition requires a name and search location".to_owned(),
            );
        }
        let definition = NativeModuleDefinition::Namespace {
            search_locations: search_locations
                .iter()
                .map(|location| (*location).to_owned())
                .collect(),
        };
        self.module_registry.register(name, definition)
    }

    pub(crate) fn import_registered(&mut self, name: &str) -> Result<RValue, String> {
        self.initialize_kernel()?;
        if let Some(module) = self.cached_module(name) {
            if module == RValue::NONE {
                return self.raise_error(
                    "ModuleNotFoundError",
                    format!("import of {name} halted; None in sys.modules"),
                );
            }
            return Ok(module);
        }
        match self.module_registry.get(name).as_deref() {
            Some(NativeModuleDefinition::Source {
                filename,
                package,
                is_package,
                initializer,
            }) => self.import_source(name, filename, package, *is_package, *initializer),
            Some(NativeModuleDefinition::Namespace { search_locations }) => {
                let locations = search_locations
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>();
                self.import_namespace(name, &locations)
            }
            None => self.import_name(name),
        }
    }

    pub(crate) fn import_builtin_target(
        &mut self,
        name: &str,
        globals: RValue,
        fromlist_is_empty: bool,
        level: usize,
    ) -> Result<(String, RValue), String> {
        let absolute = if level == 0 {
            if name.is_empty() {
                return self.raise_error("ValueError", "Empty module name");
            }
            name.to_owned()
        } else {
            if globals == RValue::NONE {
                return self.raise_error("KeyError", "'__name__' not in globals");
            }
            let package = self
                .namespace_value(globals, "__package__")
                .or_else(|| self.namespace_value(globals, "__name__"))
                .and_then(|value| crate::operations::string_value(self, value))
                .map(str::to_owned)
                .ok_or_else(|| {
                    "globals must contain a string __package__ or __name__".to_owned()
                })?;
            let mut parts = package.split('.').collect::<Vec<_>>();
            if level > parts.len() {
                return self.raise_error(
                    "ImportError",
                    "attempted relative import beyond top-level package",
                );
            }
            parts.truncate(parts.len() + 1 - level);
            if !name.is_empty() {
                parts.extend(name.split('.'));
            }
            parts.join(".")
        };

        let mut leaf = RValue::NONE;
        let mut prefix = String::new();
        for component in absolute.split('.') {
            if component.is_empty() {
                return self.raise_error(
                    "ModuleNotFoundError",
                    format!("No module named '{absolute}'"),
                );
            }
            if !prefix.is_empty() {
                prefix.push('.');
            }
            prefix.push_str(component);
            leaf = self.import_registered(&prefix)?;
        }
        if fromlist_is_empty {
            let top = absolute.split('.').next().unwrap_or(&absolute).to_owned();
            Ok((absolute, self.import_registered(&top)?))
        } else {
            Ok((absolute, leaf))
        }
    }

    pub(crate) fn has_module_definition(&self, name: &str) -> bool {
        self.module_registry.contains(name)
    }

    pub(crate) fn module_has_path(&self, module: RValue) -> bool {
        let Some(HeapObject::Module(module)) = self.heap.get(module) else {
            return false;
        };
        self.namespace_value(module.namespace, "__path__").is_some()
    }

    pub(crate) fn register_module_resource(
        &mut self,
        module: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        if !self.module_registry.contains(module) {
            return Err(format!(
                "resource owner '{module}' is not in the compiled graph"
            ));
        }
        self.module_registry.register_resource(module, name, bytes)
    }

    pub(crate) fn read_module_resource(
        &mut self,
        package: RValue,
        name: &str,
    ) -> Result<RValue, String> {
        let module = match self.heap.get(package) {
            Some(HeapObject::Module(module)) => module.name.clone(),
            Some(HeapObject::String(module)) => module.clone(),
            _ => return self.raise_error("TypeError", "package must be a module or module name"),
        };
        let bytes = self
            .module_registry
            .resource(&module, name)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| format!("resource '{name}' does not exist in package '{module}'"))?;
        crate::operations::bytes(self, &bytes)
    }

    pub(crate) fn reload_module(&mut self, module: RValue) -> Result<RValue, String> {
        let (name, namespace) = match self.heap.get(module) {
            Some(HeapObject::Module(module)) => (module.name.clone(), module.namespace),
            _ => {
                return self.raise_error("TypeError", "reload() argument must be a module");
            }
        };
        if self.cached_module(&name) != Some(module) {
            return self.raise_error("ImportError", format!("module {name} not in sys.modules"));
        }
        let Some(definition) = self.module_registry.get(&name) else {
            return self.raise_error(
                "ModuleNotFoundError",
                format!("spec not found for the module '{name}'"),
            );
        };
        match definition.as_ref() {
            NativeModuleDefinition::Namespace { .. } => Ok(module),
            NativeModuleDefinition::Source { initializer, .. } => {
                self.transition_module(module, ModuleState::Initializing)?;
                self.push_module_namespace(namespace);
                let arguments = RValue::NONE;
                let mut output = RValue::NONE;
                let status = unsafe {
                    (*initializer)(
                        (self as *mut Self).cast(),
                        &raw const module,
                        &raw const arguments,
                        0,
                        &raw mut output,
                    )
                };
                self.pop_module_namespace(namespace);
                self.transition_module(module, ModuleState::Ready)?;
                if status == RStatus::Ok {
                    Ok(module)
                } else {
                    Err("native module reload failed".to_owned())
                }
            }
        }
    }

    pub(crate) fn import_name(&mut self, name: &str) -> Result<RValue, String> {
        self.initialize_kernel()?;
        if let Some(module) = self.cached_module(name) {
            if module == RValue::NONE {
                return self.raise_error(
                    "ModuleNotFoundError",
                    format!("import of {name} halted; None in sys.modules"),
                );
            }
            return Ok(module);
        }
        if !matches!(
            name,
            "builtins"
                | "importlib"
                | "importlib.resources"
                | "inspect"
                | "weakref"
                | "sys"
                | "rimera"
                | "rimera.async_runtime"
        ) {
            return self.raise_error("ModuleNotFoundError", format!("No module named '{name}'"));
        }

        let namespace = self.allocate(HeapObject::Dictionary(DictionaryObject {
            entries: Vec::new(),
        }))?;
        self.with_temporary_roots(&[namespace], |context| {
            let module_name = crate::operations::string(context, name)?;
            context.with_temporary_roots(&[namespace, module_name], |context| {
                let module = context.allocate(HeapObject::Module(ModuleObject {
                    name: name.to_owned(),
                    namespace,
                    state: ModuleState::Created,
                }))?;
                context.with_temporary_roots(&[namespace, module], |context| {
                    context.transition_module(module, ModuleState::Initializing)?;
                    let initialized = (|| {
                        context.namespace_set(namespace, "__name__", module_name)?;
                        context.namespace_set(namespace, "__package__", RValue::NONE)?;
                        context.namespace_set(namespace, "__loader__", RValue::NONE)?;
                        context.namespace_set(namespace, "__spec__", RValue::NONE)?;
                        if name == "importlib" {
                            let reload = context.allocate(HeapObject::BuiltinFunction(
                                BuiltinFunctionObject {
                                    name: "reload".to_owned(),
                                    kind: BuiltinFunctionKind::Reload,
                                },
                            ))?;
                            context.namespace_set(namespace, "reload", reload)?;
                            let invalidate = context.allocate(HeapObject::BuiltinFunction(
                                BuiltinFunctionObject {
                                    name: "invalidate_caches".to_owned(),
                                    kind: BuiltinFunctionKind::InvalidateImportCaches,
                                },
                            ))?;
                            context.namespace_set(namespace, "invalidate_caches", invalidate)?;
                        } else if name == "importlib.resources" {
                            let read_binary = context.allocate(HeapObject::BuiltinFunction(
                                BuiltinFunctionObject {
                                    name: "read_binary".to_owned(),
                                    kind: BuiltinFunctionKind::ReadBinaryResource,
                                },
                            ))?;
                            context.namespace_set(namespace, "read_binary", read_binary)?;
                        } else if name == "rimera.async_runtime" {
                            let run = context.allocate(HeapObject::BuiltinFunction(
                                BuiltinFunctionObject {
                                    name: "run".to_owned(),
                                    kind: BuiltinFunctionKind::AsyncRuntimeRun,
                                },
                            ))?;
                            context.namespace_set(namespace, "run", run)?;
                        }
                        Ok(())
                    })();
                    match initialized {
                        Ok(()) => {
                            context.cache_module(name, module)?;
                            if let Err(error) = context.publish_module_to_parent(name, module) {
                                context.transition_module(module, ModuleState::Failed)?;
                                context.remove_cached_module_if(name, module);
                                return Err(error);
                            }
                            context.transition_module(module, ModuleState::Ready)?;
                            Ok(module)
                        }
                        Err(cause) => {
                            context.transition_module(module, ModuleState::Failed)?;
                            Err(cause)
                        }
                    }
                })
            })
        })
    }

    pub(crate) fn import_source(
        &mut self,
        name: &str,
        filename: &str,
        package: &str,
        is_package: bool,
        initializer: RNativeModuleInitializer,
    ) -> Result<RValue, String> {
        self.initialize_kernel()?;
        if let Some(module) = self.cached_module(name) {
            if module == RValue::NONE {
                return self.raise_error(
                    "ModuleNotFoundError",
                    format!("import of {name} halted; None in sys.modules"),
                );
            }
            return Ok(module);
        }

        let namespace = self.allocate(HeapObject::Dictionary(DictionaryObject {
            entries: Vec::new(),
        }))?;
        self.with_temporary_roots(&[namespace], |context| {
            context.initialize_source_module_metadata(
                namespace, name, filename, package, is_package,
            )?;
            context.with_temporary_roots(&[namespace], |context| {
                let module = context.allocate(HeapObject::Module(ModuleObject {
                    name: name.to_owned(),
                    namespace,
                    state: ModuleState::Created,
                }))?;
                context.with_temporary_roots(&[namespace, module], |context| {
                    context.transition_module(module, ModuleState::Initializing)?;
                    context.cache_module(name, module)?;
                    if let Err(error) = context.publish_module_to_parent(name, module) {
                        context.transition_module(module, ModuleState::Failed)?;
                        context.remove_cached_module_if(name, module);
                        return Err(error);
                    }
                    context.push_module_namespace(namespace);
                    let arguments = RValue::NONE;
                    let mut output = RValue::NONE;
                    // SAFETY: codegen registers only an exported initializer
                    // with the documented five-argument native signature.
                    let status = unsafe {
                        initializer(
                            (context as *mut Self).cast(),
                            &raw const module,
                            &raw const arguments,
                            0,
                            &raw mut output,
                        )
                    };
                    context.pop_module_namespace(namespace);
                    if status == RStatus::Ok {
                        let Some(published) = context.cached_module(name) else {
                            context.transition_module(module, ModuleState::Failed)?;
                            context.remove_module_from_parent_if(name, module);
                            return context.raise_error("KeyError", format!("'{name}'"));
                        };
                        if published == RValue::NONE {
                            context.transition_module(module, ModuleState::Failed)?;
                            context.remove_module_from_parent_if(name, module);
                            return context.raise_error(
                                "ModuleNotFoundError",
                                format!("import of {name} halted; None in sys.modules"),
                            );
                        }
                        if let Err(error) = context.publish_module_to_parent(name, published) {
                            context.transition_module(module, ModuleState::Failed)?;
                            context.remove_cached_module_if(name, module);
                            context.remove_module_from_parent_if(name, module);
                            return Err(error);
                        }
                        context.transition_module(module, ModuleState::Ready)?;
                        Ok(published)
                    } else {
                        context.transition_module(module, ModuleState::Failed)?;
                        context.remove_cached_module_if(name, module);
                        context.remove_module_from_parent_if(name, module);
                        Err("native module initializer failed".to_owned())
                    }
                })
            })
        })
    }

    pub(crate) fn import_namespace(
        &mut self,
        name: &str,
        search_locations: &[&str],
    ) -> Result<RValue, String> {
        self.initialize_kernel()?;
        if let Some(module) = self.cached_module(name) {
            if module == RValue::NONE {
                return self.raise_error(
                    "ModuleNotFoundError",
                    format!("import of {name} halted; None in sys.modules"),
                );
            }
            return Ok(module);
        }
        if search_locations.is_empty() {
            return self.raise_error("ModuleNotFoundError", format!("No module named '{name}'"));
        }

        let namespace = self.allocate(HeapObject::Dictionary(DictionaryObject {
            entries: Vec::new(),
        }))?;
        self.with_temporary_roots(&[namespace], |context| {
            context.initialize_namespace_module_metadata(namespace, name, search_locations)?;
            context.with_temporary_roots(&[namespace], |context| {
                let module = context.allocate(HeapObject::Module(ModuleObject {
                    name: name.to_owned(),
                    namespace,
                    state: ModuleState::Created,
                }))?;
                context.with_temporary_roots(&[namespace, module], |context| {
                    context.transition_module(module, ModuleState::Initializing)?;
                    context.cache_module(name, module)?;
                    if let Err(error) = context.publish_module_to_parent(name, module) {
                        context.transition_module(module, ModuleState::Failed)?;
                        context.remove_cached_module_if(name, module);
                        return Err(error);
                    }
                    context.transition_module(module, ModuleState::Ready)?;
                    Ok(module)
                })
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn import_from(
        &mut self,
        module: RValue,
        name: &str,
        filename: &str,
        package: &str,
        is_package: bool,
        initializer: Option<RNativeModuleInitializer>,
        search_locations: &[&str],
    ) -> Result<RValue, String> {
        let module_name = match self.heap.get(module) {
            Some(HeapObject::Module(module)) => module.name.clone(),
            _ => return self.raise_error("TypeError", "from-import target is not a module"),
        };
        if let Ok(value) = self.attribute_get(module, name) {
            return Ok(value);
        }
        if let Some(initializer) = initializer {
            let submodule = format!("{module_name}.{name}");
            return self.with_temporary_roots(&[module], |context| {
                context.import_source(&submodule, filename, package, is_package, initializer)
            });
        }
        if !search_locations.is_empty() {
            let submodule = format!("{module_name}.{name}");
            return self.with_temporary_roots(&[module], |context| {
                context.import_namespace(&submodule, search_locations)
            });
        }
        let submodule = format!("{module_name}.{name}");
        if submodule == "rimera.async_runtime" {
            return self
                .with_temporary_roots(&[module], |context| context.import_registered(&submodule));
        }
        self.raise_error(
            "ImportError",
            format!("cannot import name '{name}' from '{module_name}'"),
        )
    }

    fn initialize_source_module_metadata(
        &mut self,
        namespace: RValue,
        name: &str,
        filename: &str,
        package: &str,
        is_package: bool,
    ) -> Result<(), String> {
        let module_name = crate::operations::string(self, name)?;
        self.namespace_set(namespace, "__name__", module_name)?;
        let filename_value = crate::operations::string(self, filename)?;
        self.namespace_set(namespace, "__file__", filename_value)?;
        let package_value = crate::operations::string(self, package)?;
        self.namespace_set(namespace, "__package__", package_value)?;

        let search_path = if is_package {
            let directory = std::path::Path::new(filename)
                .parent()
                .map_or_else(String::new, |path| path.display().to_string());
            let directory = crate::operations::string(self, &directory)?;
            self.with_temporary_roots(&[namespace, directory], |context| {
                crate::operations::list(context, &[directory])
            })?
        } else {
            RValue::NONE
        };
        if is_package {
            self.namespace_set(namespace, "__path__", search_path)?;
        }

        let loader_type = self
            .ensure_builtin_type("SourceFileLoader")?
            .ok_or_else(|| "SourceFileLoader type is unavailable".to_owned())?;
        let loader = self.new_instance(loader_type)?;
        self.namespace_set(namespace, "__loader__", loader)?;
        self.attribute_set(loader, "name", module_name)?;
        self.attribute_set(loader, "path", filename_value)?;

        let spec_type = self
            .ensure_builtin_type("ModuleSpec")?
            .ok_or_else(|| "ModuleSpec type is unavailable".to_owned())?;
        let spec = self.new_instance(spec_type)?;
        self.namespace_set(namespace, "__spec__", spec)?;
        self.attribute_set(spec, "name", module_name)?;
        self.attribute_set(spec, "loader", loader)?;
        self.attribute_set(spec, "origin", filename_value)?;
        self.attribute_set(spec, "parent", package_value)?;
        self.attribute_set(spec, "submodule_search_locations", search_path)?;
        self.attribute_set(spec, "has_location", RValue::boolean(true))?;
        Ok(())
    }

    fn initialize_namespace_module_metadata(
        &mut self,
        namespace: RValue,
        name: &str,
        search_locations: &[&str],
    ) -> Result<(), String> {
        let module_name = crate::operations::string(self, name)?;
        self.with_temporary_roots(&[namespace, module_name], |context| {
            context.namespace_set(namespace, "__name__", module_name)?;
            context.namespace_set(namespace, "__package__", module_name)?;
            context.namespace_set(namespace, "__file__", RValue::NONE)?;

            let mut location_values = Vec::with_capacity(search_locations.len());
            for location in search_locations {
                let value = context.with_temporary_roots(&location_values, |context| {
                    crate::operations::string(context, location)
                })?;
                location_values.push(value);
            }
            context.with_temporary_roots(&location_values, |context| {
                let search_path = crate::operations::list(context, &location_values)?;
                context.with_temporary_roots(&[search_path], |context| {
                    context.namespace_set(namespace, "__path__", search_path)?;

                    let loader_type = context
                        .ensure_builtin_type("NamespaceLoader")?
                        .ok_or_else(|| "NamespaceLoader type is unavailable".to_owned())?;
                    let loader = context.new_instance(loader_type)?;
                    context.with_temporary_roots(&[search_path, loader], |context| {
                        context.namespace_set(namespace, "__loader__", loader)?;
                        context.attribute_set(loader, "name", module_name)?;

                        let spec_type = context
                            .ensure_builtin_type("ModuleSpec")?
                            .ok_or_else(|| "ModuleSpec type is unavailable".to_owned())?;
                        let spec = context.new_instance(spec_type)?;
                        context.with_temporary_roots(&[search_path, loader, spec], |context| {
                            context.namespace_set(namespace, "__spec__", spec)?;
                            context.attribute_set(spec, "name", module_name)?;
                            context.attribute_set(spec, "loader", loader)?;
                            context.attribute_set(spec, "origin", RValue::NONE)?;
                            context.attribute_set(spec, "parent", module_name)?;
                            context.attribute_set(
                                spec,
                                "submodule_search_locations",
                                search_path,
                            )?;
                            context.attribute_set(spec, "has_location", RValue::boolean(false))
                        })
                    })
                })
            })
        })
    }

    fn publish_module_to_parent(&mut self, name: &str, module: RValue) -> Result<(), String> {
        let Some((parent_name, child_name)) = name.rsplit_once('.') else {
            return Ok(());
        };
        let parent = self
            .cached_module(parent_name)
            .ok_or_else(|| "import parent is not initialized".to_owned())?;
        let parent_namespace = match self.heap.get(parent) {
            Some(HeapObject::Module(parent)) => parent.namespace,
            _ => return Err("import parent is not a module".to_owned()),
        };
        self.namespace_set(parent_namespace, child_name, module)
    }

    fn remove_module_from_parent_if(&mut self, name: &str, expected: RValue) {
        let Some((parent_name, child_name)) = name.rsplit_once('.') else {
            return;
        };
        let Some(parent) = self.cached_module(parent_name) else {
            return;
        };
        let namespace = match self.heap.get(parent) {
            Some(HeapObject::Module(parent)) => parent.namespace,
            _ => return,
        };
        let Some(HeapObject::Dictionary(dictionary)) = self.heap.get_mut(namespace) else {
            return;
        };
        if dictionary.get(child_name) == Some(expected) {
            dictionary.remove(child_name);
        }
    }

    pub(crate) fn import_star(&mut self, module: RValue) -> Result<RValue, String> {
        let (module_name, namespace) = match self.heap.get(module) {
            Some(HeapObject::Module(module)) => (module.name.clone(), module.namespace),
            _ => return self.raise_error("TypeError", "star-import target is not a module"),
        };
        let globals = self
            .globals()
            .ok_or_else(|| "module globals are unavailable".to_owned())?;
        if let Some(all) = self.namespace_value(namespace, "__all__") {
            let iterator = self.with_temporary_roots(&[module, all, globals], |context| {
                crate::operations::import_names_iterator(context, all)
            })?;
            return self.with_temporary_roots(&[module, namespace, globals, iterator], |context| {
                loop {
                    let Some(value) = crate::operations::iterator_next(context, iterator)? else {
                        return Ok(RValue::NONE);
                    };
                    context.with_temporary_roots(&[value], |context| {
                        let Some(name) = crate::operations::string_value(context, value) else {
                            let value_type = context.type_of(value)?;
                            let type_name = context.type_name(value_type);
                            return context.raise_error(
                                "TypeError",
                                format!(
                                    "Item in {module_name}.__all__ must be str, not {type_name}"
                                ),
                            );
                        };
                        let name = name.to_owned();
                        let value = match context.attribute_get(module, &name) {
                            Ok(value) => value,
                            Err(error) => return context.raise_error("AttributeError", error),
                        };
                        context.namespace_set(globals, &name, value)
                    })?;
                }
            });
        }
        let names = match self.heap.get(namespace) {
            Some(HeapObject::Dictionary(dictionary)) => dictionary
                .entries
                .iter()
                .map(|(name, _)| name.clone())
                .filter(|name| !name.starts_with('_'))
                .collect::<Vec<_>>(),
            _ => return Err("module namespace is invalid".to_owned()),
        };
        self.with_temporary_roots(&[module, namespace, globals], |context| {
            for name in names {
                let value = match context.attribute_get(module, &name) {
                    Ok(value) => value,
                    Err(error) => return context.raise_error("AttributeError", error),
                };
                context.namespace_set(globals, &name, value)?;
            }
            Ok(RValue::NONE)
        })
    }

    pub(crate) fn transition_module(
        &mut self,
        module: RValue,
        next: ModuleState,
    ) -> Result<(), String> {
        let Some(HeapObject::Module(module)) = self.heap.get_mut(module) else {
            return Err("module state transition requires a module object".to_owned());
        };
        let legal = matches!(
            (&module.state, &next),
            (ModuleState::Created, ModuleState::Initializing)
                | (ModuleState::Ready, ModuleState::Initializing)
                | (
                    ModuleState::Initializing,
                    ModuleState::Ready | ModuleState::Failed
                )
        );
        if !legal {
            return Err(format!(
                "illegal module state transition from {:?} to {next:?}",
                module.state
            ));
        }
        module.state = next;
        Ok(())
    }

    pub(crate) fn builtins(&self) -> Option<RValue> {
        self.kernel.as_ref().map(|kernel| kernel.builtins)
    }

    pub(crate) fn new_builtin_exception(
        &mut self,
        exception_type: &str,
        arguments: &[RValue],
    ) -> Result<RValue, String> {
        self.initialize_kernel()?;
        let type_value = self
            .ensure_builtin_type(exception_type)?
            .ok_or_else(|| format!("unknown built-in exception type `{exception_type}`"))?;
        self.new_exception(type_value, arguments)
    }

    pub(crate) fn raise_stop_iteration(&mut self, value: RValue) -> Result<RValue, String> {
        let arguments = if value == RValue::NONE {
            Vec::new()
        } else {
            vec![value]
        };
        let exception = self.with_temporary_roots(&arguments, |context| {
            context.new_builtin_exception("StopIteration", &arguments)
        })?;
        self.raise_value(exception, None, false)?;
        Ok(exception)
    }

    pub(crate) fn raise_builtin(
        &mut self,
        exception_type: &str,
        message: impl Into<String>,
    ) -> Result<RValue, String> {
        self.initialize_kernel()?;
        let message = message.into();
        let type_value = self
            .ensure_builtin_type(exception_type)?
            .ok_or_else(|| format!("unknown built-in exception type `{exception_type}`"))?;
        let text = self.allocate(HeapObject::String(message.clone()))?;
        let exception = self.new_exception(type_value, &[text])?;
        self.raise_value(exception, None, false)?;
        self.exception = Some(message);
        Ok(exception)
    }

    /// Raises a concrete managed Python exception and returns the same message
    /// through the legacy Rust `Result` channel used by runtime operations.
    /// The managed exception state is authoritative; FFI callers must preserve
    /// it instead of reclassifying the message text.
    pub(crate) fn raise_error<T>(
        &mut self,
        exception_type: &str,
        message: impl Into<String>,
    ) -> Result<T, String> {
        let message = message.into();
        self.raise_builtin(exception_type, message.clone())?;
        Err(message)
    }

    /// Consumes a pending `StopIteration` raised by an iterator protocol call.
    ///
    /// The iterator protocol represents normal exhaustion internally; public
    /// callers such as `next()` turn that result back into `StopIteration`.
    pub(crate) fn consume_stop_iteration(&mut self) -> bool {
        self.consume_exception_type("StopIteration")
    }

    /// Consumes one pending exception of a known internal protocol type.
    /// This is used for class-namespace mapping lookup, where `KeyError`
    /// means "continue with global/builtin lookup" rather than failure.
    pub(crate) fn consume_exception_type(&mut self, expected: &str) -> bool {
        let Some(exception) = self.raised else {
            return false;
        };
        if self.exception_type_name(exception) != Some(expected) {
            return false;
        }
        self.raised = None;
        self.exception = None;
        true
    }

    pub(crate) fn raise_emergency_memory_error(&mut self) {
        if let Some(exception) = self
            .kernel
            .as_ref()
            .map(|kernel| kernel.emergency_memory_error)
        {
            self.raised = Some(exception);
            self.exception = Some(HEAP_LIMIT_MESSAGE.to_owned());
        } else {
            self.fail(HEAP_LIMIT_MESSAGE);
        }
    }

    pub(crate) fn new_exception(
        &mut self,
        exception_type: RValue,
        arguments: &[RValue],
    ) -> Result<RValue, String> {
        let Some(HeapObject::Type(actual_type)) = self.heap.get(exception_type) else {
            return Err("exceptions must derive from BaseException".to_owned());
        };
        let base_exception = self
            .builtin_type("BaseException")
            .ok_or_else(|| "exception kernel is not initialized".to_owned())?;
        if !actual_type.mro.contains(&base_exception) {
            return Err("exceptions must derive from BaseException".to_owned());
        }
        let mut roots = Vec::with_capacity(arguments.len() + 1);
        roots.push(exception_type);
        roots.extend_from_slice(arguments);
        self.with_temporary_roots(&roots, |context| {
            let arguments =
                context.allocate(HeapObject::Tuple(arguments.to_vec().into_boxed_slice()))?;
            context.with_temporary_roots(&[exception_type, arguments], |context| {
                context.allocate(HeapObject::Exception(ExceptionObject {
                    exception_type,
                    arguments,
                    dictionary: None,
                    traceback: None,
                    cause: None,
                    context: None,
                    suppress_context: false,
                    group_message: None,
                    group_exceptions: None,
                    group_origin: None,
                }))
            })
        })
    }

    pub(crate) fn new_exception_group(
        &mut self,
        exception_type: RValue,
        message: RValue,
        exceptions: &[RValue],
    ) -> Result<RValue, String> {
        let Some(HeapObject::String(message_text)) = self.heap.get(message) else {
            return Err("exception group message must be a string".to_owned());
        };
        let message_text = message_text.clone();
        if exceptions.is_empty() {
            return Err("exception group must contain at least one exception".to_owned());
        }
        if exceptions
            .iter()
            .any(|value| !matches!(self.heap.get(*value), Some(HeapObject::Exception(_))))
        {
            return Err("exception group children must be exception instances".to_owned());
        }
        let requested_name = match self.heap.get(exception_type) {
            Some(HeapObject::Type(group_type)) => group_type.name.clone(),
            _ => return Err("exception group type must be a type object".to_owned()),
        };
        let exception_base = self
            .builtin_type("Exception")
            .ok_or_else(|| "Exception type is missing from the kernel".to_owned())?;
        let contains_only_exceptions = exceptions
            .iter()
            .map(|child| self.exception_matches(*child, exception_base))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .all(|matches| matches);
        let exception_type = match requested_name.as_str() {
            "ExceptionGroup" if !contains_only_exceptions => {
                return Err("Cannot nest BaseExceptions in an ExceptionGroup".to_owned());
            }
            "BaseExceptionGroup" if contains_only_exceptions => self
                .builtin_type("ExceptionGroup")
                .ok_or_else(|| "ExceptionGroup type is missing from the kernel".to_owned())?,
            _ => exception_type,
        };
        let mut roots = Vec::with_capacity(exceptions.len() + 2);
        roots.extend_from_slice(exceptions);
        roots.extend([exception_type, message]);
        self.with_temporary_roots(&roots, |context| {
            let children =
                context.allocate(HeapObject::Tuple(exceptions.to_vec().into_boxed_slice()))?;
            context.with_temporary_roots(&[exception_type, message, children], |context| {
                let arguments = context.allocate(HeapObject::Tuple(
                    vec![message, children].into_boxed_slice(),
                ))?;
                context.with_temporary_roots(&[exception_type, arguments, children], |context| {
                    context.allocate(HeapObject::Exception(ExceptionObject {
                        exception_type,
                        arguments,
                        dictionary: None,
                        traceback: None,
                        cause: None,
                        context: None,
                        suppress_context: false,
                        group_message: Some(message_text),
                        group_exceptions: Some(children),
                        group_origin: None,
                    }))
                })
            })
        })
    }

    pub(crate) fn split_exception(
        &mut self,
        exception: RValue,
        expected_type: RValue,
    ) -> Result<(Option<RValue>, Option<RValue>), String> {
        if self.exception_group_type_in_match(expected_type)? {
            return Err("catching ExceptionGroup with except* is not allowed".to_owned());
        }
        let input_is_group = matches!(
            self.heap.get(exception),
            Some(HeapObject::Exception(ExceptionObject {
                group_exceptions: Some(_),
                ..
            }))
        );
        let (matched, rest) = self
            .with_temporary_roots(&[exception, expected_type], |context| {
                context.split_exception_inner(exception, expected_type)
            })?;
        if input_is_group || matched.is_none() {
            return Ok((matched, rest));
        }
        let matched = matched.expect("matched partition checked above");
        let roots = rest.into_iter().chain([matched]).collect::<Vec<_>>();
        let wrapped = self.with_temporary_roots(&roots, |context| {
            let message = context.allocate(HeapObject::String(String::new()))?;
            let group_type = context
                .builtin_type("ExceptionGroup")
                .ok_or_else(|| "ExceptionGroup type is missing from the kernel".to_owned())?;
            context.new_exception_group(group_type, message, &[matched])
        })?;
        Ok((Some(wrapped), rest))
    }

    fn exception_group_type_in_match(&self, expected_type: RValue) -> Result<bool, String> {
        if let Some(HeapObject::Tuple(types)) = self.heap.get(expected_type) {
            for expected in types.iter().copied() {
                if self.exception_group_type_in_match(expected)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let Some(HeapObject::Type(expected)) = self.heap.get(expected_type) else {
            return Err("catching classes must derive from BaseException".to_owned());
        };
        let group_base = self
            .builtin_type("BaseExceptionGroup")
            .ok_or_else(|| "BaseExceptionGroup type is missing from the kernel".to_owned())?;
        Ok(expected.mro.contains(&group_base))
    }

    pub(crate) fn merge_active_exception_group(&mut self, remainder: RValue) -> Result<(), String> {
        if remainder == RValue::NONE {
            return Ok(());
        }
        let active = self
            .raised
            .ok_or_else(|| "no raised exception is available to merge".to_owned())?;
        let exception_group = self
            .builtin_type("ExceptionGroup")
            .ok_or_else(|| "ExceptionGroup type is missing from the kernel".to_owned())?;
        let message_text = "errors in an except* handler";
        let merged = self.with_temporary_roots(&[active, remainder], |context| {
            let message = context.allocate(HeapObject::String(message_text.to_owned()))?;
            context.new_exception_group(exception_group, message, &[active, remainder])
        })?;
        self.raised = Some(merged);
        Ok(())
    }

    pub(crate) fn combine_exceptions(
        &mut self,
        left: RValue,
        right: RValue,
    ) -> Result<RValue, String> {
        if left == RValue::NONE {
            return Ok(right);
        }
        if right == RValue::NONE {
            return Ok(left);
        }
        let origin = |value| match self.heap.get(value) {
            Some(HeapObject::Exception(exception)) => exception.group_origin,
            _ => None,
        };
        if let (Some(left_origin), Some(right_origin)) = (origin(left), origin(right))
            && left_origin == right_origin
        {
            return Ok(left_origin);
        }
        let exception_group = self
            .builtin_type("ExceptionGroup")
            .ok_or_else(|| "ExceptionGroup type is missing from the kernel".to_owned())?;
        self.with_temporary_roots(&[left, right], |context| {
            let message =
                context.allocate(HeapObject::String("errors in except* handlers".to_owned()))?;
            context.new_exception_group(exception_group, message, &[left, right])
        })
    }

    fn split_exception_inner(
        &mut self,
        exception: RValue,
        expected_type: RValue,
    ) -> Result<(Option<RValue>, Option<RValue>), String> {
        let group = match self.heap.get(exception) {
            Some(HeapObject::Exception(object)) => object.group_exceptions.map(|children| {
                (
                    object.exception_type,
                    object.group_message.clone(),
                    children,
                )
            }),
            _ => return Err("exception splitting requires an exception instance".to_owned()),
        };
        let Some((group_type, message, children)) = group else {
            return if self.exception_matches(exception, expected_type)? {
                Ok((Some(exception), None))
            } else {
                Ok((None, Some(exception)))
            };
        };
        let Some(HeapObject::Tuple(children)) = self.heap.get(children) else {
            return Err("exception group children are invalid".to_owned());
        };
        let children = children.to_vec();
        let mut matched = Vec::new();
        let mut rest = Vec::new();
        for child in children {
            let roots = matched
                .iter()
                .chain(&rest)
                .copied()
                .chain([exception, expected_type])
                .collect::<Vec<_>>();
            let (child_match, child_rest) = self.with_temporary_roots(&roots, |context| {
                context.split_exception_inner(child, expected_type)
            })?;
            matched.extend(child_match);
            rest.extend(child_rest);
        }
        let message = message.ok_or_else(|| "exception group message is missing".to_owned())?;
        let matched_group =
            self.derive_exception_group(exception, group_type, &message, &matched, &rest)?;
        let roots = matched_group
            .into_iter()
            .chain(rest.iter().copied())
            .collect::<Vec<_>>();
        let rest_group = self.with_temporary_roots(&roots, |context| {
            context.derive_exception_group(exception, group_type, &message, &rest, &[])
        })?;
        Ok((matched_group, rest_group))
    }

    fn derive_exception_group(
        &mut self,
        origin: RValue,
        group_type: RValue,
        message: &str,
        children: &[RValue],
        additional_roots: &[RValue],
    ) -> Result<Option<RValue>, String> {
        if children.is_empty() {
            return Ok(None);
        }
        let roots = children
            .iter()
            .chain(additional_roots)
            .copied()
            .chain([group_type])
            .collect::<Vec<_>>();
        self.with_temporary_roots(&roots, |context| {
            let message_value = context.allocate(HeapObject::String(message.to_owned()))?;
            let group = context.new_exception_group(group_type, message_value, children)?;
            let Some(HeapObject::Exception(group_object)) = context.heap.get_mut(group) else {
                return Err("derived exception group became invalid".to_owned());
            };
            group_object.group_origin = Some(origin);
            Ok(Some(group))
        })
    }

    /// Normalizes one source-level `raise` operand. This deliberately lives
    /// outside `raise_value`: only the dedicated `rimera_raise` ABI needs the
    /// generic call path for exception classes, so ordinary runtime error
    /// machinery remains dead-strip friendly for programs that never execute
    /// a source `raise`.
    pub(crate) fn normalize_raise_operand(&mut self, value: RValue) -> Result<RValue, String> {
        if matches!(self.heap.get(value), Some(HeapObject::Exception(_))) {
            return Ok(value);
        }
        let Some(HeapObject::Type(exception_type)) = self.heap.get(value) else {
            return Err("exceptions must derive from BaseException".to_owned());
        };
        let base_exception = self
            .builtin_type("BaseException")
            .ok_or_else(|| "exception kernel is not initialized".to_owned())?;
        if !exception_type.mro.contains(&base_exception) {
            return Err("exceptions must derive from BaseException".to_owned());
        }
        crate::call::invoke(self, value, &[], &[])
    }

    pub(crate) fn raise_value(
        &mut self,
        exception: RValue,
        cause: Option<RValue>,
        suppress_context: bool,
    ) -> Result<(), String> {
        if !matches!(self.heap.get(exception), Some(HeapObject::Exception(_))) {
            return Err("exceptions must derive from BaseException".to_owned());
        }
        if cause
            .is_some_and(|value| !matches!(self.heap.get(value), Some(HeapObject::Exception(_))))
        {
            return Err("exception causes must derive from BaseException".to_owned());
        }
        let implicit_context = self
            .handled
            .last()
            .copied()
            .or(self.raised)
            .or_else(|| self.ambient_exception_context.last().copied())
            .filter(|active| *active != exception);
        let Some(HeapObject::Exception(object)) = self.heap.get_mut(exception) else {
            return Err("exception handle became invalid".to_owned());
        };
        object.cause = cause;
        object.suppress_context = suppress_context || cause.is_some();
        if object.context.is_none() {
            object.context = implicit_context;
        }
        self.raised = Some(exception);
        self.exception = self.exception_message(exception);
        Ok(())
    }

    pub(crate) fn reraise(&mut self) -> Result<(), String> {
        let exception = self
            .handled
            .last()
            .copied()
            .ok_or_else(|| "No active exception to reraise".to_owned())?;
        self.raised = Some(exception);
        self.exception = self.exception_message(exception);
        Ok(())
    }

    pub(crate) fn handler_enter(&mut self) -> Result<RValue, String> {
        let exception = self
            .raised
            .take()
            .ok_or_else(|| "no raised exception is available for a handler".to_owned())?;
        self.handled.push(exception);
        Ok(exception)
    }

    pub(crate) fn handler_leave(&mut self) -> Result<(), String> {
        self.handled
            .pop()
            .map(|_| ())
            .ok_or_else(|| "no active exception handler to leave".to_owned())
    }

    pub(crate) fn exception_matches(
        &self,
        exception: RValue,
        expected_type: RValue,
    ) -> Result<bool, String> {
        if let Some(HeapObject::Tuple(expected_types)) = self.heap.get(expected_type) {
            let expected_types = expected_types.to_vec();
            for expected in expected_types {
                if self.exception_matches(exception, expected)? {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let Some(HeapObject::Exception(exception)) = self.heap.get(exception) else {
            return Err("exception matching requires an exception instance".to_owned());
        };
        let Some(HeapObject::Type(expected)) = self.heap.get(expected_type) else {
            return Err("catching classes must derive from BaseException".to_owned());
        };
        let Some(HeapObject::Type(actual)) = self.heap.get(exception.exception_type) else {
            return Err("exception instance has an invalid type".to_owned());
        };
        Ok(actual.mro.contains(&expected_type)
            && expected
                .mro
                .iter()
                .any(|value| self.builtin_type("BaseException") == Some(*value)))
    }

    pub(crate) fn attach_module_traceback(
        &mut self,
        filename: &str,
        function: &str,
        line: u32,
        column: u32,
    ) -> Result<(), String> {
        let frame = Some(self.ensure_module_frame(filename, line)?);
        self.attach_traceback_frame(filename, function, line, column, frame)
    }

    pub(crate) fn attach_traceback(
        &mut self,
        filename: &str,
        function: &str,
        line: u32,
        column: u32,
    ) -> Result<(), String> {
        let frame = if self.active_calls.is_empty() {
            Some(self.ensure_module_frame(filename, line)?)
        } else {
            Some(self.ensure_active_frame(self.active_calls.len() - 1, line)?)
        };
        self.attach_traceback_frame(filename, function, line, column, frame)
    }

    fn attach_traceback_frame(
        &mut self,
        filename: &str,
        function: &str,
        line: u32,
        column: u32,
        frame: Option<RValue>,
    ) -> Result<(), String> {
        let exception = self
            .raised
            .ok_or_else(|| "cannot attach a traceback without an active exception".to_owned())?;
        let next = match self.heap.get(exception) {
            Some(HeapObject::Exception(object)) => object.traceback,
            _ => return Err("active exception handle is invalid".to_owned()),
        };
        let traceback = self.with_temporary_roots(
            &next
                .into_iter()
                .chain(frame)
                .chain([exception])
                .collect::<Vec<_>>(),
            |context| {
                context.allocate(HeapObject::Traceback(TracebackObject {
                    filename: filename.to_owned(),
                    function: function.to_owned(),
                    line,
                    column,
                    frame,
                    next,
                }))
            },
        )?;
        let Some(HeapObject::Exception(object)) = self.heap.get_mut(exception) else {
            return Err("active exception handle became invalid".to_owned());
        };
        object.traceback = Some(traceback);
        Ok(())
    }

    pub(crate) fn exception_type_name(&self, exception: RValue) -> Option<&str> {
        let HeapObject::Exception(exception) = self.heap.get(exception)? else {
            return None;
        };
        let HeapObject::Type(exception_type) = self.heap.get(exception.exception_type)? else {
            return None;
        };
        Some(&exception_type.name)
    }

    pub(crate) fn exception_message(&self, exception: RValue) -> Option<String> {
        let HeapObject::Exception(exception) = self.heap.get(exception)? else {
            return None;
        };
        let HeapObject::Tuple(arguments) = self.heap.get(exception.arguments)? else {
            return None;
        };
        match arguments.as_ref() {
            [] => Some(String::new()),
            [value] => crate::operations::display(self, *value).ok(),
            _ => crate::operations::display(self, exception.arguments).ok(),
        }
    }

    pub(crate) fn render_active_exception(&mut self) -> String {
        let Some(exception) = self.raised.take().or_else(|| self.handled.last().copied()) else {
            return format!(
                "Rimera runtime error: {}",
                self.exception
                    .take()
                    .unwrap_or_else(|| "unknown failure".to_owned())
            );
        };
        let mut output = String::new();
        self.render_exception_chain(exception, &mut output, &mut Vec::new());
        self.exception = None;
        output
    }

    fn render_exception_chain(
        &self,
        exception: RValue,
        output: &mut String,
        visited: &mut Vec<RValue>,
    ) {
        if visited.contains(&exception) {
            return;
        }
        visited.push(exception);
        let (cause, context, suppress_context, traceback) = match self.heap.get(exception) {
            Some(HeapObject::Exception(object)) => (
                object.cause,
                object.context,
                object.suppress_context,
                object.traceback,
            ),
            _ => (None, None, false, None),
        };
        if let Some(cause) = cause {
            self.render_exception_chain(cause, output, visited);
            output.push_str(
                "\n\nThe above exception was the direct cause of the following exception:\n\n",
            );
        } else if !suppress_context && let Some(context) = context {
            self.render_exception_chain(context, output, visited);
            output.push_str(
                "\n\nDuring handling of the above exception, another exception occurred:\n\n",
            );
        }
        let mut traceback = traceback;
        if traceback.is_some() {
            output.push_str("Traceback (most recent call last):\n");
            while let Some(value) = traceback {
                let Some(HeapObject::Traceback(frame)) = self.heap.get(value) else {
                    break;
                };
                output.push_str(&format!(
                    "  File {:?}, line {}, in {}\n",
                    frame.filename, frame.line, frame.function
                ));
                traceback = frame.next;
            }
        }
        if let Some(HeapObject::Exception(group)) = self.heap.get(exception)
            && let Some(children) = group.group_exceptions
            && let Some(HeapObject::Tuple(children)) = self.heap.get(children)
        {
            let children = children.to_vec();
            output.push_str(
                self.exception_type_name(exception)
                    .unwrap_or("ExceptionGroup"),
            );
            output.push_str(": ");
            output.push_str(group.group_message.as_deref().unwrap_or(""));
            output.push_str(&format!(
                " ({} sub-exception{})",
                children.len(),
                if children.len() == 1 { "" } else { "s" }
            ));
            for (index, child) in children.into_iter().enumerate() {
                output.push_str(&format!(
                    "\n+-+---------------- {} ----------------\n",
                    index + 1
                ));
                self.render_exception_chain(child, output, visited);
            }
            output.push_str("\n+------------------------------------");
            return;
        }
        output.push_str(
            self.exception_type_name(exception)
                .unwrap_or("RuntimeError"),
        );
        let message = self.exception_message(exception).unwrap_or_default();
        if !message.is_empty() {
            output.push_str(": ");
            output.push_str(&message);
        }
    }

    pub(crate) fn add_context_root(&mut self, value: RValue) {
        self.context_roots.push(value);
    }

    pub(crate) fn remove_context_root(&mut self, value: RValue) -> bool {
        let Some(index) = self.context_roots.iter().rposition(|root| *root == value) else {
            return false;
        };
        self.context_roots.remove(index);
        true
    }

    #[cfg(test)]
    pub(crate) fn native_root_count(&self) -> usize {
        self.native_roots.len()
    }
}
