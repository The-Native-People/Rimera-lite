use core::ffi::c_void;
use core::ptr;

pub const ABI_VERSION: u32 = 1;

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
