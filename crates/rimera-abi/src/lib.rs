use core::ffi::c_void;
use core::ptr;

pub const ABI_VERSION: u32 = 1;

/// Python `compile()` grammar mode shared by the compiler/runtime boundary.
/// Parsing and lowering remain compiler-owned; the ABI carries only stable
/// mode identity for managed code metadata and later native-loader work.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RDynamicCompileMode {
    Exec = 0,
    Eval = 1,
    Single = 2,
}

impl RDynamicCompileMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exec => "exec",
            Self::Eval => "eval",
            Self::Single => "single",
        }
    }
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RTag {
    None = 0,
    Bool = 1,
    SmallInt = 2,
    Handle = 3,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RValue {
    pub tag: u32,
    pub flags: u32,
    pub payload: u64,
}

impl RValue {
    pub const NONE: Self = Self {
        tag: RTag::None as u32,
        flags: 0,
        payload: 0,
    };

    /// Internal native-local sentinel. It is never a Python-visible value: the
    /// tag remains non-handle so GC root scanning can safely ignore an unbound
    /// stack slot, while `flags = 1` distinguishes it from Python `None`.
    pub const UNBOUND: Self = Self {
        tag: RTag::None as u32,
        flags: 1,
        payload: 0,
    };

    #[must_use]
    pub const fn is_unbound(self) -> bool {
        self.tag == RTag::None as u32 && self.flags == 1 && self.payload == 0
    }

    #[must_use]
    pub const fn boolean(value: bool) -> Self {
        Self {
            tag: RTag::Bool as u32,
            flags: 0,
            payload: value as u64,
        }
    }

    #[must_use]
    pub const fn small_int(value: i64) -> Self {
        Self {
            tag: RTag::SmallInt as u32,
            flags: 0,
            payload: value.cast_unsigned(),
        }
    }

    #[must_use]
    pub fn handle(index: u32, generation: u32) -> Self {
        Self {
            tag: RTag::Handle as u32,
            flags: 0,
            payload: (u64::from(generation) << 32) | u64::from(index),
        }
    }

    #[must_use]
    pub fn handle_parts(self) -> Option<(u32, u32)> {
        if self.tag == RTag::Handle as u32 {
            let bytes = self.payload.to_le_bytes();
            let index = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let generation = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            Some((index, generation))
        } else {
            None
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RStatus {
    Ok = 0,
    Exception = 1,
    InvalidArgument = 2,
    AbiMismatch = 3,
}

/// Opaque generational identity for one submitted asynchronous root task.
///
/// The async runtime owns the slot/generation table. Python/runtime code may
/// retain this value for identity and wake/cancel operations, but no layer may
/// reinterpret it as a pointer.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RAsyncTaskId {
    pub index: u32,
    pub generation: u32,
}

/// Opaque 16-byte payload transported by the async facade without inspecting
/// Python `RValue`, exception, or frame layouts.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RAsyncOpaqueValue {
    pub low: u64,
    pub high: u64,
}

/// Terminal category produced by a submitted root task.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsyncCompletionKind {
    Returned = 0,
    Raised = 1,
    Cancelled = 2,
    Dropped = 3,
    DriverError = 4,
}

/// Executor-facade failures that do not originate from Python execution.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsyncDriverError {
    ReentrantPoll = 1,
    Shutdown = 2,
    RuntimeContract = 3,
}

/// Inline completion record. `payload` is runtime-owned opaque data for
/// `Returned`/`Raised`; `DriverError` uses `payload.low` for `RAsyncDriverError`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RAsyncCompletion {
    pub kind: RAsyncCompletionKind,
    pub reserved: [u8; 7],
    pub payload: RAsyncOpaqueValue,
}

impl RAsyncCompletion {
    #[must_use]
    pub const fn new(kind: RAsyncCompletionKind, payload: RAsyncOpaqueValue) -> Self {
        Self {
            kind,
            reserved: [0; 7],
            payload,
        }
    }

    #[must_use]
    pub const fn driver_error(error: RAsyncDriverError) -> Self {
        Self::new(
            RAsyncCompletionKind::DriverError,
            RAsyncOpaqueValue {
                low: error as u64,
                high: 0,
            },
        )
    }
}

/// Result of one backend poll of the narrow runtime resume callback.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsyncPollState {
    Pending = 0,
    Ready = 1,
}

/// Result of invoking an opaque task wake handle.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsyncWakeStatus {
    Woken = 0,
    NoRegisteredWaker = 1,
    StaleTask = 2,
    Coalesced = 3,
}

