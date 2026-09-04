use std::collections::BTreeMap;
use std::mem::size_of;

use num_bigint::BigInt;
use rimera_abi::{RParameterKind as ParameterKind, RTypeParameterKind, RValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Normal,
    Generator { persistent_slot_count: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub kind: ParameterKind,
    pub has_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallArgumentsObject {
    pub callable: RValue,
    pub positional: Vec<RValue>,
    pub keywords: Vec<(String, RValue)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeObject {
    pub code_address: usize,
    pub kind: FunctionKind,
    pub name: String,
    pub qualified_name: String,
    pub parameters: Box<[Parameter]>,
    pub filename: String,
    pub first_line: u32,
    pub local_names: Box<[String]>,
    pub cell_names: Box<[String]>,
    pub free_names: Box<[String]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionObject {
    pub code: RValue,
    pub name: String,
    pub qualified_name: String,
    pub closure: Option<RValue>,
    pub defaults: Option<RValue>,
    pub keyword_defaults: Option<RValue>,
    pub annotations: Option<RValue>,
    pub type_params: Option<RValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParameterObject {
    pub name: String,
    pub kind: RTypeParameterKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAliasObject {
    pub name: String,
    pub type_params: RValue,
    pub value: RValue,
}

/// Persistent execution state for one compiled synchronous generator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratorObject {
    pub function: RValue,
    pub name: String,
    pub qualified_name: String,
    pub resume_address: usize,
    pub state: u32,
    pub slots: Box<[Option<RValue>]>,
    /// Reflection-only cell registry for the generator activation. The cells
    /// are the same cells used by compiled execution; no duplicate locals
    /// storage or frame interpreter is introduced.
    pub local_cells: Vec<(String, RValue)>,
    /// CPython 3.12 keeps one refreshed locals dictionary per generator frame.
    /// It survives suspension but is dropped from the generator at terminal
    /// completion; independently retained dictionaries remain ordinary GC
    /// managed values.
    pub locals_snapshot: Option<RValue>,
    /// Stable Python-visible frame identity while the generator is live.
    pub frame: Option<RValue>,
    pub delegate: Option<RValue>,
    pub handled: Box<[RValue]>,
    pub raised: Option<RValue>,
    pub started: bool,
    pub running: bool,
    pub closed: bool,
    pub completed: bool,
    pub return_value: Option<RValue>,
}

/// A permanent native callable exposed through the ordinary Python call path.
///
/// These are values in the managed heap rather than compiler intrinsics, so
/// builtin lookup, lifetime, `type`, and invocation use the same contracts as
/// compiled functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinFunctionKind {
    Abs,
    All,
    Any,
    Len,
    Print,
    Round,
    IsInstance,
    IsSubclass,
    Iter,
    Next,
    TypePrepare,
    Property,
    StaticMethod,
    ClassMethod,
    GetAttr,
    SetAttr,
    DelAttr,
    HasAttr,
    Callable,
    Hash,
    Repr,
    Format,
    Reversed,
    FloatConjugate,
    FloatIsInteger,
    FloatAsIntegerRatio,
    FloatHex,
    FloatFromHex,
    ComplexConjugate,
    RangeCount,
    RangeIndex,
    SliceIndices,
    Bin,
    Hex,
    Oct,
    Chr,
    Ord,
    DivMod,
    Pow,
    Sum,
    Min,
    Max,
    DictKeys,
    DictValues,
    DictItems,
    DictGet,
    DictSetDefault,
    DictPop,
    DictPopItem,
    DictUpdate,
    DictClear,
    DictCopy,
    ListAppend,
    ListExtend,
    ListInsert,
    ListPop,
    ListRemove,
    ListClear,
    ListCopy,
    ListCount,
    ListIndex,
    ListReverse,
    ListSort,
    GeneratorIter,
    GeneratorNext,
    GeneratorSend,
    GeneratorThrow,
    GeneratorClose,
    ExceptionWithTraceback,
    BuiltinStorageInit,
    MemoryViewRelease,
    MemoryViewToBytes,
    MemoryViewToList,
    MemoryViewToReadOnly,
    MemoryViewHex,
    MemoryViewCast,
    SetAdd,
    SetDiscard,
    SetRemove,
    SetPop,
    SetClear,
    SetCopy,
    SetUpdate,
    SetIntersectionUpdate,
    SetDifferenceUpdate,
    SetSymmetricDifferenceUpdate,
    SetUnion,
    SetIntersection,
    SetDifference,
    SetSymmetricDifference,
    SetIsDisjoint,
    SetIsSubset,
    SetIsSuperset,
    Enumerate,
    Zip,
    Map,
    Filter,
    Sorted,
    Id,
    Ascii,
    Dir,
    Vars,
    Globals,
    Locals,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinFunctionObject {
    pub name: String,
    pub kind: BuiltinFunctionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeObject {
    pub name: String,
    pub qualified_name: String,
    /// The class which constructs this type. This is always `type` for the
    /// kernel and ordinary user classes, but remains explicit so custom
    /// metaclass selection can use the same traced object graph.
    pub metaclass: RValue,
    pub bases: Box<[RValue]>,
    pub mro: Box<[RValue]>,
    pub namespace: RValue,
    pub slot_names: Box<[String]>,
    pub has_dictionary: bool,
    pub has_weakref: bool,
    pub flags: u8,
    pub version_tag: u64,
    pub layout: TypeLayout,
}

pub const TYPE_FLAG_BUILTIN: u8 = 1 << 0;
pub const TYPE_FLAG_EXCEPTION: u8 = 1 << 1;
pub const TYPE_FLAG_INSTANTIABLE: u8 = 1 << 2;

/// Fixed native storage carried by instances of a supported builtin subclass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeLayout {
    Object,
    List,
    Tuple,
    Dictionary,
    Set,
    Float,
    Complex,
    Bytes,
    ByteArray,
    FrozenSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceObject {
    pub class: RValue,
    pub dictionary: Option<RValue>,
    pub slots: Box<[Option<RValue>]>,
    /// Native payload for a supported builtin-storage subclass. The payload
    /// remains opaque to generated code and is traced as an ordinary handle.
    pub storage: Option<RValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberDescriptorObject {
    pub owner: RValue,
    pub index: usize,
    pub name: String,
}

/// A compiled function retrieved through an instance attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundMethodObject {
    pub function: RValue,
    pub receiver: RValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyMethodKind {
    Getter,
    Setter,
    Deleter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyObject {
    pub getter: Option<RValue>,
    pub setter: Option<RValue>,
    pub deleter: Option<RValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyMethodObject {
    pub property: RValue,
    pub kind: PropertyMethodKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticMethodObject {
    pub callable: RValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassMethodObject {
    pub callable: RValue,
}

/// The state carried by Python's explicit two-argument `super` form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuperObject {
    pub start_type: RValue,
    pub receiver: RValue,
    pub receiver_type: RValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellObject {
    pub value: Option<RValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryObject {
    pub entries: Vec<(String, RValue)>,
}

/// Insertion-ordered hash storage shared by Python mappings and sets.
///
/// Entry identifiers remain stable for their lifetime. Removal leaves a
/// tombstone so outstanding iterators never observe shifted entry indices;
/// bucket chains point at those stable identifiers and discard tombstones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedHashTable<T> {
    entries: Vec<Option<OrderedHashEntry<T>>>,
    order: Vec<usize>,
    buckets: BTreeMap<i64, Vec<usize>>,
    len: usize,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OrderedHashEntry<T> {
    hash: i64,
    value: T,
}

impl<T> Default for OrderedHashTable<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            order: Vec::new(),
            buckets: BTreeMap::new(),
            len: 0,
            version: 0,
        }
    }
}

impl<T> OrderedHashTable<T> {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn candidates(&self, hash: i64) -> Vec<usize> {
        self.buckets.get(&hash).cloned().unwrap_or_default()
    }

    pub fn value(&self, entry: usize) -> Option<&T> {
        self.entries.get(entry)?.as_ref().map(|entry| &entry.value)
    }

    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.order.iter().filter_map(|entry| self.value(*entry))
    }

    /// Produces a stable, insertion-ordered snapshot before user equality or
    /// hashing code is invoked.  Runtime protocol calls must never retain a
    /// borrow into a table, because those calls may allocate or mutate it.
    pub fn snapshot(&self) -> Vec<(usize, T)>
    where
        T: Clone,
    {
        self.order
            .iter()
            .filter_map(|entry| self.value(*entry).cloned().map(|value| (*entry, value)))
            .collect()
    }

    pub fn hashed_snapshot(&self) -> Vec<(usize, i64, T)>
    where
        T: Clone,
    {
        self.order
            .iter()
            .filter_map(|index| {
                self.entries
                    .get(*index)?
                    .as_ref()
                    .map(|entry| (*index, entry.hash, entry.value.clone()))
            })
            .collect()
    }

    pub fn update(&mut self, entry: usize, update: impl FnOnce(&mut T)) -> bool {
        let Some(entry) = self.entries.get_mut(entry).and_then(Option::as_mut) else {
            return false;
        };
        update(&mut entry.value);
        true
    }

    pub fn insert_new(&mut self, hash: i64, value: T) {
        let entry = self.entries.len();
        self.entries.push(Some(OrderedHashEntry { hash, value }));
        self.order.push(entry);
        self.buckets.entry(hash).or_default().push(entry);
        self.len += 1;
        self.version = self.version.wrapping_add(1);
    }

    pub fn remove(&mut self, index: usize) -> Option<T> {
        let entry = self.entries.get_mut(index)?.take()?;
        if let Some(bucket) = self.buckets.get_mut(&entry.hash) {
            bucket.retain(|candidate| *candidate != index);
        }
        self.len -= 1;
        self.version = self.version.wrapping_add(1);
        Some(entry.value)
    }

    pub fn managed_size(&self) -> usize {
        let entry_storage = self
            .entries
            .capacity()
            .saturating_mul(size_of::<Option<OrderedHashEntry<T>>>());
        let order_storage = self.order.capacity().saturating_mul(size_of::<usize>());
        let bucket_vectors = self.buckets.values().fold(0_usize, |size, bucket| {
            size.saturating_add(bucket.capacity().saturating_mul(size_of::<usize>()))
        });
        let map_nodes = self.buckets.len().saturating_mul(
            size_of::<(i64, Vec<usize>)>().saturating_add(size_of::<usize>().saturating_mul(4)),
        );
        entry_storage
            .saturating_add(order_storage)
            .saturating_add(bucket_vectors)
            .saturating_add(map_nodes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ValueDictionaryObject {
    pub table: OrderedHashTable<(RValue, RValue)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SetObject {
    pub table: OrderedHashTable<RValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteArrayObject {
    pub bytes: Vec<u8>,
    /// Active native memoryviews. Resizing is prohibited while this is nonzero.
    pub exports: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceObject {
    pub start: Option<RValue>,
    pub stop: Option<RValue>,
    pub step: Option<RValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictionaryViewKind {
    Keys,
    Values,
    Items,
}

/// A live view over a Python-visible ordered dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryViewObject {
    pub dictionary: RValue,
    pub kind: DictionaryViewKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingProxyObject {
    pub dictionary: RValue,
}

/// One Python-level PEP 688 export lease shared by every derived view. The
/// provider callback is fired exactly once when the last live view releases or
/// when the entire lease becomes unreachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferLeaseObject {
    pub provider: RValue,
    pub exported_view: RValue,
    pub active_views: usize,
    pub released: bool,
}

/// Native buffer metadata. The initial exporters are bytes, bytearray, and
/// memoryview; the exporter handle keeps the backing allocation alive. A
/// provider lease is present only for Python-level PEP 688 exports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryViewObject {
    pub exporter: RValue,
    pub lease: Option<RValue>,
    pub format: String,
    pub item_size: usize,
    pub shape: Box<[usize]>,
    pub strides: Box<[isize]>,
    pub suboffsets: Box<[isize]>,
    pub offset: usize,
    pub readonly: bool,
    pub released: bool,
}

impl DictionaryObject {
    pub fn get(&self, name: &str) -> Option<RValue> {
        self.entries
            .iter()
            .rev()
            .find_map(|(key, value)| (key == name).then_some(*value))
    }

    pub fn insert(&mut self, name: String, value: RValue) -> Option<RValue> {
        if let Some((_, current)) = self.entries.iter_mut().find(|(key, _)| key == &name) {
            return Some(std::mem::replace(current, value));
        }
        self.entries.push((name, value));
        None
    }

    pub fn remove(&mut self, name: &str) -> Option<RValue> {
        let index = self.entries.iter().position(|(key, _)| key == name)?;
        Some(self.entries.remove(index).1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionObject {
    pub exception_type: RValue,
    pub arguments: RValue,
    pub dictionary: Option<RValue>,
    pub traceback: Option<RValue>,
    pub cause: Option<RValue>,
    pub context: Option<RValue>,
    pub suppress_context: bool,
    pub group_message: Option<String>,
    pub group_exceptions: Option<RValue>,
    pub group_origin: Option<RValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TracebackObject {
    pub filename: String,
    pub function: String,
    pub line: u32,
    pub column: u32,
    pub frame: Option<RValue>,
    pub next: Option<RValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameObject {
    pub code: RValue,
    pub globals: RValue,
    pub locals: RValue,
    pub back: Option<RValue>,
    pub line: u32,
    /// Present only while this frame is owned by a live generator. This lets
    /// reflection refresh `f_locals` from the authoritative generator state.
    pub generator: Option<RValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleObject {
    pub name: String,
    pub namespace: RValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeObject {
    pub start: BigInt,
    pub stop: BigInt,
    pub step: BigInt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IteratorObject {
    Range {
        current: BigInt,
        stop: BigInt,
        step: BigInt,
    },
    Sequence {
        source: RValue,
        index: usize,
        /// Mapping/set iterators reject size-changing mutation. Sequence
        /// iterators deliberately leave this empty to preserve Python's live
        /// list-length semantics.
        expected_version: Option<u64>,
    },
    /// Implements Python's legacy sequence iteration fallback by repeatedly
    /// invoking `__getitem__` with zero-based integer indices.
    SequenceProtocol {
        source: RValue,
        index: usize,
    },
    ReverseSequence {
        source: RValue,
        index: isize,
        expected_version: Option<u64>,
    },
    /// Implements the legacy reversible-sequence protocol through the
    /// receiver's `__getitem__` method. The receiver remains opaque to the
    /// generated code and is retained by the iterator while it is live.
    ReverseProtocol {
        source: RValue,
        index: isize,
    },
    Enumerate {
        source: RValue,
        index: BigInt,
    },
    Zip {
        sources: Box<[RValue]>,
        strict: bool,
    },
    CallSentinel {
        callable: RValue,
        sentinel: RValue,
    },
    Map {
        callable: RValue,
        sources: Box<[RValue]>,
    },
    Filter {
        predicate: RValue,
        source: RValue,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ManagedObject {
    NotImplemented,
    Ellipsis,
    Float(f64),
    Complex { real: f64, imag: f64 },
    BigInt(num_bigint::BigInt),
    String(String),
    Bytes(Vec<u8>),
    ByteArray(ByteArrayObject),
    Slice(SliceObject),
    ValueArray(Box<[RValue]>),
    Tuple(Box<[RValue]>),
    List(Vec<RValue>),
    Dictionary(DictionaryObject),
    ValueDictionary(ValueDictionaryObject),
    Set(SetObject),
    FrozenSet(SetObject),
    DictionaryView(DictionaryViewObject),
    MappingProxy(MappingProxyObject),
    MemoryView(MemoryViewObject),
    BufferLease(BufferLeaseObject),
    Module(ModuleObject),
    TypeParameter(TypeParameterObject),
    TypeAlias(TypeAliasObject),
    Type(TypeObject),
    Instance(InstanceObject),
    BoundMethod(BoundMethodObject),
    CallArguments(CallArgumentsObject),
    Property(PropertyObject),
    PropertyMethod(PropertyMethodObject),
    StaticMethod(StaticMethodObject),
    ClassMethod(ClassMethodObject),
    MemberDescriptor(MemberDescriptorObject),
    Super(SuperObject),
    Function(FunctionObject),
    Code(CodeObject),
    Generator(Box<GeneratorObject>),
    BuiltinFunction(BuiltinFunctionObject),
    Cell(CellObject),
    Exception(ExceptionObject),
    Traceback(TracebackObject),
    Frame(Box<FrameObject>),
    Range(RangeObject),
    Iterator(IteratorObject),
}

impl ManagedObject {
    pub fn trace_children(&self, visitor: &mut dyn FnMut(RValue)) {
        match self {
            Self::NotImplemented
            | Self::Ellipsis
            | Self::Float(_)
            | Self::Complex { .. }
            | Self::String(_)
            | Self::Bytes(_)
            | Self::ByteArray(_) => {}
            Self::Slice(object) => {
                object.start.into_iter().for_each(&mut *visitor);
                object.stop.into_iter().for_each(&mut *visitor);
                object.step.into_iter().for_each(visitor);
            }
            Self::ValueArray(values) | Self::Tuple(values) => {
                values.iter().copied().for_each(visitor);
            }
            Self::List(values) => {
                values.iter().copied().for_each(visitor);
            }
            Self::Dictionary(dictionary) => dictionary
                .entries
                .iter()
                .for_each(|(_, value)| visitor(*value)),
            Self::ValueDictionary(dictionary) => {
                dictionary.table.values().for_each(|(key, value)| {
                    visitor(*key);
                    visitor(*value);
                })
            }
            Self::Set(set) => set.table.values().copied().for_each(visitor),
            Self::FrozenSet(set) => set.table.values().copied().for_each(visitor),
            Self::DictionaryView(object) => visitor(object.dictionary),
            Self::MappingProxy(object) => visitor(object.dictionary),
            Self::MemoryView(object) => {
                visitor(object.exporter);
                object.lease.into_iter().for_each(visitor);
            }
            Self::BufferLease(object) => {
                visitor(object.provider);
                visitor(object.exported_view);
            }
            Self::Module(object) => visitor(object.namespace),
            Self::TypeParameter(_) => {}
            Self::TypeAlias(object) => {
                visitor(object.type_params);
                visitor(object.value);
            }
            Self::Type(object) => object
                .bases
                .iter()
                .chain(object.mro.iter())
                .copied()
                .chain(std::iter::once(object.metaclass))
                .chain(std::iter::once(object.namespace))
                .for_each(visitor),
            Self::Instance(object) => {
                visitor(object.class);
                object.dictionary.into_iter().for_each(&mut *visitor);
                object
                    .slots
                    .iter()
                    .flatten()
                    .copied()
                    .for_each(&mut *visitor);
                object.storage.into_iter().for_each(visitor);
            }
            Self::BoundMethod(object) => {
                visitor(object.function);
                visitor(object.receiver);
            }
            Self::CallArguments(object) => {
                visitor(object.callable);
                object.positional.iter().copied().for_each(&mut *visitor);
                object
                    .keywords
                    .iter()
                    .map(|(_, value)| *value)
                    .for_each(visitor);
            }
            Self::Property(object) => {
                object.getter.into_iter().for_each(&mut *visitor);
                object.setter.into_iter().for_each(&mut *visitor);
                object.deleter.into_iter().for_each(visitor);
            }
            Self::PropertyMethod(object) => visitor(object.property),
            Self::StaticMethod(object) => visitor(object.callable),
            Self::ClassMethod(object) => visitor(object.callable),
            Self::MemberDescriptor(object) => visitor(object.owner),
            Self::Super(object) => {
                visitor(object.start_type);
                visitor(object.receiver);
                visitor(object.receiver_type);
            }
            Self::Function(object) => {
                visitor(object.code);
                object.closure.into_iter().for_each(&mut *visitor);
                object.defaults.into_iter().for_each(&mut *visitor);
                object.keyword_defaults.into_iter().for_each(&mut *visitor);
                object.annotations.into_iter().for_each(&mut *visitor);
                object.type_params.into_iter().for_each(visitor);
            }
            Self::Code(_) => {}
            Self::Generator(object) => {
                visitor(object.function);
                object
                    .slots
                    .iter()
                    .flatten()
                    .copied()
                    .for_each(&mut *visitor);
                object
                    .local_cells
                    .iter()
                    .map(|(_, cell)| *cell)
                    .for_each(&mut *visitor);
                object.locals_snapshot.into_iter().for_each(&mut *visitor);
                object.frame.into_iter().for_each(&mut *visitor);
                object.delegate.into_iter().for_each(&mut *visitor);
                object.handled.iter().copied().for_each(&mut *visitor);
                object.raised.into_iter().for_each(&mut *visitor);
                object.return_value.into_iter().for_each(visitor);
            }
            Self::BuiltinFunction(_) => {}
            Self::Cell(object) => object.value.into_iter().for_each(visitor),
            Self::Exception(object) => {
                visitor(object.exception_type);
                visitor(object.arguments);
                object.dictionary.into_iter().for_each(&mut *visitor);
                object.traceback.into_iter().for_each(&mut *visitor);
                object.cause.into_iter().for_each(&mut *visitor);
                object.context.into_iter().for_each(&mut *visitor);
                object.group_exceptions.into_iter().for_each(&mut *visitor);
                object.group_origin.into_iter().for_each(visitor);
            }
            Self::Traceback(object) => {
                object.frame.into_iter().for_each(&mut *visitor);
                object.next.into_iter().for_each(visitor);
            }
            Self::Frame(object) => {
                visitor(object.code);
                visitor(object.globals);
                visitor(object.locals);
                object.back.into_iter().for_each(&mut *visitor);
                object.generator.into_iter().for_each(visitor);
            }
            Self::Iterator(
                IteratorObject::Sequence { source, .. }
                | IteratorObject::SequenceProtocol { source, .. }
                | IteratorObject::ReverseSequence { source, .. }
                | IteratorObject::ReverseProtocol { source, .. }
                | IteratorObject::Enumerate { source, .. },
            ) => visitor(*source),
            Self::Iterator(IteratorObject::Filter { predicate, source }) => {
                visitor(*predicate);
                visitor(*source);
            }
            Self::Iterator(
                IteratorObject::Zip { sources, .. } | IteratorObject::Map { sources, .. },
            ) => {
                sources.iter().copied().for_each(&mut *visitor);
                if let Self::Iterator(IteratorObject::Map { callable, .. }) = self {
                    visitor(*callable);
                }
            }
            Self::Iterator(IteratorObject::CallSentinel { callable, sentinel }) => {
                visitor(*callable);
                visitor(*sentinel);
            }
            Self::BigInt(_) | Self::Range(_) | Self::Iterator(IteratorObject::Range { .. }) => {}
        }
    }

    pub fn managed_size(&self) -> usize {
        let payload = match self {
            Self::NotImplemented | Self::Ellipsis => 0,
            Self::Float(_) => 0,
            Self::Complex { .. } => 0,
            Self::BigInt(value) => usize::try_from(value.bits().div_ceil(8)).unwrap_or(usize::MAX),
            Self::String(value) => value.capacity(),
            Self::Bytes(bytes) => bytes.capacity(),
            Self::ByteArray(object) => object.bytes.capacity(),
            Self::Slice(_) => size_of::<SliceObject>(),
            Self::ValueArray(values) | Self::Tuple(values) => {
                values.len().saturating_mul(size_of::<RValue>())
            }
            Self::List(values) => values.capacity().saturating_mul(size_of::<RValue>()),
            Self::Dictionary(dictionary) => dictionary.entries.iter().fold(
                dictionary
                    .entries
                    .capacity()
                    .saturating_mul(size_of::<(String, RValue)>()),
                |size, (name, _)| size.saturating_add(name.capacity()),
            ),
            Self::ValueDictionary(dictionary) => dictionary.table.managed_size(),
            Self::Set(set) | Self::FrozenSet(set) => set.table.managed_size(),
            Self::DictionaryView(_) => size_of::<DictionaryViewObject>(),
            Self::MappingProxy(_) => size_of::<MappingProxyObject>(),
            Self::MemoryView(object) => object
                .format
                .capacity()
                .saturating_add(object.shape.len().saturating_mul(size_of::<usize>()))
                .saturating_add(object.strides.len().saturating_mul(size_of::<isize>()))
                .saturating_add(object.suboffsets.len().saturating_mul(size_of::<isize>())),
            Self::BufferLease(_) => size_of::<BufferLeaseObject>(),
            Self::Module(object) => {
                size_of::<ModuleObject>().saturating_add(object.name.capacity())
            }
            Self::TypeParameter(object) => {
                size_of::<TypeParameterObject>().saturating_add(object.name.capacity())
            }
            Self::TypeAlias(object) => {
                size_of::<TypeAliasObject>().saturating_add(object.name.capacity())
            }
            Self::Type(object) => object
                .name
                .capacity()
                .saturating_add(object.qualified_name.capacity())
                .saturating_add(object.bases.len().saturating_mul(size_of::<RValue>()))
                .saturating_add(object.mro.len().saturating_mul(size_of::<RValue>()))
                .saturating_add(
                    object
                        .slot_names
                        .iter()
                        .map(String::capacity)
                        .sum::<usize>(),
                ),
            Self::Instance(object) => size_of::<InstanceObject>().saturating_add(
                object
                    .slots
                    .len()
                    .saturating_mul(size_of::<Option<RValue>>()),
            ),
            Self::BoundMethod(_) => size_of::<BoundMethodObject>(),
            Self::CallArguments(object) => object
                .positional
                .capacity()
                .saturating_mul(size_of::<RValue>())
                .saturating_add(
                    object
                        .keywords
                        .capacity()
                        .saturating_mul(size_of::<(String, RValue)>()),
                )
                .saturating_add(
                    object
                        .keywords
                        .iter()
                        .map(|(name, _)| name.capacity())
                        .sum::<usize>(),
                ),
            Self::Property(_) => size_of::<PropertyObject>(),
            Self::PropertyMethod(_) => size_of::<PropertyMethodObject>(),
            Self::StaticMethod(_) => size_of::<StaticMethodObject>(),
            Self::ClassMethod(_) => size_of::<ClassMethodObject>(),
            Self::MemberDescriptor(object) => {
                size_of::<MemberDescriptorObject>() + object.name.capacity()
            }
            Self::Super(_) => size_of::<SuperObject>(),
            Self::Function(object) => object
                .name
                .capacity()
                .saturating_add(object.qualified_name.capacity())
                .saturating_add(size_of::<FunctionObject>()),
            Self::Code(object) => object
                .name
                .capacity()
                .saturating_add(object.qualified_name.capacity())
                .saturating_add(object.filename.capacity())
                .saturating_add(
                    object
                        .parameters
                        .len()
                        .saturating_mul(size_of::<Parameter>()),
                )
                .saturating_add(
                    object
                        .parameters
                        .iter()
                        .map(|parameter| parameter.name.capacity())
                        .sum::<usize>(),
                )
                .saturating_add(
                    object
                        .local_names
                        .iter()
                        .chain(object.cell_names.iter())
                        .chain(object.free_names.iter())
                        .map(String::capacity)
                        .sum::<usize>(),
                )
                .saturating_add(
                    (object.local_names.len() + object.cell_names.len() + object.free_names.len())
                        .saturating_mul(size_of::<String>()),
                ),
            Self::Generator(object) => size_of::<GeneratorObject>()
                .saturating_add(object.name.capacity())
                .saturating_add(object.qualified_name.capacity())
                .saturating_add(
                    object
                        .slots
                        .len()
                        .saturating_mul(size_of::<Option<RValue>>()),
                )
                .saturating_add(
                    object
                        .local_cells
                        .capacity()
                        .saturating_mul(size_of::<(String, RValue)>()),
                )
                .saturating_add(
                    object
                        .local_cells
                        .iter()
                        .map(|(name, _)| name.capacity())
                        .sum::<usize>(),
                )
                .saturating_add(object.handled.len().saturating_mul(size_of::<RValue>())),
            Self::BuiltinFunction(object) => object.name.capacity(),
            Self::Cell(_) => size_of::<CellObject>(),
            Self::Exception(object) => object
                .group_message
                .as_ref()
                .map_or(0, String::capacity)
                .saturating_add(size_of::<ExceptionObject>()),
            Self::Traceback(object) => object
                .filename
                .capacity()
                .saturating_add(object.function.capacity())
                .saturating_add(size_of::<TracebackObject>()),
            Self::Frame(_) => size_of::<FrameObject>(),
            Self::Range(object) => usize::try_from(
                object
                    .start
                    .bits()
                    .saturating_add(object.stop.bits())
                    .saturating_add(object.step.bits())
                    .div_ceil(8),
            )
            .unwrap_or(usize::MAX),
            Self::Iterator(object) => match object {
                IteratorObject::Range {
                    current,
                    stop,
                    step,
                } => usize::try_from(
                    current
                        .bits()
                        .saturating_add(stop.bits())
                        .saturating_add(step.bits())
                        .div_ceil(8),
                )
                .unwrap_or(usize::MAX),
                IteratorObject::Sequence { .. }
                | IteratorObject::SequenceProtocol { .. }
                | IteratorObject::ReverseSequence { .. }
                | IteratorObject::ReverseProtocol { .. }
                | IteratorObject::Enumerate { .. }
                | IteratorObject::Zip { .. }
                | IteratorObject::CallSentinel { .. }
                | IteratorObject::Map { .. }
                | IteratorObject::Filter { .. } => size_of::<IteratorObject>(),
            },
        };
        size_of::<Self>().saturating_add(payload)
    }
}