/// Cancellation lifecycle for one root task. Request, observation, and
/// acknowledgement are deliberately separate from terminal task completion.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsyncCancelState {
    Clear = 0,
    Requested = 1,
    Observed = 2,
    Acknowledged = 3,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RAsyncCancellation {
    pub request_id: u64,
    pub state: RAsyncCancelState,
    pub reserved: [u8; 7],
}

impl RAsyncCancellation {
    pub const CLEAR: Self = Self {
        request_id: 0,
        state: RAsyncCancelState::Clear,
        reserved: [0; 7],
    };
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsyncCancelTransition {
    Observe = 1,
    Acknowledge = 2,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsyncCancelTransitionStatus {
    Applied = 0,
    StaleRequest = 1,
    StaleTask = 2,
    InvalidTransition = 3,
}

/// Monotonic deadline relative to the owning executor-facade epoch. This is a
/// transport record, never wall-clock time.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RAsyncDeadline {
    pub nanos_from_epoch: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RAsyncTimerId {
    pub sequence: u64,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RAsyncTimerState {
    Registered = 0,
    Fired = 1,
    Cancelled = 2,
}

/// Backend-neutral wake callback. The `data` pointer is owned by the async
/// facade and remains valid only while the corresponding task/facade is alive.
pub type RAsyncWakeFn =
    unsafe extern "C" fn(data: *mut c_void, task: RAsyncTaskId) -> RAsyncWakeStatus;

/// Runtime acknowledgement of cancellation delivery. The facade validates
/// task generation and request identity before applying the transition.
pub type RAsyncCancelTransitionFn = unsafe extern "C" fn(
    data: *mut c_void,
    task: RAsyncTaskId,
    request_id: u64,
    transition: RAsyncCancelTransition,
) -> RAsyncCancelTransitionStatus;

/// Poll-local wake/cancellation view given to the runtime callback. The runtime
/// may retain the `(control_data, wake, task)` triple as an opaque wake handle;
/// it must not inspect `control_data`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RAsyncPollContext {
    pub task: RAsyncTaskId,
    pub control_data: *mut c_void,
    pub wake: RAsyncWakeFn,
    pub cancel_transition: RAsyncCancelTransitionFn,
    pub cancellation: RAsyncCancellation,
}

/// Narrow runtime callback used by one submitted root future. The executor
/// facade owns polling; the runtime owns Python coroutine/frame semantics and
/// writes an opaque terminal completion only when returning `Ready`.
pub type RAsyncResumeFn = unsafe extern "C" fn(
    runtime: *mut c_void,
    poll: *const RAsyncPollContext,
    completion: *mut RAsyncCompletion,
) -> RAsyncPollState;

/// Exactly-once cleanup callback used when an active root task is dropped or
/// the local facade is shut down before terminal completion.
pub type RAsyncDropFn = unsafe extern "C" fn(runtime: *mut c_void, task: RAsyncTaskId);

/// Native entry used to initialize one already-created managed module.
///
/// The signature intentionally matches ordinary non-generator compiled
/// functions so module initializers share the verified Cranelift body path.
pub type RNativeModuleInitializer = unsafe extern "C" fn(
    context: *mut c_void,
    module: *const RValue,
    arguments: *const RValue,
    argument_count: usize,
    output: *mut RValue,
) -> RStatus;

/// Operation requested when resuming a native generator.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RGeneratorOperation {
    Next = 0,
    Send = 1,
    Throw = 2,
    Close = 3,
}

/// Successful result category returned by a native generator resume entry.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RGeneratorOutcome {
    Yielded = 0,
    Returned = 1,
    /// Internal async suspension produced by `await`. Ordinary coroutines
    /// transport this like a yielded scheduler token; async-generator operation
    /// wrappers use the distinction so a delegated await never becomes a
    /// Python-visible generated item.
    Suspended = 2,
}

/// Result of forwarding one operation through a `yield from` delegate.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RGeneratorDelegateOutcome {
    Yielded = 0,
    Completed = 1,
    Propagate = 2,
}

/// Native protocol identifier for a unary Python operation.
///
/// The value is part of the compiler/runtime contract: MIR and generated code
/// transport this identifier without inventing stage-local operator tables.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RUnaryOperator {
    Positive = 0,
    Negate = 1,
    Invert = 2,
    Not = 3,
}

/// Native protocol identifier for the currently supported binary operations.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RBinaryOperator {
    Add = 0,
    Subtract = 1,
    Multiply = 2,
    FloorDivide = 3,
    Modulo = 4,
    TrueDivide = 5,
    Power = 6,
    LeftShift = 7,
    RightShift = 8,
    BitAnd = 9,
    BitXor = 10,
    BitOr = 11,
    MatrixMultiply = 12,
}

/// Native protocol identifier for the currently supported comparisons.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RCompareOperator {
    Equal = 0,
    NotEqual = 1,
    Less = 2,
    LessEqual = 3,
    Greater = 4,
    GreaterEqual = 5,
    In = 6,
    NotIn = 7,
    Is = 8,
    IsNot = 9,
}

/// F-string conversion applied before the ordinary `format(value, spec)` path.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RFormatConversion {
    None = 0,
    Str = 1,
    Repr = 2,
    Ascii = 3,
}

impl TryFrom<u8> for RFormatConversion {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Str),
            2 => Ok(Self::Repr),
            3 => Ok(Self::Ascii),
            _ => Err(()),
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RParameterKind {
    PositionalOnly = 0,
    PositionalOrKeyword = 1,
    VarArgs = 2,
    KeywordOnly = 3,
    VarKeywords = 4,
}

impl TryFrom<u8> for RParameterKind {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::PositionalOnly),
            1 => Ok(Self::PositionalOrKeyword),
            2 => Ok(Self::VarArgs),
            3 => Ok(Self::KeywordOnly),
            4 => Ok(Self::VarKeywords),
            _ => Err(()),
        }
    }
}

/// Python 3.12 PEP 695 parameter family transported by the native metadata ABI.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RTypeParameterKind {
    TypeVar = 0,
    TypeVarTuple = 1,
    ParamSpec = 2,
}

impl TryFrom<u8> for RTypeParameterKind {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::TypeVar),
            1 => Ok(Self::TypeVarTuple),
            2 => Ok(Self::ParamSpec),
            _ => Err(()),
        }
    }
}

/// Source-ordered call-site argument part transported between MIR/codegen and
/// the runtime accumulator. Final binding still happens through `rimera_call`'s
/// authoritative Python binder.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RCallArgumentKind {
    Positional = 0,
    Starred = 1,
    Keyword = 2,
    KeywordUnpack = 3,
}

impl TryFrom<u8> for RCallArgumentKind {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Positional),
            1 => Ok(Self::Starred),
            2 => Ok(Self::Keyword),
            3 => Ok(Self::KeywordUnpack),
            _ => Err(()),
        }
    }
}

#[repr(C)]
#[derive(Debug)]
pub struct RRootFrame {
    pub previous: *mut Self,
    pub slots: *mut RValue,
    pub len: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RParameterSpec {
    pub name: *const u8,
    pub name_len: usize,
    pub kind: u8,
    pub has_default: u8,
    pub reserved: [u8; 6],
    pub default: RValue,
}

/// One UTF-8 identifier transported as part of immutable code metadata.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RNameSpec {
    pub name: *const u8,
    pub name_len: usize,
}

/// Compiler-owned Python-visible metadata for one native code object.
///
/// Native code addresses are intentionally not part of this public metadata
/// record: the runtime receives the executable address separately and keeps it
/// opaque inside its managed code object.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RCodeMetadataSpec {
    pub filename: *const u8,
    pub filename_len: usize,
    pub first_line: u32,
    pub reserved: u32,
    pub local_names: *const RNameSpec,
    pub local_name_len: usize,
    pub cell_names: *const RNameSpec,
    pub cell_name_len: usize,
    pub free_names: *const RNameSpec,
    pub free_name_len: usize,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RKeywordArgument {
    pub name: *const u8,
    pub name_len: usize,
    pub value: RValue,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RCallArguments {
    pub positional: *const RValue,
    pub positional_len: usize,
    pub keywords: *const RKeywordArgument,
    pub keyword_len: usize,
}

pub type RNativeFunction = unsafe extern "C" fn(
    context: *mut c_void,
    function: *const RValue,
    bound_arguments: *const RValue,
    bound_argument_count: usize,
    output: *mut RValue,
) -> RStatus;

/// Native entry emitted for a generator suspension state machine.
pub type RNativeGeneratorResume = unsafe extern "C" fn(
    context: *mut c_void,
    generator: *const RValue,
    operation: RGeneratorOperation,
    input: *const RValue,
    output: *mut RValue,
    outcome: *mut RGeneratorOutcome,
) -> RStatus;

impl RRootFrame {
    #[must_use]
    pub const fn new(slots: *mut RValue, len: usize) -> Self {
        Self {
            previous: ptr::null_mut(),
            slots,
            len,
        }
    }
}

const _: () = assert!(size_of::<RValue>() == 16);
const _: () = assert!(align_of::<RValue>() == 8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_round_trips_slot_and_generation() {
        assert_eq!(RValue::handle(17, 9).handle_parts(), Some((17, 9)));
    }

    #[test]
    fn abi_layout_is_stable() {
        assert_eq!(size_of::<RValue>(), 16);
        assert_eq!(RStatus::Ok as i32, 0);
        assert_eq!(RTag::Handle as u32, 3);
        assert_eq!(size_of::<RCallArguments>(), 32);
        assert_eq!(size_of::<RKeywordArgument>(), 32);
        assert_eq!(size_of::<RNameSpec>(), 16);
        assert_eq!(size_of::<RCodeMetadataSpec>(), 72);
        assert_eq!(size_of::<RAsyncTaskId>(), 8);
        assert_eq!(align_of::<RAsyncTaskId>(), 4);
        assert_eq!(core::mem::offset_of!(RAsyncTaskId, index), 0);
        assert_eq!(core::mem::offset_of!(RAsyncTaskId, generation), 4);
        assert_eq!(size_of::<RAsyncOpaqueValue>(), 16);
        assert_eq!(align_of::<RAsyncOpaqueValue>(), 8);
        assert_eq!(size_of::<RAsyncCompletion>(), 24);
        assert_eq!(align_of::<RAsyncCompletion>(), 8);
        assert_eq!(core::mem::offset_of!(RAsyncCompletion, kind), 0);
        assert_eq!(core::mem::offset_of!(RAsyncCompletion, reserved), 1);
        assert_eq!(core::mem::offset_of!(RAsyncCompletion, payload), 8);
        assert_eq!(size_of::<RAsyncCancellation>(), 16);
        assert_eq!(align_of::<RAsyncCancellation>(), 8);
        assert_eq!(core::mem::offset_of!(RAsyncCancellation, request_id), 0);
        assert_eq!(core::mem::offset_of!(RAsyncCancellation, state), 8);
        assert_eq!(core::mem::offset_of!(RAsyncCancellation, reserved), 9);
        assert_eq!(size_of::<RAsyncPollContext>(), 48);
        assert_eq!(align_of::<RAsyncPollContext>(), 8);
        assert_eq!(core::mem::offset_of!(RAsyncPollContext, task), 0);
        assert_eq!(core::mem::offset_of!(RAsyncPollContext, control_data), 8);
        assert_eq!(core::mem::offset_of!(RAsyncPollContext, wake), 16);
        assert_eq!(
            core::mem::offset_of!(RAsyncPollContext, cancel_transition),
            24
        );
        assert_eq!(core::mem::offset_of!(RAsyncPollContext, cancellation), 32);
        assert_eq!(size_of::<RAsyncDeadline>(), 8);
        assert_eq!(size_of::<RAsyncTimerId>(), 8);
        assert_eq!(RAsyncCompletionKind::Returned as u8, 0);
        assert_eq!(RAsyncCompletionKind::DriverError as u8, 4);
        assert_eq!(RAsyncDriverError::ReentrantPoll as u8, 1);
        assert_eq!(RAsyncDriverError::RuntimeContract as u8, 3);
        assert_eq!(RAsyncPollState::Pending as u8, 0);
        assert_eq!(RAsyncWakeStatus::StaleTask as u8, 2);
        assert_eq!(RAsyncWakeStatus::Coalesced as u8, 3);
        assert_eq!(RAsyncCancelState::Clear as u8, 0);
        assert_eq!(RAsyncCancelState::Acknowledged as u8, 3);
        assert_eq!(RAsyncCancelTransition::Observe as u8, 1);
        assert_eq!(RAsyncCancelTransitionStatus::InvalidTransition as u8, 3);
        assert_eq!(RAsyncTimerState::Cancelled as u8, 2);
    }

    #[test]
    fn protocol_operator_identifiers_are_stable() {
        assert_eq!(RUnaryOperator::Positive as u8, 0);
        assert_eq!(RUnaryOperator::Not as u8, 3);
        assert_eq!(RBinaryOperator::Add as u8, 0);
        assert_eq!(RBinaryOperator::Modulo as u8, 4);
        assert_eq!(RBinaryOperator::BitOr as u8, 11);
        assert_eq!(RCompareOperator::Equal as u8, 0);
        assert_eq!(RCompareOperator::NotIn as u8, 7);
        assert_eq!(RCallArgumentKind::Positional as u8, 0);
        assert_eq!(RCallArgumentKind::KeywordUnpack as u8, 3);
        assert_eq!(RFormatConversion::None as u8, 0);
        assert_eq!(RFormatConversion::Ascii as u8, 3);
    }
}
