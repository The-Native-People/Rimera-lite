use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive, Zero};
use rimera_abi::{RGeneratorOperation, RGeneratorOutcome, RTag, RValue};

use crate::RimeraContext;
use crate::heap::HeapObject;
use crate::object::{
    DictionaryViewKind, DictionaryViewObject, IteratorObject, MemoryViewObject, RangeObject,
    SetObject, ValueDictionaryObject,
};

fn instance_storage(context: &RimeraContext, value: RValue) -> Option<RValue> {
    match context.heap.get(value) {
        Some(HeapObject::Instance(instance)) => instance.storage,
        _ => None,
    }
}

pub fn int_from_decimal(context: &mut RimeraContext, text: &str) -> Result<RValue, String> {
    let value = text
        .parse::<BigInt>()
        .map_err(|_| "invalid integer literal".to_owned())?;
    store_integer(context, value)
}

pub fn string(context: &mut RimeraContext, value: &str) -> Result<RValue, String> {
    context.allocate(HeapObject::String(value.to_owned()))
}

pub fn float(context: &mut RimeraContext, value: f64) -> Result<RValue, String> {
    context.allocate(HeapObject::Float(value))
}

pub fn complex(context: &mut RimeraContext, real: f64, imag: f64) -> Result<RValue, String> {
    context.allocate(HeapObject::Complex { real, imag })
}

pub fn bytes(context: &mut RimeraContext, value: &[u8]) -> Result<RValue, String> {
    context.allocate(HeapObject::Bytes(value.to_vec()))
}

pub fn bytearray(context: &mut RimeraContext, value: &[u8]) -> Result<RValue, String> {
    context.allocate(HeapObject::ByteArray(crate::object::ByteArrayObject {
        bytes: value.to_vec(),
        exports: 0,
    }))
}

pub fn slice(
    context: &mut RimeraContext,
    start: Option<RValue>,
    stop: Option<RValue>,
    step: Option<RValue>,
) -> Result<RValue, String> {
    let roots = [start, stop, step]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    context.with_temporary_roots(&roots, |context| {
        context.allocate(HeapObject::Slice(crate::object::SliceObject {
            start,
            stop,
            step,
        }))
    })
}

pub fn value_array(context: &mut RimeraContext, values: &[RValue]) -> Result<RValue, String> {
    context.with_temporary_roots(values, |context| {
        context.allocate(HeapObject::ValueArray(values.to_vec().into_boxed_slice()))
    })
}

pub fn list(context: &mut RimeraContext, values: &[RValue]) -> Result<RValue, String> {
    context.with_temporary_roots(values, |context| {
        context.allocate(HeapObject::List(values.to_vec()))
    })
}

pub fn tuple(context: &mut RimeraContext, values: &[RValue]) -> Result<RValue, String> {
    context.with_temporary_roots(values, |context| {
        context.allocate(HeapObject::Tuple(values.to_vec().into_boxed_slice()))
    })
}

/// Creates an insertion-ordered dictionary.  The current object kernel keeps
/// builtin immutable keys in a compact ordered table; equality is Python
/// equality for the supported key domain rather than raw handle identity.
pub fn dictionary(
    context: &mut RimeraContext,
    keys: &[RValue],
    values: &[RValue],
) -> Result<RValue, String> {
    if keys.len() != values.len() {
        return Err("dictionary key/value arrays have different lengths".to_owned());
    }
    let mut roots = keys.to_vec();
    roots.extend_from_slice(values);
    context.with_temporary_roots(&roots, |context| {
        let mut table = crate::object::OrderedHashTable::default();
        for (&key, &value) in keys.iter().zip(values) {
            let hash = hash_i64(context, key)?;
            let candidates = table
                .candidates(hash)
                .into_iter()
                .filter_map(|index| table.value(index).copied().map(|entry| (index, entry)))
                .collect::<Vec<_>>();
            let mut replacement = None;
            for (index, (existing, _)) in candidates {
                if value_equal(context, existing, key)? {
                    replacement = Some(index);
                    break;
                }
            }
            if let Some(index) = replacement {
                table.update(index, |(_, existing)| *existing = value);
            } else {
                table.insert_new(hash, (key, value));
            }
        }
        context.allocate(HeapObject::ValueDictionary(ValueDictionaryObject { table }))
    })
}

pub fn set(context: &mut RimeraContext, values: &[RValue]) -> Result<RValue, String> {
    context.with_temporary_roots(values, |context| {
        let mut table = crate::object::OrderedHashTable::default();
        for &value in values {
            let hash = hash_i64(context, value)?;
            let candidates = table
                .candidates(hash)
                .into_iter()
                .filter_map(|index| table.value(index).copied())
                .collect::<Vec<_>>();
            let mut existing = false;
            for candidate in candidates {
                if value_equal(context, candidate, value)? {
                    existing = true;
                    break;
                }
            }
            if !existing {
                table.insert_new(hash, value);
            }
        }
        context.allocate(HeapObject::Set(SetObject { table }))
    })
}

pub fn frozenset(context: &mut RimeraContext, values: &[RValue]) -> Result<RValue, String> {
    context.with_temporary_roots(values, |context| {
        let mut table = crate::object::OrderedHashTable::default();
        for &value in values {
            let hash = hash_i64(context, value)?;
            let candidates = table
                .candidates(hash)
                .into_iter()
                .filter_map(|index| table.value(index).copied())
                .collect::<Vec<_>>();
            let mut existing = false;
            for candidate in candidates {
                if value_equal(context, candidate, value)? {
                    existing = true;
                    break;
                }
            }
            if !existing {
                table.insert_new(hash, value);
            }
        }
        context.allocate(HeapObject::FrozenSet(SetObject { table }))
    })
}

pub fn set_add(context: &mut RimeraContext, receiver: RValue, value: RValue) -> Result<(), String> {
    context.with_temporary_roots(&[receiver, value], |context| {
        let hash = hash_i64(context, value)?;
        'restart: loop {
            let candidates = match context.heap.get(receiver) {
                Some(HeapObject::Set(set)) => set.table.candidates(hash),
                Some(HeapObject::FrozenSet(_)) => {
                    return Err("'frozenset' object has no attribute 'add'".to_owned());
                }
                Some(_) => return Err("operation requires a set".to_owned()),
                None => return Err("value contains a stale heap handle".to_owned()),
            };
            for position in candidates {
                let (candidate, version_before) = match context.heap.get(receiver) {
                    Some(HeapObject::Set(set)) => {
                        let Some(candidate) = set.table.value(position).copied() else {
                            continue;
                        };
                        (candidate, set.table.version)
                    }
                    _ => return Err("operation requires a set".to_owned()),
                };
                if candidate == value || value_equal(context, candidate, value)? {
                    // CPython's set insertion treats an equality hit as present even
                    // when the equality callback itself removed that slot.
                    return Ok(());
                }
                let version_after = match context.heap.get(receiver) {
                    Some(HeapObject::Set(set)) => set.table.version,
                    _ => return Err("operation requires a set".to_owned()),
                };
                if version_after != version_before {
                    continue 'restart;
                }
            }
            let Some(HeapObject::Set(set)) = context.heap.get_mut(receiver) else {
                return Err("operation requires a set".to_owned());
            };
            set.table.insert_new(hash, value);
            return Ok(());
        }
    })
}

pub fn set_discard(
    context: &mut RimeraContext,
    receiver: RValue,
    value: RValue,
) -> Result<(), String> {
    context.with_temporary_roots(&[receiver, value], |context| {
        if matches!(context.heap.get(receiver), Some(HeapObject::FrozenSet(_))) {
            return Err("'frozenset' object has no attribute 'discard'".to_owned());
        }
        if !matches!(context.heap.get(receiver), Some(HeapObject::Set(_))) {
            return Err("operation requires a set".to_owned());
        }
        let hash = hash_i64(context, value)?;
        let Some(position) = set_find_entry(context, receiver, value, hash)? else {
            return Ok(());
        };
        let Some(HeapObject::Set(set)) = context.heap.get_mut(receiver) else {
            return Err("operation requires a set".to_owned());
        };
        set.table.remove(position);
        Ok(())
    })
}

pub fn collect_iterable(context: &mut RimeraContext, value: RValue) -> Result<Vec<RValue>, String> {
    context.with_temporary_roots(&[value], |context| {
        let iterator = iterator_new(context, value)?;
        context.with_temporary_roots(&[iterator], |context| {
            let mut values = Vec::new();
            loop {
                let next = context
                    .with_temporary_roots(&values, |context| iterator_next(context, iterator))?;
                let Some(value) = next else {
                    break;
                };
                values.push(value);
            }
            Ok(values)
        })
    })
}

pub fn enumerate(
    context: &mut RimeraContext,
    iterable: RValue,
    start: RValue,
) -> Result<RValue, String> {
    context.with_temporary_roots(&[iterable, start], |context| {
        let source = iterator_new(context, iterable)?;
        let index = index_integer(context, start)?;
        context.with_temporary_roots(&[source], |context| {
            context.allocate(HeapObject::Iterator(IteratorObject::Enumerate {
                source,
                index,
            }))
        })
    })
}

pub fn zip(context: &mut RimeraContext, iterables: &[RValue]) -> Result<RValue, String> {
    zip_with_strict(context, iterables, false)
}

pub fn zip_with_strict(
    context: &mut RimeraContext,
    iterables: &[RValue],
    strict: bool,
) -> Result<RValue, String> {
    context.with_temporary_roots(iterables, |context| {
        let sources = iterables
            .iter()
            .map(|value| iterator_new(context, *value))
            .collect::<Result<Vec<_>, _>>()?;
        context.with_temporary_roots(&sources.clone(), |context| {
            context.allocate(HeapObject::Iterator(IteratorObject::Zip {
                sources: sources.into_boxed_slice(),
                strict,
            }))
        })
    })
}

pub fn call_sentinel_iterator(
    context: &mut RimeraContext,
    callable: RValue,
    sentinel: RValue,
) -> Result<RValue, String> {
    context.with_temporary_roots(&[callable, sentinel], |context| {
        context.allocate(HeapObject::Iterator(IteratorObject::CallSentinel {
            callable,
            sentinel,
        }))
    })
}
pub fn map(
    context: &mut RimeraContext,
    callable: RValue,
    iterables: &[RValue],
) -> Result<RValue, String> {
    context.with_temporary_roots(&[callable], |context| {
        let iterator = zip(context, iterables)?;
        let sources = match context.heap.get(iterator) {
            Some(HeapObject::Iterator(IteratorObject::Zip { sources, .. })) => sources.to_vec(),
            _ => unreachable!("zip creates a zip iterator"),
        };
        context.with_temporary_roots(&sources.clone(), |context| {
            context.allocate(HeapObject::Iterator(IteratorObject::Map {
                callable,
                sources: sources.into_boxed_slice(),
            }))
        })
    })
}

pub fn filter(
    context: &mut RimeraContext,
    predicate: RValue,
    iterable: RValue,
) -> Result<RValue, String> {
    context.with_temporary_roots(&[predicate, iterable], |context| {
        let source = iterator_new(context, iterable)?;
        context.with_temporary_roots(&[source], |context| {
            context.allocate(HeapObject::Iterator(IteratorObject::Filter {
                predicate,
                source,
            }))
        })
    })
}

pub fn byte_values(context: &mut RimeraContext, value: RValue) -> Result<Vec<u8>, String> {
    match context.heap.get(value) {
        Some(HeapObject::Bytes(values)) => Ok(values.clone()),
        Some(HeapObject::ByteArray(values)) => Ok(values.bytes.clone()),
        Some(HeapObject::MemoryView(view)) => {
            let view = view.clone();
            memoryview_bytes(context, &view)
        }
        Some(HeapObject::String(_)) => Err("string argument without an encoding".to_owned()),
        _ => {
            let values = collect_iterable(context, value)?;
            let mut bytes = Vec::with_capacity(values.len());
            for value in values {
                let integer = index_integer(context, value)?;
                let Some(byte) = integer.to_u8() else {
                    return context.raise_error("ValueError", "bytes must be in range(0, 256)");
                };
                bytes.push(byte);
            }
            Ok(bytes)
        }
    }
}

pub fn dictionary_view(
    context: &mut RimeraContext,
    dictionary: RValue,
    kind: DictionaryViewKind,
) -> Result<RValue, String> {
    match context.heap.get(dictionary) {
        Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_)) => context
            .with_temporary_roots(&[dictionary], |context| {
                context.allocate(HeapObject::DictionaryView(DictionaryViewObject {
                    dictionary,
                    kind,
                }))
            }),
        Some(_) => Err("dictionary view requires a dictionary".to_owned()),
        None => Err("value contains a stale heap handle".to_owned()),
    }
}

fn allocate_memoryview_object(
    context: &mut RimeraContext,
    view: MemoryViewObject,
) -> Result<RValue, String> {
    let exporter = view.exporter;
    let exported_bytearray = matches!(context.heap.get(exporter), Some(HeapObject::ByteArray(_)));
    if exported_bytearray && let Some(HeapObject::ByteArray(bytes)) = context.heap.get_mut(exporter)
    {
        bytes.exports = bytes.exports.saturating_add(1);
    }
    match context.allocate(HeapObject::MemoryView(view)) {
        Ok(value) => Ok(value),
        Err(error) => {
            if exported_bytearray
                && let Some(HeapObject::ByteArray(bytes)) = context.heap.get_mut(exporter)
            {
                bytes.exports = bytes.exports.saturating_sub(1);
            }
            Err(error)
        }
    }
}

pub fn memoryview(context: &mut RimeraContext, exporter: RValue) -> Result<RValue, String> {
    context.with_temporary_roots(&[exporter], |context| {
        let view = match context.heap.get(exporter) {
            Some(HeapObject::Bytes(bytes)) => MemoryViewObject {
                exporter,
                format: "B".to_owned(),
                item_size: 1,
                shape: vec![bytes.len()].into_boxed_slice(),
                strides: vec![1].into_boxed_slice(),
                suboffsets: Box::new([]),
                offset: 0,
                readonly: true,
                released: false,
            },
            Some(HeapObject::ByteArray(bytes)) => MemoryViewObject {
                exporter,
                format: "B".to_owned(),
                item_size: 1,
                shape: vec![bytes.bytes.len()].into_boxed_slice(),
                strides: vec![1].into_boxed_slice(),
                suboffsets: Box::new([]),
                offset: 0,
                readonly: false,
                released: false,
            },
            Some(HeapObject::MemoryView(parent)) if !parent.released => MemoryViewObject {
                exporter: parent.exporter,
                format: parent.format.clone(),
                item_size: parent.item_size,
                shape: parent.shape.clone(),
                strides: parent.strides.clone(),
                suboffsets: parent.suboffsets.clone(),
                offset: parent.offset,
                readonly: parent.readonly,
                released: false,
            },
            Some(HeapObject::MemoryView(_)) => {
                return Err("operation forbidden on released memoryview object".to_owned());
            }
            Some(_) => return Err("memoryview: a bytes-like object is required".to_owned()),
            None => return Err("value contains a stale heap handle".to_owned()),
        };
        context.with_temporary_roots(&[view.exporter], |context| {
            allocate_memoryview_object(context, view)
        })
    })
}

pub fn memoryview_release(context: &mut RimeraContext, value: RValue) -> Result<(), String> {
    let exporter = match context.heap.get(value) {
        Some(HeapObject::MemoryView(view)) => view.exporter,
        Some(_) => return Err("operation requires a memoryview".to_owned()),
        None => return Err("value contains a stale heap handle".to_owned()),
    };
    let Some(HeapObject::MemoryView(view)) = context.heap.get_mut(value) else {
        unreachable!()
    };
    if view.released {
        return Ok(());
    }
    view.released = true;
    if let Some(HeapObject::ByteArray(bytes)) = context.heap.get_mut(exporter) {
        bytes.exports = bytes.exports.saturating_sub(1);
    }
    Ok(())
}

/// Materializes a native bytes value from a view without exposing exporter
/// pointers to generated code.
pub fn memoryview_to_bytes(context: &mut RimeraContext, value: RValue) -> Result<RValue, String> {
    memoryview_to_bytes_order(context, value, "C")
}

pub fn memoryview_to_bytes_order(
    context: &mut RimeraContext,
    value: RValue,
    order: &str,
) -> Result<RValue, String> {
    let view = match context.heap.get(value) {
        Some(HeapObject::MemoryView(view)) if !view.released => view.clone(),
        Some(HeapObject::MemoryView(_)) => {
            return Err("operation forbidden on released memoryview object".to_owned());
        }
        Some(_) => return Err("operation requires a memoryview".to_owned()),
        None => return Err("value contains a stale heap handle".to_owned()),
    };
    let data = match order {
        "C" | "A" => memoryview_raw_bytes(context, &view)?,
        "F" => memoryview_raw_bytes_fortran(context, &view)?,
        _ => return Err("order must be 'C', 'F' or 'A'".to_owned()),
    };
    bytes(context, &data)
}

fn memoryview_list_dimension(
    context: &mut RimeraContext,
    view: &MemoryViewObject,
    axis: usize,
    indices: &mut Vec<usize>,
) -> Result<RValue, String> {
    if axis == view.shape.len() {
        return memoryview_scalar_get(context, view, indices);
    }
    let mut values = Vec::with_capacity(view.shape[axis]);
    for index in 0..view.shape[axis] {
        indices.push(index);
        values.push(memoryview_list_dimension(context, view, axis + 1, indices)?);
        indices.pop();
    }
    list(context, &values)
}

pub fn memoryview_to_list(context: &mut RimeraContext, value: RValue) -> Result<RValue, String> {
    let view = match context.heap.get(value) {
        Some(HeapObject::MemoryView(view)) if !view.released => view.clone(),
        Some(HeapObject::MemoryView(_)) => {
            return Err("operation forbidden on released memoryview object".to_owned());
        }
        Some(_) => return Err("operation requires a memoryview".to_owned()),
        None => return Err("value contains a stale heap handle".to_owned()),
    };
    memoryview_list_dimension(context, &view, 0, &mut Vec::with_capacity(view.shape.len()))
}

pub fn memoryview_to_readonly(
    context: &mut RimeraContext,
    value: RValue,
) -> Result<RValue, String> {
    let mut view = match context.heap.get(value) {
        Some(HeapObject::MemoryView(view)) if !view.released => view.clone(),
        Some(HeapObject::MemoryView(_)) => {
            return Err("operation forbidden on released memoryview object".to_owned());
        }
        Some(_) => return Err("operation requires a memoryview".to_owned()),
        None => return Err("value contains a stale heap handle".to_owned()),
    };
    view.readonly = true;
    context.with_temporary_roots(&[view.exporter], |context| {
        allocate_memoryview_object(context, view)
    })
}

pub fn memoryview_hex(
    context: &mut RimeraContext,
    value: RValue,
    separator: Option<u8>,
    bytes_per_sep: isize,
) -> Result<RValue, String> {
    let data = match context.heap.get(value) {
        Some(HeapObject::MemoryView(view)) => memoryview_bytes(context, view)?,
        Some(_) => return Err("operation requires a memoryview".to_owned()),
        None => return Err("value contains a stale heap handle".to_owned()),
    };
    let mut groups = Vec::new();
    let width = bytes_per_sep.unsigned_abs();
    if separator.is_none() || width == 0 || data.is_empty() {
        groups.push(data.as_slice());
    } else if bytes_per_sep > 0 {
        let first = data.len() % width;
        let mut start = 0;
        if first != 0 {
            groups.push(&data[..first]);
            start = first;
        }
        while start < data.len() {
            groups.push(&data[start..(start + width).min(data.len())]);
            start += width;
        }
    } else {
        let mut start = 0;
        while start < data.len() {
            groups.push(&data[start..(start + width).min(data.len())]);
            start += width;
        }
    }
    let separator = separator.map(char::from).map(|value| value.to_string());
    let mut rendered_groups = Vec::with_capacity(groups.len());
    for group in groups {
        let mut text = String::with_capacity(group.len().saturating_mul(2));
        for byte in group {
            use std::fmt::Write as _;
            write!(&mut text, "{byte:02x}").expect("writing to String cannot fail");
        }
        rendered_groups.push(text);
    }
    string(
        context,
        &rendered_groups.join(separator.as_deref().unwrap_or("")),
    )
}

pub fn memoryview_cast(
    context: &mut RimeraContext,
    value: RValue,
    format: &str,
    shape: Option<RValue>,
) -> Result<RValue, String> {
    let view = match context.heap.get(value) {
        Some(HeapObject::MemoryView(view)) if !view.released => view.clone(),
        Some(HeapObject::MemoryView(_)) => {
            return Err("operation forbidden on released memoryview object".to_owned());
        }
        Some(_) => return Err("operation requires a memoryview".to_owned()),
        None => return Err("value contains a stale heap handle".to_owned()),
    };
    let item_size = match buffer_format_size(format) {
        Ok(size) => size,
        Err(message) => return context.raise_error("ValueError", message),
    };
    let source_code = buffer_format_code(&view.format)
        .map_err(|_| "memoryview: source format is not supported".to_owned())?;
    let destination_code = match buffer_format_code(format) {
        Ok(code) => code,
        Err(message) => return context.raise_error("ValueError", message),
    };
    let source_is_byte = matches!(source_code, 'B' | 'b' | 'c');
    let destination_is_byte = matches!(destination_code, 'B' | 'b' | 'c');
    if !source_is_byte && !destination_is_byte {
        return context.raise_error(
            "TypeError",
            "memoryview: cannot cast between two non-byte formats",
        );
    }
    if !memoryview_is_contiguous(&view) {
        return context.raise_error(
            "TypeError",
            "memoryview: casts are restricted to C-contiguous views",
        );
    }
    if view
        .offset
        .checked_add(
            view.shape
                .iter()
                .product::<usize>()
                .saturating_mul(view.item_size),
        )
        .is_none()
    {
        return Err("memoryview bounds overflow".to_owned());
    }
    let byte_len = view
        .shape
        .iter()
        .product::<usize>()
        .saturating_mul(view.item_size);
    let shape = match shape {
        None => vec![byte_len / item_size],
        Some(shape) => {
            let values = match context.heap.get(shape) {
                Some(HeapObject::Tuple(values)) => values.to_vec(),
                Some(HeapObject::List(values)) => values.clone(),
                _ => {
                    return context.raise_error("TypeError", "shape must be a list or a tuple");
                }
            };
            let mut dimensions = Vec::with_capacity(values.len());
            for value in values {
                let dimension = index_integer(context, value)?;
                let Some(dimension) = dimension.to_usize().filter(|dimension| *dimension > 0)
                else {
                    return context.raise_error(
                        "ValueError",
                        "memoryview.cast(): elements of shape must be integers > 0",
                    );
                };
                dimensions.push(dimension);
            }
            dimensions
        }
    };
    let elements = shape.iter().try_fold(1_usize, |total, dimension| {
        total
            .checked_mul(*dimension)
            .ok_or_else(|| "memoryview: product(shape) is too large".to_owned())
    })?;
    if elements.checked_mul(item_size) != Some(byte_len) {
        return context.raise_error(
            "TypeError",
            "memoryview: product(shape) * itemsize != buffer size",
        );
    }
    let mut strides = vec![item_size as isize; shape.len()];
    for index in (0..shape.len().saturating_sub(1)).rev() {
        strides[index] = strides[index + 1].saturating_mul(shape[index + 1] as isize);
    }
    context.with_temporary_roots(&[view.exporter], |context| {
        allocate_memoryview_object(
            context,
            MemoryViewObject {
                exporter: view.exporter,
                format: format.to_owned(),
                item_size,
                shape: shape.into_boxed_slice(),
                strides: strides.into_boxed_slice(),
                suboffsets: Box::new([]),
                offset: view.offset,
                readonly: view.readonly,
                released: false,
            },
        )
    })
}

pub fn unpack(
    context: &mut RimeraContext,
    value: RValue,
    before_count: usize,
    after_count: usize,
    starred: bool,
) -> Result<RValue, String> {
    let expected = before_count
        .checked_add(after_count)
        .ok_or_else(|| "too many unpacking targets".to_owned())?;
    context.with_temporary_roots(&[value], |context| {
        let iterator = iterator_new(context, value)?;
        context.with_temporary_roots(&[iterator], |context| {
            let mut consumed = Vec::with_capacity(expected.saturating_add(usize::from(starred)));

            for _ in 0..before_count {
                let next = context
                    .with_temporary_roots(&consumed, |context| iterator_next(context, iterator))?;
                let Some(item) = next else {
                    let got = consumed.len();
                    let message = if starred {
                        format!(
                            "not enough values to unpack (expected at least {expected}, got {got})"
                        )
                    } else {
                        format!("not enough values to unpack (expected {expected}, got {got})")
                    };
                    return context.raise_error("ValueError", message);
                };
                consumed.push(item);
            }

            if !starred {
                let extra = context
                    .with_temporary_roots(&consumed, |context| iterator_next(context, iterator))?;
                if extra.is_some() {
                    return context.raise_error(
                        "ValueError",
                        format!("too many values to unpack (expected {expected})"),
                    );
                }
                return context
                    .with_temporary_roots(&consumed, |context| value_array(context, &consumed));
            }

            while let Some(item) = context
                .with_temporary_roots(&consumed, |context| iterator_next(context, iterator))?
            {
                consumed.push(item);
            }

            if consumed.len() < expected {
                return context.raise_error(
                    "ValueError",
                    format!(
                        "not enough values to unpack (expected at least {expected}, got {})",
                        consumed.len()
                    ),
                );
            }

            let starred_end = consumed.len() - after_count;
            let starred_values = &consumed[before_count..starred_end];
            let starred_list =
                context.with_temporary_roots(&consumed, |context| list(context, starred_values))?;

            let mut result = Vec::with_capacity(expected.saturating_add(1));
            result.extend_from_slice(&consumed[..before_count]);
            result.push(starred_list);
            result.extend_from_slice(&consumed[starred_end..]);

            let mut roots = consumed;
            roots.push(starred_list);
            context.with_temporary_roots(&roots, |context| value_array(context, &result))
        })
    })
}

pub fn range(
    context: &mut RimeraContext,
    start: RValue,
    stop: RValue,
    step: RValue,
) -> Result<RValue, String> {
    context.with_temporary_roots(&[start, stop, step], |context| {
        let start = index_integer(context, start)?;
        let stop = index_integer(context, stop)?;
        let step = index_integer(context, step)?;
        if step.is_zero() {
            return Err("range() arg 3 must not be zero".to_owned());
        }
        context.allocate(HeapObject::Range(RangeObject { start, stop, step }))
    })
}
pub fn iterator_new(context: &mut RimeraContext, value: RValue) -> Result<RValue, String> {
    context.with_temporary_roots(&[value], |context| {
        if let Some(storage) = instance_storage(context, value) {
            if let Some(iterator) = context.invoke_special_method(value, "__iter__", &[])? {
                return Ok(iterator);
            }
            return iterator_new(context, storage);
        }
        let iterator = match context.heap.get(value) {
            Some(HeapObject::Range(range)) => IteratorObject::Range {
                current: range.start.clone(),
                stop: range.stop.clone(),
                step: range.step.clone(),
            },
            Some(HeapObject::List(_))
            | Some(HeapObject::Tuple(_))
            | Some(HeapObject::String(_))
            | Some(HeapObject::Bytes(_))
            | Some(HeapObject::ByteArray(_))
            | Some(HeapObject::MemoryView(_))
            | Some(HeapObject::DictionaryView(_))
            | Some(HeapObject::MappingProxy(_))
            | Some(HeapObject::Dictionary(_))
            | Some(HeapObject::ValueDictionary(_))
            | Some(HeapObject::Set(_))
            | Some(HeapObject::FrozenSet(_)) => IteratorObject::Sequence {
                source: value,
                index: 0,
                expected_version: collection_version(context, value),
            },
            Some(HeapObject::Generator(_)) => return Ok(value),
            Some(HeapObject::Iterator(_)) => return Ok(value),
            Some(_) => {
                if let Some(iterator) = context.invoke_special_method(value, "__iter__", &[])? {
                    if matches!(
                        context.heap.get(iterator),
                        Some(HeapObject::Iterator(_) | HeapObject::Generator(_))
                    ) || context.special_method(iterator, "__next__")?.is_some()
                    {
                        return Ok(iterator);
                    }
                    return Err("iter() returned non-iterator".to_owned());
                }
                if context.special_method(value, "__getitem__")?.is_some() {
                    return context.allocate(HeapObject::Iterator(
                        IteratorObject::SequenceProtocol {
                            source: value,
                            index: 0,
                        },
                    ));
                }
                return Err("object is not iterable".to_owned());
            }
            None => return Err("value contains a stale heap handle".to_owned()),
        };
        context.allocate(HeapObject::Iterator(iterator))
    })
}

pub fn iterator_next(
    context: &mut RimeraContext,
    iterator: RValue,
) -> Result<Option<RValue>, String> {
    if matches!(context.heap.get(iterator), Some(HeapObject::Generator(_))) {
        return match context.resume_generator(iterator, RGeneratorOperation::Next, RValue::NONE)? {
            crate::context::GeneratorResume {
                value,
                outcome: RGeneratorOutcome::Yielded,
            } => Ok(Some(value)),
            crate::context::GeneratorResume {
                outcome: RGeneratorOutcome::Returned,
                ..
            } => Ok(None),
        };
    }
    let Some(object) = context.heap.get_mut(iterator) else {
        return Err("value contains a stale heap handle".to_owned());
    };
    match object {
        HeapObject::Iterator(IteratorObject::Range {
            current,
            stop,
            step,
        }) => {
            let active = if step.sign() == num_bigint::Sign::Minus {
                current > stop
            } else {
                current < stop
            };
            if !active {
                return Ok(None);
            }
            let value = current.clone();
            *current += step.clone();
            store_integer(context, value).map(Some)
        }
        HeapObject::Iterator(IteratorObject::Sequence {
            source,
            index,
            expected_version,
        }) => {
            let source = *source;
            let current = *index;
            *index += 1;
            if let Some(expected) = *expected_version
                && collection_version(context, source) != Some(expected)
            {
                return context
                    .raise_error("RuntimeError", "dictionary changed size during iteration");
            }
            match context.heap.get(source) {
                Some(HeapObject::List(values)) => Ok(values.get(current).copied()),
                Some(HeapObject::Tuple(values)) => Ok(values.get(current).copied()),
                Some(HeapObject::String(value)) => value
                    .chars()
                    .nth(current)
                    .map(|character| string(context, &character.to_string()))
                    .transpose(),
                Some(HeapObject::Bytes(value)) => Ok(value
                    .get(current)
                    .copied()
                    .map(|byte| RValue::small_int(i64::from(byte)))),
                Some(HeapObject::ByteArray(value)) => Ok(value
                    .bytes
                    .get(current)
                    .copied()
                    .map(|byte| RValue::small_int(i64::from(byte)))),
                Some(HeapObject::MemoryView(view)) => {
                    let view = view.clone();
                    if view.released {
                        return Err("operation forbidden on released memoryview object".to_owned());
                    }
                    match view.shape.len() {
                        0 => return Err("invalid indexing of 0-dim memory".to_owned()),
                        1 => {}
                        _ => {
                            return Err(
                                "multi-dimensional sub-views are not implemented".to_owned()
                            );
                        }
                    }
                    if current >= view.shape[0] {
                        return Ok(None);
                    }
                    memoryview_scalar_get(context, &view, &[current]).map(Some)
                }
                Some(HeapObject::Dictionary(values)) => {
                    let key = values.entries.get(current).map(|(key, _)| key.clone());
                    key.map(|key| string(context, &key)).transpose()
                }
                Some(HeapObject::ValueDictionary(values)) => Ok(values
                    .table
                    .snapshot()
                    .get(current)
                    .map(|(_, (key, _))| *key)),
                Some(HeapObject::MappingProxy(proxy)) => match context.heap.get(proxy.dictionary) {
                    Some(HeapObject::Dictionary(values)) => {
                        let key = values.entries.get(current).map(|(key, _)| key.clone());
                        key.map(|key| string(context, &key)).transpose()
                    }
                    Some(HeapObject::ValueDictionary(values)) => Ok(values
                        .table
                        .snapshot()
                        .get(current)
                        .map(|(_, (key, _))| *key)),
                    _ => Err("mappingproxy source is invalid".to_owned()),
                },
                Some(HeapObject::DictionaryView(view)) => {
                    let dictionary = view.dictionary;
                    let kind = view.kind;
                    let entries = match context.heap.get(dictionary) {
                        Some(HeapObject::Dictionary(values)) => {
                            let entries = values.entries.clone();
                            entries
                                .into_iter()
                                .map(|(key, value)| string(context, &key).map(|key| (key, value)))
                                .collect::<Result<Vec<_>, _>>()?
                        }
                        Some(HeapObject::ValueDictionary(values)) => values
                            .table
                            .snapshot()
                            .into_iter()
                            .map(|(_, entry)| entry)
                            .collect(),
                        _ => return Err("dictionary view source is invalid".to_owned()),
                    };
                    let Some((key, value)) = entries.get(current).copied() else {
                        return Ok(None);
                    };
                    match kind {
                        DictionaryViewKind::Keys => Ok(Some(key)),
                        DictionaryViewKind::Values => Ok(Some(value)),
                        DictionaryViewKind::Items => tuple(context, &[key, value]).map(Some),
                    }
                }
                Some(HeapObject::Set(values) | HeapObject::FrozenSet(values)) => Ok(values
                    .table
                    .snapshot()
                    .get(current)
                    .map(|(_, value)| *value)),
                _ => Err("iterator source is invalid".to_owned()),
            }
        }
        HeapObject::Iterator(IteratorObject::SequenceProtocol { source, index }) => {
            let source = *source;
            let current = *index;
            *index += 1;
            match item_get(context, source, RValue::small_int(current as i64)) {
                Ok(value) => Ok(Some(value)),
                Err(error) if context.consume_exception_type("IndexError") => Ok(None),
                Err(error) => Err(error),
            }
        }
        HeapObject::Iterator(IteratorObject::Enumerate { source, index }) => {
            let source = *source;
            let current = index.clone();
            *index += BigInt::from(1_u8);
            match iterator_next(context, source)? {
                Some(value) => {
                    let index = store_integer(context, current)?;
                    tuple(context, &[index, value]).map(Some)
                }
                None => Ok(None),
            }
        }
        HeapObject::Iterator(IteratorObject::Zip { sources, strict }) => {
            let sources = sources.to_vec();
            let strict = *strict;
            if sources.is_empty() {
                return Ok(None);
            }
            let mut values = Vec::with_capacity(sources.len());
            for (position, source) in sources.iter().copied().enumerate() {
                match iterator_next(context, source)? {
                    Some(value) => values.push(value),
                    None if !strict => return Ok(None),
                    None if position > 0 => {
                        let earlier = if position == 1 {
                            "argument 1".to_owned()
                        } else {
                            format!("arguments 1-{position}")
                        };
                        return Err(format!(
                            "zip() argument {} is shorter than {earlier}",
                            position + 1
                        ));
                    }
                    None => {
                        for (later, source) in sources.iter().copied().enumerate().skip(1) {
                            if iterator_next(context, source)?.is_some() {
                                let earlier = if later == 1 {
                                    "argument 1".to_owned()
                                } else {
                                    format!("arguments 1-{later}")
                                };
                                return Err(format!(
                                    "zip() argument {} is longer than {earlier}",
                                    later + 1
                                ));
                            }
                        }
                        return Ok(None);
                    }
                }
            }
            tuple(context, &values).map(Some)
        }
        HeapObject::Iterator(IteratorObject::CallSentinel { callable, sentinel }) => {
            let callable = *callable;
            let sentinel = *sentinel;
            let value = crate::call::invoke(context, callable, &[], &[])?;
            let equal = compare(context, 0, value, sentinel)?;
            if truthy(context, equal)? {
                Ok(None)
            } else {
                Ok(Some(value))
            }
        }
        HeapObject::Iterator(IteratorObject::Map { callable, sources }) => {
            let callable = *callable;
            let sources = sources.to_vec();
            let mut values = Vec::with_capacity(sources.len());
            for source in sources {
                let Some(value) = iterator_next(context, source)? else {
                    return Ok(None);
                };
                values.push(value);
            }
            context
                .with_temporary_roots(&values, |context| {
                    crate::call::invoke(context, callable, &values, &[])
                })
                .map(Some)
        }
        HeapObject::Iterator(IteratorObject::Filter { predicate, source }) => {
            let predicate = *predicate;
            let source = *source;
            while let Some(value) = iterator_next(context, source)? {
                let include = if predicate == RValue::NONE {
                    truthy(context, value)?
                } else {
                    let result = crate::call::invoke(context, predicate, &[value], &[])?;
                    truthy(context, result)?
                };
                if include {
                    return Ok(Some(value));
                }
            }
            Ok(None)
        }
        HeapObject::Iterator(IteratorObject::ReverseSequence {
            source,
            index,
            expected_version,
        }) => {
            let source = *source;
            let current = *index;
            *index -= 1;
            if let Some(expected) = *expected_version
                && collection_version(context, source) != Some(expected)
            {
                return context
                    .raise_error("RuntimeError", "dictionary changed size during iteration");
            }
            if current < 0 {
                return Ok(None);
            }
            match context.heap.get(source) {
                Some(HeapObject::List(values)) => Ok(values.get(current as usize).copied()),
                Some(HeapObject::Tuple(values)) => Ok(values.get(current as usize).copied()),
                Some(HeapObject::String(value)) => value
                    .chars()
                    .nth(current as usize)
                    .map(|character| string(context, &character.to_string()))
                    .transpose(),
                Some(HeapObject::Bytes(value)) => Ok(value
                    .get(current as usize)
                    .copied()
                    .map(|byte| RValue::small_int(i64::from(byte)))),
                Some(HeapObject::ByteArray(value)) => Ok(value
                    .bytes
                    .get(current as usize)
                    .copied()
                    .map(|byte| RValue::small_int(i64::from(byte)))),
                Some(HeapObject::Dictionary(values)) => {
                    let key = values
                        .entries
                        .get(current as usize)
                        .map(|(key, _)| key.clone());
                    key.map(|key| string(context, &key)).transpose()
                }
                Some(HeapObject::ValueDictionary(values)) => Ok(values
                    .table
                    .snapshot()
                    .get(current as usize)
                    .map(|(_, (key, _))| *key)),
                Some(HeapObject::MappingProxy(proxy)) => match context.heap.get(proxy.dictionary) {
                    Some(HeapObject::Dictionary(values)) => {
                        let key = values
                            .entries
                            .get(current as usize)
                            .map(|(key, _)| key.clone());
                        key.map(|key| string(context, &key)).transpose()
                    }
                    Some(HeapObject::ValueDictionary(values)) => Ok(values
                        .table
                        .snapshot()
                        .get(current as usize)
                        .map(|(_, (key, _))| *key)),
                    _ => Err("mappingproxy source is invalid".to_owned()),
                },
                Some(HeapObject::DictionaryView(view)) => {
                    let dictionary = view.dictionary;
                    let kind = view.kind;
                    let entries = match context.heap.get(dictionary) {
                        Some(HeapObject::Dictionary(values)) => {
                            let entries = values.entries.clone();
                            entries
                                .into_iter()
                                .map(|(key, value)| string(context, &key).map(|key| (key, value)))
                                .collect::<Result<Vec<_>, _>>()?
                        }
                        Some(HeapObject::ValueDictionary(values)) => values
                            .table
                            .snapshot()
                            .into_iter()
                            .map(|(_, entry)| entry)
                            .collect(),
                        _ => return Err("dictionary view source is invalid".to_owned()),
                    };
                    let Some((key, value)) = entries.get(current as usize).copied() else {
                        return Ok(None);
                    };
                    match kind {
                        DictionaryViewKind::Keys => Ok(Some(key)),
                        DictionaryViewKind::Values => Ok(Some(value)),
                        DictionaryViewKind::Items => tuple(context, &[key, value]).map(Some),
                    }
                }
                _ => Err("reverse iterator source is invalid".to_owned()),
            }
        }
        HeapObject::Iterator(IteratorObject::ReverseProtocol { source, index }) => {
            let source = *source;
            let current = *index;
            *index -= 1;
            if current < 0 {
                return Ok(None);
            }
            item_get(context, source, RValue::small_int(current as i64)).map(Some)
        }
        _ => match context.invoke_special_method(iterator, "__next__", &[]) {
            Ok(Some(value)) => Ok(Some(value)),
            Ok(None) => Err("object is not an iterator".to_owned()),
            Err(error) if context.consume_stop_iteration() => Ok(None),
            Err(error) => Err(error),
        },
    }
}

pub fn reversed(context: &mut RimeraContext, value: RValue) -> Result<RValue, String> {
    context.with_temporary_roots(&[value], |context| {
        if let Some(result) = context.invoke_special_method(value, "__reversed__", &[])? {
            return Ok(result);
        }
        if let Some(HeapObject::Range(range)) = context.heap.get(value) {
            let range = range.clone();
            let length = range_length_value(&range);
            if length.is_zero() {
                return context.allocate(HeapObject::Iterator(IteratorObject::Range {
                    current: BigInt::ZERO,
                    stop: BigInt::ZERO,
                    step: BigInt::from(1_u8),
                }));
            }
            let reverse_step = -range.step.clone();
            let current = &range.start + (&range.step * (&length - 1_u8));
            let stop = &range.start - &range.step;
            return context.allocate(HeapObject::Iterator(IteratorObject::Range {
                current,
                stop,
                step: reverse_step,
            }));
        }
        let (length, expected_version) = match context.heap.get(value) {
            Some(HeapObject::List(values)) => (values.len(), None),
            Some(HeapObject::Tuple(values)) => (values.len(), None),
            Some(HeapObject::String(value)) => (value.chars().count(), None),
            Some(HeapObject::Bytes(value)) => (value.len(), None),
            Some(HeapObject::ByteArray(value)) => (value.bytes.len(), None),
            Some(HeapObject::Dictionary(values)) => (values.entries.len(), None),
            Some(HeapObject::ValueDictionary(values)) => {
                (values.table.len(), Some(values.table.version))
            }
            Some(HeapObject::DictionaryView(view)) => {
                let dictionary = view.dictionary;
                let length = match context.heap.get(dictionary) {
                    Some(HeapObject::Dictionary(values)) => values.entries.len(),
                    Some(HeapObject::ValueDictionary(values)) => values.table.len(),
                    _ => return Err("dictionary view source is invalid".to_owned()),
                };
                (length, collection_version(context, value))
            }
            Some(HeapObject::MappingProxy(proxy)) => {
                let dictionary = proxy.dictionary;
                let length = match context.heap.get(dictionary) {
                    Some(HeapObject::Dictionary(values)) => values.entries.len(),
                    Some(HeapObject::ValueDictionary(values)) => values.table.len(),
                    _ => return Err("mappingproxy source is invalid".to_owned()),
                };
                (length, collection_version(context, value))
            }
            _ => {
                let length = length(context, value)?;
                let length = integer(context, length)?
                    .to_isize()
                    .ok_or_else(|| "sequence is too large".to_owned())?;
                if length < 0 {
                    return Err("__len__() should return >= 0".to_owned());
                }
                return context.allocate(HeapObject::Iterator(IteratorObject::ReverseProtocol {
                    source: value,
                    index: length - 1,
                }));
            }
        };
        let index = isize::try_from(length).map_err(|_| "sequence is too large".to_owned())? - 1;
        context.allocate(HeapObject::Iterator(IteratorObject::ReverseSequence {
            source: value,
            index,
            expected_version,
        }))
    })
}

pub fn length(context: &mut RimeraContext, value: RValue) -> Result<RValue, String> {
    context.with_temporary_roots(&[value], |context| {
        if let Some(storage) = instance_storage(context, value) {
            if let Some(result) = context.invoke_special_method(value, "__len__", &[])? {
                integer(context, result)?;
                return Ok(result);
            }
            return length(context, storage);
        }
        if let Some(HeapObject::Range(range)) = context.heap.get(value) {
            let length = range_length_value(range);
            if length > BigInt::from(isize::MAX) {
                return context.raise_error(
                    "OverflowError",
                    "Python int too large to convert to C ssize_t",
                );
            }
            return store_integer(context, length);
        }
        if let Some(HeapObject::DictionaryView(view)) = context.heap.get(value) {
            let dictionary = view.dictionary;
            return length(context, dictionary);
        }
        if let Some(HeapObject::MappingProxy(proxy)) = context.heap.get(value) {
            let dictionary = proxy.dictionary;
            return length(context, dictionary);
        }
        let length = match context.heap.get(value) {
            Some(HeapObject::String(value)) => value.chars().count(),
            Some(HeapObject::Bytes(value)) => value.len(),
            Some(HeapObject::ByteArray(value)) => value.bytes.len(),
            Some(HeapObject::MemoryView(value)) => {
                if value.released {
                    return Err("operation forbidden on released memoryview object".to_owned());
                }
                value
                    .shape
                    .first()
                    .copied()
                    .ok_or_else(|| "0-dim memory has no length".to_owned())?
            }
            Some(HeapObject::Tuple(values)) => values.len(),
            Some(HeapObject::List(values)) => values.len(),
            Some(HeapObject::Dictionary(values)) => values.entries.len(),
            Some(HeapObject::ValueDictionary(values)) => values.table.len(),
            Some(HeapObject::Set(values)) => values.table.len(),
            Some(HeapObject::FrozenSet(values)) => values.table.len(),
            Some(HeapObject::DictionaryView(view)) => length(context, view.dictionary)?
                .payload
                .try_into()
                .map_err(|_| "dictionary view length is too large".to_owned())?,
            Some(_) => {
                let result = context
                    .invoke_special_method(value, "__len__", &[])?
                    .ok_or_else(|| "object has no length".to_owned())?;
                integer(context, result)?;
                return Ok(result);
            }
            None => return Err("value contains a stale heap handle".to_owned()),
        };
        store_integer(context, BigInt::from(length))
    })
}

pub fn item_get(
    context: &mut RimeraContext,
    collection: RValue,
    index: RValue,
) -> Result<RValue, String> {
    if let Some(storage) = instance_storage(context, collection) {
        if let Some(result) = context.invoke_special_method(collection, "__getitem__", &[index])? {
            return Ok(result);
        }
        return item_get(context, storage, index);
    }
    if let Some(HeapObject::MappingProxy(proxy)) = context.heap.get(collection) {
        return item_get(context, proxy.dictionary, index);
    }
    if matches!(
        context.heap.get(collection),
        Some(HeapObject::ValueDictionary(_))
    ) {
        if let Some(value) = value_dictionary_lookup(context, collection, index)? {
            return Ok(value);
        }
        return context.raise_error("KeyError", "dictionary key not found");
    }
    if let Some(HeapObject::Dictionary(dictionary)) = context.heap.get(collection) {
        let key = string_value(context, index)
            .ok_or_else(|| "dictionary key must be string".to_owned())?;
        if let Some(value) = dictionary.get(key) {
            return Ok(value);
        }
        return context.raise_error("KeyError", "dictionary key not found");
    }
    if let Some(slice) = slice_value(context, index) {
        return slice_item(context, collection, &slice);
    }
    if let Some(HeapObject::Range(range)) = context.heap.get(collection) {
        let range = range.clone();
        let length = range_length_value(&range);
        let mut index = index_integer(context, index)?;
        if index.sign() == num_bigint::Sign::Minus {
            index += &length;
        }
        if index.sign() == num_bigint::Sign::Minus || index >= length {
            return context.raise_error("IndexError", "range object index out of range");
        }
        return store_integer(context, range.start + range.step * index);
    }
    if let Some(HeapObject::Tuple(values)) = context.heap.get(collection) {
        let values = values.to_vec();
        let Some(index) = index_integer(context, index)?.to_isize() else {
            return context.raise_error("IndexError", "tuple index out of range");
        };
        return sequence_item(&values, index, "tuple")
            .or_else(|message| context.raise_error("IndexError", message));
    }
    if let Some(HeapObject::List(values)) = context.heap.get(collection) {
        let values = values.clone();
        let Some(index) = index_integer(context, index)?.to_isize() else {
            return context.raise_error("IndexError", "list index out of range");
        };
        return sequence_item(&values, index, "list")
            .or_else(|message| context.raise_error("IndexError", message));
    }
    if let Some(HeapObject::String(value)) = context.heap.get(collection) {
        let values = value.chars().collect::<Vec<_>>();
        let Some(index) = index_integer(context, index)?.to_isize() else {
            return context.raise_error("IndexError", "string index out of range");
        };
        let length = isize::try_from(values.len()).map_err(|_| "string is too large".to_owned())?;
        let normalized = if index < 0 { length + index } else { index };
        let Some(position) = usize::try_from(normalized).ok() else {
            return context.raise_error("IndexError", "string index out of range");
        };
        let Some(character) = values.get(position) else {
            return context.raise_error("IndexError", "string index out of range");
        };
        return string(context, &character.to_string());
    }
    if let Some(HeapObject::Bytes(values)) = context.heap.get(collection) {
        let values = values.clone();
        let Some(index) = index_integer(context, index)?.to_isize() else {
            return context.raise_error("IndexError", "bytes index out of range");
        };
        return byte_item(&values, index, "bytes")
            .or_else(|message| context.raise_error("IndexError", message));
    }
    if let Some(HeapObject::ByteArray(values)) = context.heap.get(collection) {
        let values = values.bytes.clone();
        let Some(index) = index_integer(context, index)?.to_isize() else {
            return context.raise_error("IndexError", "bytearray index out of range");
        };
        return byte_item(&values, index, "bytearray")
            .or_else(|message| context.raise_error("IndexError", message));
    }
    if let Some(HeapObject::MemoryView(view)) = context.heap.get(collection) {
        let view = view.clone();
        if view.released {
            return Err("operation forbidden on released memoryview object".to_owned());
        }
        let tuple_indices = match context.heap.get(index) {
            Some(HeapObject::Tuple(values)) => Some(values.to_vec()),
            _ => None,
        };
        let raw_indices = if let Some(values) = tuple_indices {
            if values.len() > view.shape.len() {
                return Err(format!(
                    "cannot index {}-dimension view with {}-element tuple",
                    view.shape.len(),
                    values.len()
                ));
            }
            if values.len() < view.shape.len() {
                return Err("sub-views are not implemented".to_owned());
            }
            values
        } else {
            match view.shape.len() {
                0 => return Err("invalid indexing of 0-dim memory".to_owned()),
                1 => vec![index],
                _ => return Err("multi-dimensional sub-views are not implemented".to_owned()),
            }
        };
        let mut indices = Vec::with_capacity(raw_indices.len());
        for (axis, value) in raw_indices.into_iter().enumerate() {
            let raw = index_integer(context, value)?
                .to_isize()
                .ok_or_else(|| "index cannot fit in the native sequence range".to_owned())?;
            let length = isize::try_from(view.shape[axis])
                .map_err(|_| "memoryview is too large".to_owned())?;
            let normalized = if raw < 0 { length + raw } else { raw };
            indices.push(
                usize::try_from(normalized)
                    .ok()
                    .filter(|index| *index < view.shape[axis])
                    .ok_or_else(|| format!("index out of bounds on dimension {}", axis + 1))?,
            );
        }
        return memoryview_scalar_get(context, &view, &indices);
    }
    match context.heap.get(collection) {
        Some(_) => context
            .invoke_special_method(collection, "__getitem__", &[index])?
            .ok_or_else(|| "object is not subscriptable".to_owned()),
        None => Err("value contains a stale heap handle".to_owned()),
    }
}

fn slice_item(
    context: &mut RimeraContext,
    collection: RValue,
    slice: &crate::object::SliceObject,
) -> Result<RValue, String> {
    let source = match context.heap.get(collection) {
        Some(HeapObject::String(value)) => {
            let values = value.chars().collect::<Vec<_>>();
            let indexes = normalize_slice(context, slice, values.len())?;
            return string(
                context,
                &indexes
                    .into_iter()
                    .map(|index| values[index])
                    .collect::<String>(),
            );
        }
        Some(HeapObject::Bytes(value)) => {
            let value = value.clone();
            let indexes = normalize_slice(context, slice, value.len())?;
            return bytes(
                context,
                &indexes
                    .into_iter()
                    .map(|index| value[index])
                    .collect::<Vec<_>>(),
            );
        }
        Some(HeapObject::ByteArray(value)) => {
            let value = value.bytes.clone();
            let indexes = normalize_slice(context, slice, value.len())?;
            return bytearray(
                context,
                &indexes
                    .into_iter()
                    .map(|index| value[index])
                    .collect::<Vec<_>>(),
            );
        }
        Some(HeapObject::Range(range)) => {
            let range = range.clone();
            let length = range_length_value(&range);
            let (start, stop, step) = normalize_slice_bigint(context, slice, &length)?;
            let child = RangeObject {
                start: &range.start + (&range.step * start),
                stop: &range.start + (&range.step * stop),
                step: range.step * step,
            };
            return context.allocate(HeapObject::Range(child));
        }
        Some(HeapObject::MemoryView(view)) => {
            let view = view.clone();
            if view.released {
                return Err("operation forbidden on released memoryview object".to_owned());
            }
            if view.shape.is_empty() {
                return Err("invalid indexing of 0-dim memory".to_owned());
            }
            let indexes = normalize_slice(context, slice, view.shape[0])?;
            let step = slice_step(context, slice)?;
            let offset = match indexes.first().copied() {
                Some(first) => {
                    let mut coordinates = vec![0; view.shape.len()];
                    coordinates[0] = first;
                    memoryview_offset(&view, &coordinates)?
                }
                None => view.offset,
            };
            let mut shape = view.shape.to_vec();
            shape[0] = indexes.len();
            let mut strides = view.strides.to_vec();
            strides[0] = strides[0].saturating_mul(step);
            let child = MemoryViewObject {
                exporter: view.exporter,
                format: view.format.clone(),
                item_size: view.item_size,
                shape: shape.into_boxed_slice(),
                strides: strides.into_boxed_slice(),
                suboffsets: view.suboffsets.clone(),
                offset,
                readonly: view.readonly,
                released: false,
            };
            return context.with_temporary_roots(&[child.exporter], |context| {
                allocate_memoryview_object(context, child)
            });
        }
        Some(HeapObject::Tuple(value)) => (value.to_vec(), SequenceKind::Tuple),
        Some(HeapObject::List(value)) => (value.clone(), SequenceKind::List),
        Some(_) => return Err("object is not subscriptable".to_owned()),
        None => return Err("value contains a stale heap handle".to_owned()),
    };
    let indexes = normalize_slice(context, slice, source.0.len())?;
    let values = indexes
        .into_iter()
        .map(|index| source.0[index])
        .collect::<Vec<_>>();
    allocate_sequence(context, source.1, values)
}

fn slice_value(context: &RimeraContext, value: RValue) -> Option<crate::object::SliceObject> {
    match context.heap.get(value) {
        Some(HeapObject::Slice(slice)) => Some(slice.clone()),
        _ => None,
    }
}

fn normalize_slice_bounds(
    context: &mut RimeraContext,
    slice: &crate::object::SliceObject,
    len: usize,
) -> Result<(isize, isize, isize), String> {
    let length = isize::try_from(len).map_err(|_| "sequence is too large".to_owned())?;
    let mut bound = |value: Option<RValue>| -> Result<Option<isize>, String> {
        match value {
            None | Some(RValue::NONE) => Ok(None),
            Some(value) => index_integer(context, value).map(bigint_to_isize).map(Some),
        }
    };
    let step = bound(slice.step)?.unwrap_or(1);
    if step == 0 {
        return context.raise_error("ValueError", "slice step cannot be zero");
    }
    let (start, stop) = (bound(slice.start)?, bound(slice.stop)?);
    if step > 0 {
        let normalize = |value: isize| {
            let value = if value < 0 {
                value.saturating_add(length)
            } else {
                value
            };
            value.clamp(0, length)
        };
        Ok((
            start.map(normalize).unwrap_or(0),
            stop.map(normalize).unwrap_or(length),
            step,
        ))
    } else {
        let upper = length - 1;
        let normalize = |value: isize| {
            let value = if value < 0 {
                value.saturating_add(length)
            } else {
                value
            };
            value.clamp(-1, upper)
        };
        Ok((
            start.map(normalize).unwrap_or(upper),
            stop.map(normalize).unwrap_or(-1),
            step,
        ))
    }
}

fn normalize_slice_selection(
    context: &mut RimeraContext,
    slice: &crate::object::SliceObject,
    len: usize,
) -> Result<(isize, isize, isize, Vec<usize>), String> {
    let (start, stop, step) = normalize_slice_bounds(context, slice, len)?;
    let mut current = start;
    let mut indexes = Vec::new();
    if step > 0 {
        while current < stop {
            indexes.push(usize::try_from(current).expect("normalized index is non-negative"));
            current = current.saturating_add(step);
        }
    } else {
        while current > stop {
            indexes.push(usize::try_from(current).expect("normalized index is non-negative"));
            current = current.saturating_add(step);
        }
    }
    Ok((start, stop, step, indexes))
}

fn normalize_slice(
    context: &mut RimeraContext,
    slice: &crate::object::SliceObject,
    len: usize,
) -> Result<Vec<usize>, String> {
    normalize_slice_selection(context, slice, len).map(|(_, _, _, indexes)| indexes)
}
fn normalize_slice_bigint(
    context: &mut RimeraContext,
    slice: &crate::object::SliceObject,
    length: &BigInt,
) -> Result<(BigInt, BigInt, BigInt), String> {
    let mut bound = |value: Option<RValue>| -> Result<Option<BigInt>, String> {
        match value {
            None | Some(RValue::NONE) => Ok(None),
            Some(value) => index_integer(context, value).map(Some),
        }
    };
    let step = bound(slice.step)?.unwrap_or_else(|| BigInt::from(1_u8));
    if step.is_zero() {
        return context.raise_error("ValueError", "slice step cannot be zero");
    }
    let start = bound(slice.start)?;
    let stop = bound(slice.stop)?;
    if step.sign() != num_bigint::Sign::Minus {
        let normalize = |mut value: BigInt| {
            if value.sign() == num_bigint::Sign::Minus {
                value += length;
            }
            value.max(BigInt::from(0_u8)).min(length.clone())
        };
        Ok((
            start.map(normalize).unwrap_or_else(|| BigInt::from(0_u8)),
            stop.map(normalize).unwrap_or_else(|| length.clone()),
            step,
        ))
    } else {
        let upper = length - 1_u8;
        let normalize = |mut value: BigInt| {
            if value.sign() == num_bigint::Sign::Minus {
                value += length;
            }
            value.max(BigInt::from(-1_i8)).min(upper.clone())
        };
        Ok((
            start.map(normalize).unwrap_or_else(|| upper.clone()),
            stop.map(normalize).unwrap_or_else(|| BigInt::from(-1_i8)),
            step,
        ))
    }
}

pub fn slice_indices(
    context: &mut RimeraContext,
    slice_value: RValue,
    length_value: RValue,
) -> Result<RValue, String> {
    context.with_temporary_roots(&[slice_value, length_value], |context| {
        let Some(HeapObject::Slice(slice)) = context.heap.get(slice_value) else {
            return Err("slice.indices() requires a slice receiver".to_owned());
        };
        let slice = slice.clone();
        let length = index_integer(context, length_value)?;
        if length.sign() == num_bigint::Sign::Minus {
            return context.raise_error("ValueError", "length should not be negative");
        }
        let (start, stop, step) = normalize_slice_bigint(context, &slice, &length)?;
        let start = store_integer(context, start)?;
        context.with_temporary_roots(&[start], |context| {
            let stop = store_integer(context, stop)?;
            context.with_temporary_roots(&[start, stop], |context| {
                let step = store_integer(context, step)?;
                context.with_temporary_roots(&[start, stop, step], |context| {
                    tuple(context, &[start, stop, step])
                })
            })
        })
    })
}

fn bigint_to_isize(value: BigInt) -> isize {
    value.to_isize().unwrap_or_else(|| {
        if value.sign() == num_bigint::Sign::Minus {
            isize::MIN
        } else {
            isize::MAX
        }
    })
}

pub fn item_set(
    context: &mut RimeraContext,
    collection: RValue,
    index: RValue,
    value: RValue,
) -> Result<(), String> {
    context.with_temporary_roots(&[collection, index, value], |context| {
        if let Some(storage) = instance_storage(context, collection) {
            if context
                .invoke_special_method(collection, "__setitem__", &[index, value])?
                .is_some()
            {
                return Ok(());
            }
            return item_set(context, storage, index, value);
        }
        if matches!(context.heap.get(collection), Some(HeapObject::Instance(_)))
            && context
                .invoke_special_method(collection, "__setitem__", &[index, value])?
                .is_some()
        {
            return Ok(());
        }
        if matches!(
            context.heap.get(collection),
            Some(HeapObject::ValueDictionary(_))
        ) {
            let hash = hash_i64(context, index)?;
            let replacement = value_dictionary_find_entry(context, collection, index, hash)?;
            let Some(HeapObject::ValueDictionary(dictionary)) = context.heap.get_mut(collection)
            else {
                unreachable!()
            };
            if let Some(position) = replacement
                && dictionary
                    .table
                    .update(position, |(_, existing)| *existing = value)
            {
                return Ok(());
            }
            dictionary.table.insert_new(hash, (index, value));
            return Ok(());
        }
        let dictionary_key = string_value(context, index).map(ToOwned::to_owned);
        if let Some(HeapObject::Dictionary(dictionary)) = context.heap.get_mut(collection) {
            let key = dictionary_key.ok_or_else(|| "dictionary key must be string".to_owned())?;
            dictionary.insert(key, value);
            return Ok(());
        }
        if let Some(slice) = slice_value(context, index) {
            return slice_assign(context, collection, &slice, value);
        }
        if let Some(HeapObject::MemoryView(view)) = context.heap.get(collection) {
            let view = view.clone();
            if view.released {
                return Err("operation forbidden on released memoryview object".to_owned());
            }
            if view.readonly {
                return Err("cannot modify read-only memory".to_owned());
            }
            let tuple_indices = match context.heap.get(index) {
                Some(HeapObject::Tuple(values)) => Some(values.to_vec()),
                _ => None,
            };
            let raw_indices = if let Some(values) = tuple_indices {
                if values.len() > view.shape.len() {
                    return Err(format!(
                        "cannot index {}-dimension view with {}-element tuple",
                        view.shape.len(),
                        values.len()
                    ));
                }
                if values.len() < view.shape.len() {
                    return Err("sub-views are not implemented".to_owned());
                }
                values
            } else {
                match view.shape.len() {
                    0 => return Err("invalid indexing of 0-dim memory".to_owned()),
                    1 => vec![index],
                    _ => return Err("sub-views are not implemented".to_owned()),
                }
            };
            let mut indices = Vec::with_capacity(raw_indices.len());
            for (axis, raw) in raw_indices.into_iter().enumerate() {
                let raw = index_integer(context, raw)?
                    .to_isize()
                    .ok_or_else(|| "index cannot fit in the native sequence range".to_owned())?;
                let length = isize::try_from(view.shape[axis])
                    .map_err(|_| "memoryview is too large".to_owned())?;
                let normalized = if raw < 0 { length + raw } else { raw };
                indices.push(
                    usize::try_from(normalized)
                        .ok()
                        .filter(|index| *index < view.shape[axis])
                        .ok_or_else(|| format!("index out of bounds on dimension {}", axis + 1))?,
                );
            }
            memoryview_scalar_set(context, &view, &indices, value)?;
            return Ok(());
        }
        let Some(index) = index_integer(context, index)?.to_isize() else {
            return context.raise_error(
                "IndexError",
                "index cannot fit in the native sequence range",
            );
        };
        let byte = if matches!(context.heap.get(collection), Some(HeapObject::ByteArray(_))) {
            let integer = index_integer(context, value)?;
            let Some(byte) = integer.to_u8() else {
                return context.raise_error("ValueError", "byte must be in range(0, 256)");
            };
            Some(byte)
        } else {
            None
        };
        if let Some(HeapObject::ByteArray(values)) = context.heap.get_mut(collection) {
            let length = isize::try_from(values.bytes.len())
                .map_err(|_| "sequence is too large".to_owned())?;
            let index = if index < 0 { length + index } else { index };
            let Some(slot) = usize::try_from(index)
                .ok()
                .and_then(|index| values.bytes.get_mut(index))
            else {
                return context.raise_error("IndexError", "bytearray index out of range");
            };
            *slot = byte.expect("bytearray assignment validates the replacement first");
            return Ok(());
        }
        let Some(HeapObject::List(values)) = context.heap.get_mut(collection) else {
            return Err("object does not support item assignment".to_owned());
        };
        let length =
            isize::try_from(values.len()).map_err(|_| "sequence is too large".to_owned())?;
        let index = if index < 0 { length + index } else { index };
        let Some(slot) = usize::try_from(index)
            .ok()
            .and_then(|index| values.get_mut(index))
        else {
            return context.raise_error("IndexError", "list assignment index out of range");
        };
        *slot = value;
        Ok(())
    })
}

/// Deletes a native item or dispatches `__delitem__` on a user object.
pub fn item_delete(
    context: &mut RimeraContext,
    collection: RValue,
    index: RValue,
) -> Result<(), String> {
    context.with_temporary_roots(&[collection, index], |context| {
        if let Some(storage) = instance_storage(context, collection) {
            if context
                .invoke_special_method(collection, "__delitem__", &[index])?
                .is_some()
            {
                return Ok(());
            }
            return item_delete(context, storage, index);
        }
        if matches!(context.heap.get(collection), Some(HeapObject::Instance(_)))
            && context
                .invoke_special_method(collection, "__delitem__", &[index])?
                .is_some()
        {
            return Ok(());
        }
        if matches!(
            context.heap.get(collection),
            Some(HeapObject::ValueDictionary(_))
        ) {
            let hash = hash_i64(context, index)?;
            let Some(position) = value_dictionary_find_entry(context, collection, index, hash)?
            else {
                return context.raise_error("KeyError", "dictionary key not found");
            };
            let Some(HeapObject::ValueDictionary(dictionary)) = context.heap.get_mut(collection)
            else {
                unreachable!()
            };
            if dictionary.table.remove(position).is_some() {
                return Ok(());
            }
            return context.raise_error("KeyError", "dictionary key not found");
        }
        let dictionary_key = string_value(context, index).map(ToOwned::to_owned);
        if let Some(HeapObject::Dictionary(dictionary)) = context.heap.get_mut(collection) {
            let key = dictionary_key.ok_or_else(|| "dictionary key must be string".to_owned())?;
            let removed = dictionary.remove(&key).is_some();
            if removed {
                return Ok(());
            }
            return context.raise_error("KeyError", "dictionary key not found");
        }
        if let Some(slice) = slice_value(context, index) {
            return slice_delete(context, collection, &slice);
        }
        let Some(integer_index) = index_integer(context, index)?.to_isize() else {
            return context.raise_error(
                "IndexError",
                "index cannot fit in the native sequence range",
            );
        };
        let Some(HeapObject::List(values)) = context.heap.get_mut(collection) else {
            if let Some(HeapObject::ByteArray(values)) = context.heap.get_mut(collection) {
                let length = isize::try_from(values.bytes.len())
                    .map_err(|_| "sequence is too large".to_owned())?;
                let position = if integer_index < 0 {
                    length + integer_index
                } else {
                    integer_index
                };
                let Some(position) = usize::try_from(position)
                    .ok()
                    .filter(|position| *position < values.bytes.len())
                else {
                    return context.raise_error("IndexError", "bytearray index out of range");
                };
                values.bytes.remove(position);
                return Ok(());
            }
            return context
                .invoke_special_method(collection, "__delitem__", &[index])?
                .ok_or_else(|| "object does not support item deletion".to_owned())
                .map(|_| ());
        };
        let length =
            isize::try_from(values.len()).map_err(|_| "sequence is too large".to_owned())?;
        let position = if integer_index < 0 {
            length + integer_index
        } else {
            integer_index
        };
        let Some(position) = usize::try_from(position)
            .ok()
            .filter(|position| *position < values.len())
        else {
            return context.raise_error("IndexError", "list assignment index out of range");
        };
        values.remove(position);
        Ok(())
    })
}

fn slice_assign(
    context: &mut RimeraContext,
    collection: RValue,
    slice: &crate::object::SliceObject,
    replacement: RValue,
) -> Result<(), String> {
    if let Some(HeapObject::MemoryView(view)) = context.heap.get(collection) {
        let view = view.clone();
        if view.released {
            return Err("operation forbidden on released memoryview object".to_owned());
        }
        if view.readonly {
            return Err("cannot modify read-only memory".to_owned());
        }
        if view.shape.len() != 1 {
            return Err(
                "memoryview slice assignments are currently restricted to ndim = 1".to_owned(),
            );
        }
        let indexes = normalize_slice(context, slice, view.shape[0])?;
        let replacement_bytes = match context.heap.get(replacement) {
            Some(HeapObject::Bytes(bytes)) if view.format == "B" => bytes.clone(),
            Some(HeapObject::ByteArray(bytes)) if view.format == "B" => bytes.bytes.clone(),
            Some(HeapObject::MemoryView(source)) if !source.released => {
                if source.shape.len() != 1
                    || source.format != view.format
                    || source.item_size != view.item_size
                {
                    return Err(
                        "memoryview assignment: lvalue and rvalue have different structures"
                            .to_owned(),
                    );
                }
                memoryview_raw_bytes(context, source)?
            }
            Some(HeapObject::MemoryView(_)) => {
                return Err("operation forbidden on released memoryview object".to_owned());
            }
            _ => {
                return Err(
                    "memoryview assignment: lvalue and rvalue have different structures".to_owned(),
                );
            }
        };
        let expected = indexes
            .len()
            .checked_mul(view.item_size)
            .ok_or_else(|| "memoryview dimensions overflow".to_owned())?;
        if replacement_bytes.len() != expected {
            return Err(
                "memoryview assignment: lvalue and rvalue have different structures".to_owned(),
            );
        }
        let offsets = indexes
            .iter()
            .map(|index| memoryview_offset(&view, &[*index]))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(HeapObject::ByteArray(exporter)) = context.heap.get_mut(view.exporter) else {
            return Err("memoryview exporter is not writable".to_owned());
        };
        for (position, offset) in offsets.into_iter().enumerate() {
            let end = offset
                .checked_add(view.item_size)
                .ok_or_else(|| "memoryview bounds overflow".to_owned())?;
            let source_start = position * view.item_size;
            let source_end = source_start + view.item_size;
            let target = exporter
                .bytes
                .get_mut(offset..end)
                .ok_or_else(|| "memoryview exporter is shorter than its view".to_owned())?;
            target.copy_from_slice(&replacement_bytes[source_start..source_end]);
        }
        return Ok(());
    }
    if matches!(context.heap.get(collection), Some(HeapObject::List(_))) {
        let replacement = collect_iterable(context, replacement)?;
        let length = match context.heap.get(collection) {
            Some(HeapObject::List(values)) => values.len(),
            _ => unreachable!(),
        };
        let (start, stop, step, indexes) = normalize_slice_selection(context, slice, length)?;
        let Some(HeapObject::List(values)) = context.heap.get_mut(collection) else {
            unreachable!()
        };
        if step == 1 {
            let start = usize::try_from(start).expect("positive-step slice start is non-negative");
            let end = usize::try_from(stop.max(start as isize))
                .expect("positive-step slice stop is non-negative");
            values.splice(start..end, replacement);
        } else {
            if indexes.len() != replacement.len() {
                return Err(format!(
                    "attempt to assign sequence of size {} to extended slice of size {}",
                    replacement.len(),
                    indexes.len()
                ));
            }
            for (index, value) in indexes.into_iter().zip(replacement) {
                values[index] = value;
            }
        }
        return Ok(());
    }
    if matches!(context.heap.get(collection), Some(HeapObject::ByteArray(_))) {
        let replacement = byte_values(context, replacement)?;
        let (length, exports) = match context.heap.get(collection) {
            Some(HeapObject::ByteArray(values)) => (values.bytes.len(), values.exports),
            _ => unreachable!(),
        };
        let (start, stop, step, indexes) = normalize_slice_selection(context, slice, length)?;
        let selected_len = if step == 1 {
            usize::try_from(stop.max(start) - start)
                .expect("positive-step slice length is non-negative")
        } else {
            indexes.len()
        };
        if exports != 0 && selected_len != replacement.len() {
            return context.raise_error(
                "BufferError",
                "Existing exports of data: object cannot be re-sized",
            );
        }
        let Some(HeapObject::ByteArray(values)) = context.heap.get_mut(collection) else {
            unreachable!()
        };
        if step == 1 {
            let start = usize::try_from(start).expect("positive-step slice start is non-negative");
            let end = usize::try_from(stop.max(start as isize))
                .expect("positive-step slice stop is non-negative");
            values.bytes.splice(start..end, replacement);
        } else {
            if indexes.len() != replacement.len() {
                return Err(format!(
                    "attempt to assign bytes of size {} to extended slice of size {}",
                    replacement.len(),
                    indexes.len()
                ));
            }
            for (index, value) in indexes.into_iter().zip(replacement) {
                values.bytes[index] = value;
            }
        }
        return Ok(());
    }
    Err("object does not support item assignment".to_owned())
}

fn slice_delete(
    context: &mut RimeraContext,
    collection: RValue,
    slice: &crate::object::SliceObject,
) -> Result<(), String> {
    let (length, exports) = match context.heap.get(collection) {
        Some(HeapObject::List(values)) => (values.len(), None),
        Some(HeapObject::ByteArray(values)) => (values.bytes.len(), Some(values.exports)),
        Some(_) => return Err("object does not support item deletion".to_owned()),
        None => return Err("value contains a stale heap handle".to_owned()),
    };
    let mut indexes = normalize_slice(context, slice, length)?;
    if exports.is_some_and(|exports| exports != 0) && !indexes.is_empty() {
        return context.raise_error(
            "BufferError",
            "Existing exports of data: object cannot be re-sized",
        );
    }
    indexes.sort_unstable_by(|left, right| right.cmp(left));
    if let Some(HeapObject::List(values)) = context.heap.get_mut(collection) {
        for index in indexes.iter().copied() {
            values.remove(index);
        }
        return Ok(());
    }
    if let Some(HeapObject::ByteArray(values)) = context.heap.get_mut(collection) {
        for index in indexes {
            values.bytes.remove(index);
        }
        return Ok(());
    }
    Err("object does not support item deletion".to_owned())
}

fn slice_step(
    context: &mut RimeraContext,
    slice: &crate::object::SliceObject,
) -> Result<isize, String> {
    let step = slice
        .step
        .map(|value| index_integer(context, value).map(bigint_to_isize))
        .transpose()?
        .unwrap_or(1);
    if step == 0 {
        return context.raise_error("ValueError", "slice step cannot be zero");
    }
    Ok(step)
}
/// Executes `__iop__` before normal/reflected dispatch.
pub fn inplace(
    context: &mut RimeraContext,
    op: u8,
    left: RValue,
    right: RValue,
) -> Result<RValue, String> {
    context.with_temporary_roots(&[left, right], |context| {
        if matches!(context.heap.get(left), Some(HeapObject::List(_))) {
            match op {
                0 => {
                    let Some((values, SequenceKind::List)) = sequence_values(context, right) else {
                        return binary_rooted(context, op, left, right);
                    };
                    let Some(HeapObject::List(left_values)) = context.heap.get_mut(left) else {
                        unreachable!()
                    };
                    left_values.extend(values);
                    return Ok(left);
                }
                2 => {
                    let count = integer(context, right)?
                        .to_usize()
                        .ok_or_else(|| "can't multiply sequence by non-int".to_owned())?;
                    let Some(HeapObject::List(left_values)) = context.heap.get_mut(left) else {
                        unreachable!()
                    };
                    let original = left_values.clone();
                    left_values.clear();
                    for _ in 0..count {
                        left_values.extend(original.iter().copied());
                    }
                    return Ok(left);
                }
                _ => {}
            }
        }
        if op == 11
            && let Some(dictionary) = native_dictionary_storage(context, left)
        {
            crate::call::dict_merge_source(context, dictionary, right)?;
            return Ok(left);
        }
        if matches!(context.heap.get(left), Some(HeapObject::Set(_)))
            && matches!(op, 1 | 9 | 10 | 11)
        {
            let result = binary_rooted(context, op, left, right)?;
            let replacement = match context.heap.get(result) {
                Some(HeapObject::Set(values)) => values.clone(),
                _ => return Err("set in-place operation produced a non-set".to_owned()),
            };
            let Some(HeapObject::Set(values)) = context.heap.get_mut(left) else {
                unreachable!()
            };
            *values = replacement;
            return Ok(left);
        }
        if let Some(result) = context.generic_inplace(op, left, right)? {
            return Ok(result);
        }
        binary_rooted(context, op, left, right)
    })
}

fn sequence_item(values: &[RValue], index: isize, type_name: &str) -> Result<RValue, String> {
    let length = isize::try_from(values.len()).map_err(|_| "sequence is too large".to_owned())?;
    let index = if index < 0 { length + index } else { index };
    usize::try_from(index)
        .ok()
        .and_then(|index| values.get(index).copied())
        .ok_or_else(|| format!("{type_name} index out of range"))
}

fn byte_item(values: &[u8], index: isize, type_name: &str) -> Result<RValue, String> {
    let length = isize::try_from(values.len()).map_err(|_| "sequence is too large".to_owned())?;
    let index = if index < 0 { length + index } else { index };
    values
        .get(usize::try_from(index).map_err(|_| format!("{type_name} index out of range"))?)
        .copied()
        .map(|byte| RValue::small_int(i64::from(byte)))
        .ok_or_else(|| format!("{type_name} index out of range"))
}

fn collection_version(context: &RimeraContext, value: RValue) -> Option<u64> {
    match context.heap.get(value) {
        Some(HeapObject::ValueDictionary(dictionary)) => Some(dictionary.table.version),
        Some(HeapObject::Set(set)) => Some(set.table.version),
        Some(HeapObject::DictionaryView(view)) => collection_version(context, view.dictionary),
        Some(HeapObject::MappingProxy(proxy)) => collection_version(context, proxy.dictionary),
        _ => None,
    }
}

fn buffer_format_code(format: &str) -> Result<char, String> {
    let raw = format.strip_prefix('@').unwrap_or(format);
    let mut characters = raw.chars();
    let Some(code) = characters.next() else {
        return Err("memoryview: unsupported cast format".to_owned());
    };
    if characters.next().is_some()
        || !matches!(
            code,
            'b' | 'B'
                | 'c'
                | '?'
                | 'h'
                | 'H'
                | 'i'
                | 'I'
                | 'l'
                | 'L'
                | 'q'
                | 'Q'
                | 'n'
                | 'N'
                | 'e'
                | 'f'
                | 'd'
                | 'P'
        )
    {
        return Err("memoryview: unsupported cast format".to_owned());
    }
    Ok(code)
}

fn buffer_format_size(format: &str) -> Result<usize, String> {
    match buffer_format_code(format)? {
        'b' | 'B' | 'c' | '?' => Ok(1),
        'h' | 'H' | 'e' => Ok(2),
        'i' | 'I' | 'f' => Ok(4),
        'l' => Ok(std::mem::size_of::<std::os::raw::c_long>()),
        'L' => Ok(std::mem::size_of::<std::os::raw::c_ulong>()),
        'q' | 'Q' | 'd' => Ok(8),
        'n' => Ok(std::mem::size_of::<isize>()),
        'N' | 'P' => Ok(std::mem::size_of::<usize>()),
        _ => unreachable!("validated native buffer format"),
    }
}

fn memoryview_is_contiguous(view: &MemoryViewObject) -> bool {
    let mut expected = view.item_size as isize;
    for dimension in (0..view.shape.len()).rev() {
        if view.strides.get(dimension).copied() != Some(expected) {
            return false;
        }
        expected = expected.saturating_mul(view.shape[dimension] as isize);
    }
    true
}

fn memoryview_offset(view: &MemoryViewObject, indices: &[usize]) -> Result<usize, String> {
    if indices.len() != view.shape.len() {
        return Err("invalid number of indices for memoryview".to_owned());
    }
    let mut offset =
        i128::try_from(view.offset).map_err(|_| "memoryview bounds overflow".to_owned())?;
    for (axis, index) in indices.iter().enumerate() {
        if *index >= view.shape[axis] {
            return Err(format!("index out of bounds on dimension {}", axis + 1));
        }
        let contribution = i128::try_from(*index)
            .map_err(|_| "memoryview bounds overflow".to_owned())?
            .checked_mul(view.strides[axis] as i128)
            .ok_or_else(|| "memoryview bounds overflow".to_owned())?;
        offset = offset
            .checked_add(contribution)
            .ok_or_else(|| "memoryview bounds overflow".to_owned())?;
    }
    usize::try_from(offset).map_err(|_| "memoryview bounds overflow".to_owned())
}

fn memoryview_raw_bytes(
    context: &RimeraContext,
    view: &MemoryViewObject,
) -> Result<Vec<u8>, String> {
    if view.released {
        return Err("operation forbidden on released memoryview object".to_owned());
    }
    let bytes = match context.heap.get(view.exporter) {
        Some(HeapObject::Bytes(bytes)) => bytes.clone(),
        Some(HeapObject::ByteArray(bytes)) => bytes.bytes.clone(),
        Some(HeapObject::MemoryView(parent)) => memoryview_bytes(context, parent)?,
        Some(_) => return Err("memoryview exporter is invalid".to_owned()),
        None => return Err("value contains a stale heap handle".to_owned()),
    };
    let total = view
        .shape
        .iter()
        .try_fold(1_usize, |total, dim| total.checked_mul(*dim))
        .ok_or_else(|| "memoryview dimensions overflow".to_owned())?;
    let mut output = Vec::with_capacity(total.saturating_mul(view.item_size));
    for linear in 0..total {
        let mut remainder = linear;
        let mut indices = vec![0; view.shape.len()];
        for axis in (0..view.shape.len()).rev() {
            indices[axis] = remainder % view.shape[axis];
            remainder /= view.shape[axis];
        }
        let offset = memoryview_offset(view, &indices)?;
        let end = offset
            .checked_add(view.item_size)
            .ok_or_else(|| "memoryview bounds overflow".to_owned())?;
        output.extend_from_slice(
            bytes
                .get(offset..end)
                .ok_or_else(|| "memoryview exporter is shorter than its view".to_owned())?,
        );
    }
    Ok(output)
}

fn memoryview_raw_bytes_fortran(
    context: &RimeraContext,
    view: &MemoryViewObject,
) -> Result<Vec<u8>, String> {
    if view.released {
        return Err("operation forbidden on released memoryview object".to_owned());
    }
    let source = match context.heap.get(view.exporter) {
        Some(HeapObject::Bytes(bytes)) => bytes.clone(),
        Some(HeapObject::ByteArray(bytes)) => bytes.bytes.clone(),
        Some(HeapObject::MemoryView(parent)) => memoryview_bytes(context, parent)?,
        Some(_) => return Err("memoryview exporter is invalid".to_owned()),
        None => return Err("value contains a stale heap handle".to_owned()),
    };
    let total = view
        .shape
        .iter()
        .try_fold(1_usize, |total, dim| total.checked_mul(*dim))
        .ok_or_else(|| "memoryview dimensions overflow".to_owned())?;
    let mut output = Vec::with_capacity(total.saturating_mul(view.item_size));
    for linear in 0..total {
        let mut remainder = linear;
        let mut indices = vec![0; view.shape.len()];
        for (axis, index) in indices.iter_mut().enumerate() {
            *index = remainder % view.shape[axis];
            remainder /= view.shape[axis];
        }
        let offset = memoryview_offset(view, &indices)?;
        let end = offset
            .checked_add(view.item_size)
            .ok_or_else(|| "memoryview bounds overflow".to_owned())?;
        output.extend_from_slice(
            source
                .get(offset..end)
                .ok_or_else(|| "memoryview exporter is shorter than its view".to_owned())?,
        );
    }
    Ok(output)
}

fn memoryview_bytes(context: &RimeraContext, view: &MemoryViewObject) -> Result<Vec<u8>, String> {
    memoryview_raw_bytes(context, view)
}

fn memoryview_ultimate_exporter(
    context: &RimeraContext,
    mut exporter: RValue,
) -> Result<RValue, String> {
    let mut seen = Vec::new();
    loop {
        if seen.contains(&exporter) {
            return Err("memoryview exporter cycle".to_owned());
        }
        seen.push(exporter);
        match context.heap.get(exporter) {
            Some(HeapObject::MemoryView(view)) if !view.released => exporter = view.exporter,
            Some(HeapObject::MemoryView(_)) => {
                return Err("operation forbidden on released memoryview object".to_owned());
            }
            Some(_) => return Ok(exporter),
            None => return Err("value contains a stale heap handle".to_owned()),
        }
    }
}

fn memoryview_scalar_get(
    context: &mut RimeraContext,
    view: &MemoryViewObject,
    indices: &[usize],
) -> Result<RValue, String> {
    let raw = memoryview_raw_bytes(context, view)?;
    let linear = if indices.len() == 1 && view.shape.len() == 1 {
        indices[0]
    } else {
        let mut value = 0usize;
        for (axis, index) in indices.iter().enumerate() {
            if *index >= view.shape[axis] {
                return Err(format!("index out of bounds on dimension {}", axis + 1));
            }
            value = value
                .saturating_mul(view.shape[axis])
                .saturating_add(*index);
        }
        value
    };
    let start = linear
        .checked_mul(view.item_size)
        .ok_or_else(|| "memoryview bounds overflow".to_owned())?;
    let scalar = raw
        .get(start..start + view.item_size)
        .ok_or_else(|| "memoryview exporter is shorter than its view".to_owned())?;
    match buffer_format_code(&view.format)? {
        'B' => Ok(RValue::small_int(i64::from(scalar[0]))),
        'b' => Ok(RValue::small_int(i64::from(scalar[0] as i8))),
        '?' => Ok(RValue::boolean(scalar[0] != 0)),
        'c' => bytes(context, scalar),
        'h' => store_integer(
            context,
            BigInt::from(i16::from_ne_bytes([scalar[0], scalar[1]])),
        ),
        'H' => store_integer(
            context,
            BigInt::from(u16::from_ne_bytes([scalar[0], scalar[1]])),
        ),
        'i' => store_integer(
            context,
            BigInt::from(i32::from_ne_bytes(
                scalar.try_into().map_err(|_| "invalid scalar size")?,
            )),
        ),
        'I' => store_integer(
            context,
            BigInt::from(u32::from_ne_bytes(
                scalar.try_into().map_err(|_| "invalid scalar size")?,
            )),
        ),
        'l' => {
            let value = if scalar.len() == 8 {
                BigInt::from(i64::from_ne_bytes(
                    scalar.try_into().map_err(|_| "invalid scalar size")?,
                ))
            } else {
                BigInt::from(i32::from_ne_bytes(
                    scalar.try_into().map_err(|_| "invalid scalar size")?,
                ))
            };
            store_integer(context, value)
        }
        'L' => {
            let value = if scalar.len() == 8 {
                BigInt::from(u64::from_ne_bytes(
                    scalar.try_into().map_err(|_| "invalid scalar size")?,
                ))
            } else {
                BigInt::from(u32::from_ne_bytes(
                    scalar.try_into().map_err(|_| "invalid scalar size")?,
                ))
            };
            store_integer(context, value)
        }
        'q' => store_integer(
            context,
            BigInt::from(i64::from_ne_bytes(
                scalar.try_into().map_err(|_| "invalid scalar size")?,
            )),
        ),
        'Q' => store_integer(
            context,
            BigInt::from(u64::from_ne_bytes(
                scalar.try_into().map_err(|_| "invalid scalar size")?,
            )),
        ),
        'n' => store_integer(
            context,
            BigInt::from(isize::from_ne_bytes(
                scalar.try_into().map_err(|_| "invalid scalar size")?,
            )),
        ),
        'N' | 'P' => store_integer(
            context,
            BigInt::from(usize::from_ne_bytes(
                scalar.try_into().map_err(|_| "invalid scalar size")?,
            )),
        ),
        'f' => float(
            context,
            f32::from_ne_bytes(scalar.try_into().map_err(|_| "invalid scalar size")?) as f64,
        ),
        'd' => float(
            context,
            f64::from_ne_bytes(scalar.try_into().map_err(|_| "invalid scalar size")?),
        ),
        'e' => float(
            context,
            f16_to_f32(u16::from_ne_bytes([scalar[0], scalar[1]])) as f64,
        ),
        _ => Err("memoryview: unsupported scalar format".to_owned()),
    }
}

fn memoryview_numeric_float(context: &mut RimeraContext, value: RValue) -> Result<f64, String> {
    if let Some(number) = numeric_float(context, value) {
        return Ok(number);
    }
    if let Some(result) = context.invoke_special_method(value, "__float__", &[])? {
        return numeric_float(context, result)
            .ok_or_else(|| "__float__ returned non-float".to_owned());
    }
    index_integer(context, value)?
        .to_f64()
        .ok_or_else(|| "int too large to convert to float".to_owned())
}

fn f32_to_f16(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7f_ffff;
    if exponent == 0xff {
        return sign | 0x7c00 | if mantissa == 0 { 0 } else { 0x0200 };
    }
    let half_exponent = exponent - 127 + 15;
    if half_exponent >= 0x1f {
        return sign | 0x7c00;
    }
    if half_exponent <= 0 {
        if half_exponent < -10 {
            return sign;
        }
        let mantissa = mantissa | 0x80_0000;
        let shift = 14 - half_exponent;
        let mut half = (mantissa >> shift) as u16;
        let round_bit = 1_u32 << (shift - 1);
        if mantissa & round_bit != 0 && (mantissa & (round_bit - 1) != 0 || half & 1 != 0) {
            half = half.wrapping_add(1);
        }
        return sign | half;
    }
    let mut half = sign | ((half_exponent as u16) << 10) | ((mantissa >> 13) as u16);
    if mantissa & 0x1000 != 0 && (mantissa & 0x0fff != 0 || half & 1 != 0) {
        half = half.wrapping_add(1);
    }
    half
}

fn memoryview_scalar_bytes(
    context: &mut RimeraContext,
    format: &str,
    value: RValue,
) -> Result<Vec<u8>, String> {
    let invalid_value = || format!("memoryview: invalid value for format '{format}'");
    match buffer_format_code(format)? {
        'B' => index_integer(context, value)?
            .to_u8()
            .map(|value| vec![value])
            .ok_or_else(invalid_value),
        'b' => index_integer(context, value)?
            .to_i8()
            .map(|value| value.to_ne_bytes().to_vec())
            .ok_or_else(invalid_value),
        '?' => Ok(vec![u8::from(truthy(context, value)?)]),
        'c' => match context.heap.get(value) {
            Some(HeapObject::Bytes(bytes)) if bytes.len() == 1 => Ok(bytes.clone()),
            Some(HeapObject::Bytes(_)) => Err(invalid_value()),
            _ => Err("memoryview: invalid type for format 'c'".to_owned()),
        },
        'h' => index_integer(context, value)?
            .to_i16()
            .map(|value| value.to_ne_bytes().to_vec())
            .ok_or_else(invalid_value),
        'H' => index_integer(context, value)?
            .to_u16()
            .map(|value| value.to_ne_bytes().to_vec())
            .ok_or_else(invalid_value),
        'i' => index_integer(context, value)?
            .to_i32()
            .map(|value| value.to_ne_bytes().to_vec())
            .ok_or_else(invalid_value),
        'I' => index_integer(context, value)?
            .to_u32()
            .map(|value| value.to_ne_bytes().to_vec())
            .ok_or_else(invalid_value),
        'l' => {
            let value = index_integer(context, value)?;
            if std::mem::size_of::<std::os::raw::c_long>() == 8 {
                value
                    .to_i64()
                    .map(|value| value.to_ne_bytes().to_vec())
                    .ok_or_else(invalid_value)
            } else {
                value
                    .to_i32()
                    .map(|value| value.to_ne_bytes().to_vec())
                    .ok_or_else(invalid_value)
            }
        }
        'L' => {
            let value = index_integer(context, value)?;
            if std::mem::size_of::<std::os::raw::c_ulong>() == 8 {
                value
                    .to_u64()
                    .map(|value| value.to_ne_bytes().to_vec())
                    .ok_or_else(invalid_value)
            } else {
                value
                    .to_u32()
                    .map(|value| value.to_ne_bytes().to_vec())
                    .ok_or_else(invalid_value)
            }
        }
        'q' => index_integer(context, value)?
            .to_i64()
            .map(|value| value.to_ne_bytes().to_vec())
            .ok_or_else(invalid_value),
        'Q' => index_integer(context, value)?
            .to_u64()
            .map(|value| value.to_ne_bytes().to_vec())
            .ok_or_else(invalid_value),
        'n' => index_integer(context, value)?
            .to_isize()
            .map(|value| value.to_ne_bytes().to_vec())
            .ok_or_else(invalid_value),
        'N' | 'P' => index_integer(context, value)?
            .to_usize()
            .map(|value| value.to_ne_bytes().to_vec())
            .ok_or_else(invalid_value),
        'f' => Ok((memoryview_numeric_float(context, value)? as f32)
            .to_ne_bytes()
            .to_vec()),
        'd' => Ok(memoryview_numeric_float(context, value)?
            .to_ne_bytes()
            .to_vec()),
        'e' => Ok(f32_to_f16(memoryview_numeric_float(context, value)? as f32)
            .to_ne_bytes()
            .to_vec()),
        _ => Err("memoryview: unsupported scalar format".to_owned()),
    }
}

fn memoryview_scalar_set(
    context: &mut RimeraContext,
    view: &MemoryViewObject,
    indices: &[usize],
    value: RValue,
) -> Result<(), String> {
    let encoded = memoryview_scalar_bytes(context, &view.format, value)?;
    if encoded.len() != view.item_size {
        return Err("memoryview scalar size does not match itemsize".to_owned());
    }
    let offset = memoryview_offset(view, indices)?;
    let end = offset
        .checked_add(view.item_size)
        .ok_or_else(|| "memoryview bounds overflow".to_owned())?;
    let Some(HeapObject::ByteArray(bytes)) = context.heap.get_mut(view.exporter) else {
        return Err("memoryview exporter is not writable".to_owned());
    };
    let target = bytes
        .bytes
        .get_mut(offset..end)
        .ok_or_else(|| "memoryview exporter is shorter than its view".to_owned())?;
    target.copy_from_slice(&encoded);
    Ok(())
}

fn f16_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = (bits >> 10) & 0x1f;
    let mantissa = u32::from(bits & 0x03ff);
    let result = match exponent {
        0 if mantissa == 0 => sign,
        0 => {
            let mut mantissa = mantissa;
            let mut exponent = 127 - 15 + 1;
            while mantissa & 0x0400 == 0 {
                mantissa <<= 1;
                exponent -= 1;
            }
            sign | ((exponent as u32) << 23) | ((mantissa & 0x03ff) << 13)
        }
        31 => sign | 0x7f80_0000 | (mantissa << 13),
        exponent => sign | ((u32::from(exponent) + 127 - 15) << 23) | (mantissa << 13),
    };
    f32::from_bits(result)
}

pub fn value_array_get(
    context: &RimeraContext,
    array: RValue,
    index: usize,
) -> Result<RValue, String> {
    match context.heap.get(array) {
        Some(HeapObject::ValueArray(values)) => values
            .get(index)
            .copied()
            .ok_or_else(|| "managed value-array index is out of range".to_owned()),
        Some(_) => Err("operation requires a managed value array".to_owned()),
        None => Err("value contains a stale heap handle".to_owned()),
    }
}

pub fn unary(context: &mut RimeraContext, op: u8, operand: RValue) -> Result<RValue, String> {
    context.with_temporary_roots(&[operand], |context| unary_rooted(context, op, operand))
}

pub fn absolute(context: &mut RimeraContext, value: RValue) -> Result<RValue, String> {
    context.with_temporary_roots(&[value], |context| match context.heap.get(value) {
        Some(HeapObject::Float(value)) => float(context, value.abs()),
        Some(HeapObject::Complex { real, imag }) => float(context, real.hypot(*imag)),
        _ => match integer(context, value) {
            Ok(value) => store_integer(context, value.abs()),
            Err(_) => context
                .invoke_special_method(value, "__abs__", &[])?
                .ok_or_else(|| "bad operand type for abs()".to_owned()),
        },
    })
}

fn unary_rooted(context: &mut RimeraContext, op: u8, operand: RValue) -> Result<RValue, String> {
    match op {
        0 => match context.heap.get(operand) {
            Some(HeapObject::Float(_) | HeapObject::Complex { .. }) => Ok(operand),
            _ => match integer(context, operand) {
                Ok(_) => Ok(operand),
                Err(_) => context.generic_unary(op, operand),
            },
        },
        1 => match context.heap.get(operand) {
            Some(HeapObject::Float(value)) => float(context, -*value),
            Some(HeapObject::Complex { real, imag }) => complex(context, -*real, -*imag),
            _ => match integer(context, operand) {
                Ok(value) => store_integer(context, -value),
                Err(_) => context.generic_unary(op, operand),
            },
        },
        2 => match integer(context, operand) {
            Ok(value) => store_integer(context, !value),
            Err(_) => context.generic_unary(op, operand),
        },
        3 => Ok(RValue::boolean(!truthy(context, operand)?)),
        _ => Err("unknown unary operation".to_owned()),
    }
}

pub fn binary(
    context: &mut RimeraContext,
    op: u8,
    left: RValue,
    right: RValue,
) -> Result<RValue, String> {
    context.with_temporary_roots(&[left, right], |context| {
        binary_rooted(context, op, left, right)
    })
}

fn complex_divide(
    left_real: f64,
    left_imag: f64,
    right_real: f64,
    right_imag: f64,
) -> Option<(f64, f64)> {
    if right_real == 0.0 && right_imag == 0.0 {
        return None;
    }
    let (real, imag) = if right_real.abs() >= right_imag.abs() {
        let ratio = right_imag / right_real;
        let denominator = right_real + right_imag * ratio;
        (
            (left_real + left_imag * ratio) / denominator,
            (left_imag - left_real * ratio) / denominator,
        )
    } else {
        let ratio = right_real / right_imag;
        let denominator = right_imag + right_real * ratio;
        (
            (left_real * ratio + left_imag) / denominator,
            (left_imag * ratio - left_real) / denominator,
        )
    };
    Some((real, imag))
}

fn complex_power(
    base_real: f64,
    base_imag: f64,
    exponent_real: f64,
    exponent_imag: f64,
) -> Result<(f64, f64), &'static str> {
    if base_real == 0.0 && base_imag == 0.0 {
        if exponent_real == 0.0 && exponent_imag == 0.0 {
            return Ok((1.0, 0.0));
        }
        if exponent_imag == 0.0 && exponent_real > 0.0 {
            return Ok((0.0, 0.0));
        }
        return Err("0.0 to a negative or complex power");
    }
    if exponent_imag == 0.0
        && exponent_real.is_finite()
        && exponent_real.fract() == 0.0
        && exponent_real.abs() <= i32::MAX as f64
    {
        let exponent = exponent_real as i64;
        let negative = exponent < 0;
        let mut remaining = exponent.unsigned_abs();
        let mut result = (1.0, 0.0);
        let mut factor = (base_real, base_imag);
        while remaining != 0 {
            if remaining & 1 != 0 {
                result = (
                    result.0 * factor.0 - result.1 * factor.1,
                    result.0 * factor.1 + result.1 * factor.0,
                );
            }
            remaining >>= 1;
            if remaining != 0 {
                factor = (
                    factor.0 * factor.0 - factor.1 * factor.1,
                    2.0 * factor.0 * factor.1,
                );
            }
        }
        if negative {
            return Ok(complex_divide(1.0, 0.0, result.0, result.1)
                .expect("non-zero complex base has a non-zero integral power"));
        }
        return Ok(result);
    }
    let log_radius = base_real.hypot(base_imag).ln();
    let angle = base_imag.atan2(base_real);
    let magnitude_log = exponent_real * log_radius - exponent_imag * angle;
    let result_angle = exponent_imag * log_radius + exponent_real * angle;
    let magnitude = magnitude_log.exp();
    Ok((
        magnitude * result_angle.cos(),
        magnitude * result_angle.sin(),
    ))
}

fn binary_rooted(
    context: &mut RimeraContext,
    op: u8,
    left: RValue,
    right: RValue,
) -> Result<RValue, String> {
    let has_complex_operand = matches!(context.heap.get(left), Some(HeapObject::Complex { .. }))
        || matches!(context.heap.get(right), Some(HeapObject::Complex { .. }));
    if has_complex_operand
        && let (Some((left_real, left_imag)), Some((right_real, right_imag))) = (
            numeric_complex(context, left),
            numeric_complex(context, right),
        )
    {
        let (real, imag) = match op {
            0 => (left_real + right_real, left_imag + right_imag),
            1 => (left_real - right_real, left_imag - right_imag),
            2 => (
                left_real * right_real - left_imag * right_imag,
                left_real * right_imag + left_imag * right_real,
            ),
            5 => match complex_divide(left_real, left_imag, right_real, right_imag) {
                Some(value) => value,
                None => {
                    return context.raise_error("ZeroDivisionError", "complex division by zero");
                }
            },
            6 => match complex_power(left_real, left_imag, right_real, right_imag) {
                Ok(value) => value,
                Err(message) => return context.raise_error("ZeroDivisionError", message),
            },
            _ => {
                return context
                    .generic_binary(op, left, right)?
                    .ok_or_else(|| "binary operation has no implementation".to_owned());
            }
        };
        return complex(context, real, imag);
    }
    if op == 11
        && let Some(result) = native_dictionary_union(context, left, right)?
    {
        return Ok(result);
    }
    if matches!(op, 1 | 9 | 10 | 11) {
        let left_values = set_like_values(context, left)?;
        let right_values = set_like_values(context, right)?;
        if let (Some(left_values), Some(right_values)) = (left_values, right_values) {
            let mut result = Vec::new();
            match op {
                11 => {
                    result.extend(left_values.iter().copied());
                    result.extend(right_values.iter().copied());
                }
                9 => {
                    for value in left_values.iter().copied() {
                        if contains(context, right, value)? {
                            result.push(value);
                        }
                    }
                }
                1 => {
                    for value in left_values.iter().copied() {
                        if !contains(context, right, value)? {
                            result.push(value);
                        }
                    }
                }
                10 => {
                    for value in left_values.iter().copied() {
                        if !contains(context, right, value)? {
                            result.push(value);
                        }
                    }
                    for value in right_values.iter().copied() {
                        if !contains(context, left, value)? {
                            result.push(value);
                        }
                    }
                }
                _ => unreachable!(),
            }
            return if matches!(context.heap.get(left), Some(HeapObject::FrozenSet(_))) {
                frozenset(context, &result)
            } else {
                set(context, &result)
            };
        }
    }
    if op == 0
        && let (Some(left), Some(right)) =
            (string_value(context, left), string_value(context, right))
    {
        let concatenated = format!("{left}{right}");
        return string(context, &concatenated);
    }
    if op == 0
        && let (Some((left, left_mutable)), Some((right, right_mutable))) =
            (byte_sequence(context, left), byte_sequence(context, right))
    {
        if left_mutable != right_mutable {
            return Err("can't concat bytes to bytearray".to_owned());
        }
        let mut values = left;
        values.extend(right);
        return if left_mutable {
            bytearray(context, &values)
        } else {
            bytes(context, &values)
        };
    }
    if op == 0
        && let (Some((mut left_values, left_kind)), Some((right_values, right_kind))) = (
            sequence_values(context, left),
            sequence_values(context, right),
        )
    {
        if left_kind != right_kind {
            return Err("can only concatenate matching sequence types".to_owned());
        }
        left_values.extend(right_values);
        return allocate_sequence(context, left_kind, left_values);
    }
    if op == 2 {
        if let Some((values, mutable)) = byte_sequence(context, left) {
            let count = integer(context, right)?.to_usize().unwrap_or(0);
            let values = values.repeat(count);
            return if mutable {
                bytearray(context, &values)
            } else {
                bytes(context, &values)
            };
        }
        if let Some((values, mutable)) = byte_sequence(context, right) {
            let count = integer(context, left)?.to_usize().unwrap_or(0);
            let values = values.repeat(count);
            return if mutable {
                bytearray(context, &values)
            } else {
                bytes(context, &values)
            };
        }
        if let Some((values, kind)) = sequence_values(context, left) {
            return repeat_sequence(context, kind, values, integer(context, right)?);
        }
        if let Some((values, kind)) = sequence_values(context, right) {
            return repeat_sequence(context, kind, values, integer(context, left)?);
        }
    }
    if let (Some(left_float), Some(right_float)) =
        (numeric_float(context, left), numeric_float(context, right))
        && (matches!(context.heap.get(left), Some(HeapObject::Float(_)))
            || matches!(context.heap.get(right), Some(HeapObject::Float(_))))
    {
        let value = match op {
            0 => left_float + right_float,
            1 => left_float - right_float,
            2 => left_float * right_float,
            3 if right_float != 0.0 => (left_float / right_float).floor(),
            4 if right_float != 0.0 => {
                left_float - (left_float / right_float).floor() * right_float
            }
            5 if right_float != 0.0 => left_float / right_float,
            6 => left_float.powf(right_float),
            3 | 4 => return Err("float division or modulo by zero".to_owned()),
            _ => {
                return context
                    .generic_binary(op, left, right)?
                    .ok_or_else(|| "binary operation has no implementation".to_owned());
            }
        };
        return float(context, value);
    }
    if let (Ok(left_integer), Ok(right_integer)) = (integer(context, left), integer(context, right))
    {
        let value = match op {
            0 => left_integer + right_integer,
            1 => left_integer - right_integer,
            2 => left_integer * right_integer,
            3 => floor_division(&left_integer, &right_integer)?.0,
            4 => floor_division(&left_integer, &right_integer)?.1,
            5 => {
                if right_integer.is_zero() {
                    return Err("division by zero".to_owned());
                }
                return float(
                    context,
                    left_integer.to_f64().unwrap_or_else(|| {
                        if left_integer.sign() == num_bigint::Sign::Minus {
                            f64::NEG_INFINITY
                        } else {
                            f64::INFINITY
                        }
                    }) / right_integer.to_f64().unwrap_or_else(|| {
                        if right_integer.sign() == num_bigint::Sign::Minus {
                            f64::NEG_INFINITY
                        } else {
                            f64::INFINITY
                        }
                    }),
                );
            }
            6 => {
                if right_integer.sign() == num_bigint::Sign::Minus {
                    return float(
                        context,
                        left_integer
                            .to_f64()
                            .unwrap_or_else(|| {
                                if left_integer.sign() == num_bigint::Sign::Minus {
                                    f64::NEG_INFINITY
                                } else {
                                    f64::INFINITY
                                }
                            })
                            .powf(right_integer.to_f64().unwrap_or(f64::INFINITY)),
                    );
                }
                left_integer.pow(
                    right_integer
                        .to_u32()
                        .ok_or_else(|| "exponent too large".to_owned())?,
                )
            }
            7 => {
                left_integer
                    << right_integer
                        .to_usize()
                        .ok_or_else(|| "negative shift count".to_owned())?
            }
            8 => {
                left_integer
                    >> right_integer
                        .to_usize()
                        .ok_or_else(|| "negative shift count".to_owned())?
            }
            9 => left_integer & right_integer,
            10 => left_integer ^ right_integer,
            11 => left_integer | right_integer,
            _ => {
                return context
                    .generic_binary(op, left, right)?
                    .ok_or_else(|| "binary operation has no implementation".to_owned());
            }
        };
        return store_integer(context, value);
    }
    context
        .generic_binary(op, left, right)?
        .ok_or_else(|| "binary operation has no implementation".to_owned())
}

fn byte_sequence(context: &RimeraContext, value: RValue) -> Option<(Vec<u8>, bool)> {
    match context.heap.get(value) {
        Some(HeapObject::Bytes(values)) => Some((values.clone(), false)),
        Some(HeapObject::ByteArray(values)) => Some((values.bytes.clone(), true)),
        _ => None,
    }
}
type ComparableBuffer = (Vec<u8>, Vec<usize>, String);

#[derive(Clone)]
enum LogicalBuffer {
    Bytes(Vec<u8>),
    View(MemoryViewObject),
}

fn logical_buffer(context: &RimeraContext, value: RValue) -> Option<LogicalBuffer> {
    match context.heap.get(value) {
        Some(HeapObject::Bytes(values)) => Some(LogicalBuffer::Bytes(values.clone())),
        Some(HeapObject::ByteArray(values)) => Some(LogicalBuffer::Bytes(values.bytes.clone())),
        Some(HeapObject::MemoryView(view)) if !view.released => {
            Some(LogicalBuffer::View(view.clone()))
        }
        _ => None,
    }
}

fn logical_buffer_shape(buffer: &LogicalBuffer) -> Vec<usize> {
    match buffer {
        LogicalBuffer::Bytes(values) => vec![values.len()],
        LogicalBuffer::View(view) => view.shape.to_vec(),
    }
}

fn linear_memoryview_indices(shape: &[usize], linear: usize) -> Vec<usize> {
    let mut remainder = linear;
    let mut indices = vec![0; shape.len()];
    for axis in (0..shape.len()).rev() {
        if shape[axis] != 0 {
            indices[axis] = remainder % shape[axis];
            remainder /= shape[axis];
        }
    }
    indices
}

fn logical_buffer_scalar(
    context: &mut RimeraContext,
    buffer: &LogicalBuffer,
    linear: usize,
) -> Result<RValue, String> {
    match buffer {
        LogicalBuffer::Bytes(values) => values
            .get(linear)
            .copied()
            .map(|value| RValue::small_int(i64::from(value)))
            .ok_or_else(|| "buffer index out of range".to_owned()),
        LogicalBuffer::View(view) => {
            let indices = linear_memoryview_indices(&view.shape, linear);
            memoryview_scalar_get(context, view, &indices)
        }
    }
}

fn memoryview_logical_equal(
    context: &mut RimeraContext,
    left: RValue,
    right: RValue,
) -> Result<Option<bool>, String> {
    let left_is_view = matches!(context.heap.get(left), Some(HeapObject::MemoryView(_)));
    let right_is_view = matches!(context.heap.get(right), Some(HeapObject::MemoryView(_)));
    if !left_is_view && !right_is_view {
        return Ok(None);
    }
    let Some(left_buffer) = logical_buffer(context, left) else {
        return Ok(Some(false));
    };
    let Some(right_buffer) = logical_buffer(context, right) else {
        return Ok(Some(false));
    };
    let left_shape = logical_buffer_shape(&left_buffer);
    let right_shape = logical_buffer_shape(&right_buffer);
    if left_shape != right_shape {
        return Ok(Some(false));
    }
    let total = left_shape
        .iter()
        .try_fold(1_usize, |total, dimension| total.checked_mul(*dimension))
        .ok_or_else(|| "memoryview dimensions overflow".to_owned())?;
    for linear in 0..total {
        let left_value = logical_buffer_scalar(context, &left_buffer, linear)?;
        let equal = context.with_temporary_roots(&[left_value], |context| {
            let right_value = logical_buffer_scalar(context, &right_buffer, linear)?;
            context.with_temporary_roots(&[left_value, right_value], |context| {
                value_equal(context, left_value, right_value)
            })
        })?;
        if !equal {
            return Ok(Some(false));
        }
    }
    Ok(Some(true))
}

fn comparable_buffer(
    context: &RimeraContext,
    value: RValue,
) -> Result<Option<ComparableBuffer>, String> {
    match context.heap.get(value) {
        Some(HeapObject::Bytes(values)) => {
            Ok(Some((values.clone(), vec![values.len()], "B".to_owned())))
        }
        Some(HeapObject::ByteArray(values)) => Ok(Some((
            values.bytes.clone(),
            vec![values.bytes.len()],
            "B".to_owned(),
        ))),
        Some(HeapObject::MemoryView(view)) => Ok(Some((
            memoryview_bytes(context, view)?,
            view.shape.to_vec(),
            view.format.clone(),
        ))),
        _ => Ok(None),
    }
}

fn set_like_values(
    context: &mut RimeraContext,
    value: RValue,
) -> Result<Option<Vec<RValue>>, String> {
    match context.heap.get(value) {
        Some(HeapObject::Set(set) | HeapObject::FrozenSet(set)) => {
            Ok(Some(set.table.values().copied().collect()))
        }
        Some(HeapObject::DictionaryView(view))
            if matches!(
                view.kind,
                DictionaryViewKind::Keys | DictionaryViewKind::Items
            ) =>
        {
            collect_iterable(context, value).map(Some)
        }
        _ => Ok(None),
    }
}

fn value_dictionary_find_entry(
    context: &mut RimeraContext,
    dictionary: RValue,
    key: RValue,
    hash: i64,
) -> Result<Option<usize>, String> {
    'restart: loop {
        let candidates = match context.heap.get(dictionary) {
            Some(HeapObject::ValueDictionary(dictionary)) => dictionary.table.candidates(hash),
            _ => return Ok(None),
        };
        for position in candidates {
            let (candidate, version_before) = match context.heap.get(dictionary) {
                Some(HeapObject::ValueDictionary(dictionary)) => {
                    let Some((candidate, _)) = dictionary.table.value(position).copied() else {
                        continue;
                    };
                    (candidate, dictionary.table.version)
                }
                _ => return Ok(None),
            };
            if candidate == key {
                return Ok(Some(position));
            }
            let equal = value_equal(context, candidate, key)?;
            let (live_candidate, version_after) = match context.heap.get(dictionary) {
                Some(HeapObject::ValueDictionary(dictionary)) => (
                    dictionary.table.value(position).map(|(key, _)| *key),
                    dictionary.table.version,
                ),
                _ => return Ok(None),
            };
            if equal && live_candidate == Some(candidate) {
                return Ok(Some(position));
            }
            if version_after != version_before {
                continue 'restart;
            }
        }
        return Ok(None);
    }
}

fn value_dictionary_lookup(
    context: &mut RimeraContext,
    dictionary: RValue,
    key: RValue,
) -> Result<Option<RValue>, String> {
    let hash = hash_i64(context, key)?;
    let Some(position) = value_dictionary_find_entry(context, dictionary, key, hash)? else {
        return Ok(None);
    };
    Ok(match context.heap.get(dictionary) {
        Some(HeapObject::ValueDictionary(dictionary)) => {
            dictionary.table.value(position).map(|(_, value)| *value)
        }
        _ => None,
    })
}

fn set_find_entry(
    context: &mut RimeraContext,
    receiver: RValue,
    value: RValue,
    hash: i64,
) -> Result<Option<usize>, String> {
    'restart: loop {
        let candidates = match context.heap.get(receiver) {
            Some(HeapObject::Set(set) | HeapObject::FrozenSet(set)) => set.table.candidates(hash),
            _ => return Ok(None),
        };
        for position in candidates {
            let (candidate, version_before) = match context.heap.get(receiver) {
                Some(HeapObject::Set(set) | HeapObject::FrozenSet(set)) => {
                    let Some(candidate) = set.table.value(position).copied() else {
                        continue;
                    };
                    (candidate, set.table.version)
                }
                _ => return Ok(None),
            };
            if candidate == value {
                return Ok(Some(position));
            }
            let equal = value_equal(context, candidate, value)?;
            let (live_candidate, version_after) = match context.heap.get(receiver) {
                Some(HeapObject::Set(set) | HeapObject::FrozenSet(set)) => {
                    (set.table.value(position).copied(), set.table.version)
                }
                _ => return Ok(None),
            };
            if equal && live_candidate == Some(candidate) {
                return Ok(Some(position));
            }
            if version_after != version_before {
                continue 'restart;
            }
        }
        return Ok(None);
    }
}

pub(crate) fn dictionary_pop_value(
    context: &mut RimeraContext,
    dictionary: RValue,
    key: RValue,
) -> Result<Option<RValue>, String> {
    if let Some(storage) = instance_storage(context, dictionary) {
        return dictionary_pop_value(context, storage, key);
    }
    if matches!(
        context.heap.get(dictionary),
        Some(HeapObject::ValueDictionary(_))
    ) {
        let hash = hash_i64(context, key)?;
        let Some(position) = value_dictionary_find_entry(context, dictionary, key, hash)? else {
            return Ok(None);
        };
        let Some(HeapObject::ValueDictionary(dictionary)) = context.heap.get_mut(dictionary) else {
            return Ok(None);
        };
        return Ok(dictionary.table.remove(position).map(|(_, value)| value));
    }
    if let Some(key_text) = string_value(context, key).map(ToOwned::to_owned)
        && let Some(HeapObject::Dictionary(dictionary)) = context.heap.get_mut(dictionary)
    {
        return Ok(dictionary.remove(&key_text));
    }
    Ok(None)
}

pub(crate) fn set_remove_value(
    context: &mut RimeraContext,
    receiver: RValue,
    value: RValue,
) -> Result<bool, String> {
    let target = instance_storage(context, receiver).unwrap_or(receiver);
    if !matches!(context.heap.get(target), Some(HeapObject::Set(_))) {
        return Ok(false);
    }
    let hash = hash_i64(context, value)?;
    let Some(position) = set_find_entry(context, target, value, hash)? else {
        return Ok(false);
    };
    let Some(HeapObject::Set(set)) = context.heap.get_mut(target) else {
        return Ok(false);
    };
    Ok(set.table.remove(position).is_some())
}

fn native_dictionary_storage(context: &RimeraContext, value: RValue) -> Option<RValue> {
    if matches!(
        context.heap.get(value),
        Some(HeapObject::ValueDictionary(_))
    ) {
        return Some(value);
    }
    instance_storage(context, value).filter(|storage| {
        matches!(
            context.heap.get(*storage),
            Some(HeapObject::ValueDictionary(_))
        )
    })
}

pub(crate) fn merge_native_dictionary_source(
    context: &mut RimeraContext,
    target: RValue,
    source: RValue,
) -> Result<bool, String> {
    let Some(target) = native_dictionary_storage(context, target) else {
        return Ok(false);
    };
    let Some(source) = native_dictionary_storage(context, source) else {
        return Ok(false);
    };
    let entries = match context.heap.get(source) {
        Some(HeapObject::ValueDictionary(dictionary)) => dictionary.table.hashed_snapshot(),
        _ => return Ok(false),
    };
    let mut roots = vec![target, source];
    for (_, _, (key, value)) in &entries {
        roots.extend([*key, *value]);
    }
    context.with_temporary_roots(&roots, |context| {
        for (_, hash, (key, value)) in entries {
            let replacement = value_dictionary_find_entry(context, target, key, hash)?;
            let Some(HeapObject::ValueDictionary(dictionary)) = context.heap.get_mut(target) else {
                return Err("dictionary storage changed during merge".to_owned());
            };
            if let Some(position) = replacement
                && dictionary
                    .table
                    .update(position, |(_, existing)| *existing = value)
            {
                continue;
            }
            dictionary.table.insert_new(hash, (key, value));
        }
        Ok(true)
    })
}

fn native_dictionary_union(
    context: &mut RimeraContext,
    left: RValue,
    right: RValue,
) -> Result<Option<RValue>, String> {
    let Some(left_storage) = native_dictionary_storage(context, left) else {
        return Ok(None);
    };
    if native_dictionary_storage(context, right).is_none() {
        return Ok(None);
    }
    let left_dictionary = match context.heap.get(left_storage) {
        Some(HeapObject::ValueDictionary(dictionary)) => dictionary.clone(),
        _ => return Ok(None),
    };
    let result = context.allocate(HeapObject::ValueDictionary(left_dictionary))?;
    context.with_temporary_roots(&[result, right], |context| {
        merge_native_dictionary_source(context, result, right)?;
        Ok(Some(result))
    })
}

fn range_length_value(range: &RangeObject) -> BigInt {
    if range.step.sign() == num_bigint::Sign::Minus {
        if range.start <= range.stop {
            BigInt::from(0_u8)
        } else {
            ((&range.start - &range.stop - 1_u8) / (-&range.step)) + 1_u8
        }
    } else if range.start >= range.stop {
        BigInt::from(0_u8)
    } else {
        ((&range.stop - &range.start - 1_u8) / &range.step) + 1_u8
    }
}

fn integral_float_bigint(value: f64) -> Option<BigInt> {
    if !value.is_finite() {
        return None;
    }
    if value == 0.0 {
        return Some(BigInt::ZERO);
    }
    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (significand, exponent) = if exponent_bits == 0 {
        (fraction, -1074)
    } else {
        ((1_u64 << 52) | fraction, exponent_bits - 1023 - 52)
    };
    let mut integer = BigInt::from(significand);
    if exponent >= 0 {
        integer <<= exponent as usize;
    } else {
        let shift = usize::try_from(-exponent).ok()?;
        if shift >= 64 {
            return None;
        }
        let mask = (1_u64 << shift) - 1;
        if significand & mask != 0 {
            return None;
        }
        integer >>= shift;
    }
    Some(if negative { -integer } else { integer })
}

fn range_integer_position(range: &RangeObject, value: &BigInt) -> Option<BigInt> {
    let length = range_length_value(range);
    if length.is_zero() {
        return None;
    }
    let delta = value - &range.start;
    if (&delta % &range.step) != BigInt::ZERO {
        return None;
    }
    let position = delta / &range.step;
    (position.sign() != num_bigint::Sign::Minus && position < length).then_some(position)
}

pub(crate) fn range_position(
    context: &mut RimeraContext,
    range_value: RValue,
    needle: RValue,
) -> Result<Option<BigInt>, String> {
    context.with_temporary_roots(&[range_value, needle], |context| {
        let Some(HeapObject::Range(range)) = context.heap.get(range_value) else {
            return Err("range method receiver is invalid".to_owned());
        };
        let range = range.clone();
        if let Ok(value) = integer(context, needle) {
            return Ok(range_integer_position(&range, &value));
        }
        if let Some(value) = direct_float(context, needle).and_then(integral_float_bigint) {
            return Ok(range_integer_position(&range, &value));
        }
        if let Some((real, imag)) = direct_complex(context, needle)
            && imag == 0.0
            && let Some(value) = integral_float_bigint(real)
        {
            return Ok(range_integer_position(&range, &value));
        }

        let mut current = range.start.clone();
        let mut position = BigInt::ZERO;
        let active = |current: &BigInt| {
            if range.step.sign() == num_bigint::Sign::Minus {
                current > &range.stop
            } else {
                current < &range.stop
            }
        };
        while active(&current) {
            let item = store_integer(context, current.clone())?;
            let equal = context.with_temporary_roots(&[needle, item], |context| {
                value_equal(context, item, needle)
            })?;
            if equal {
                return Ok(Some(position));
            }
            current += &range.step;
            position += 1_u8;
        }
        Ok(None)
    })
}

fn ranges_equal(left: &RangeObject, right: &RangeObject) -> bool {
    let left_length = range_length_value(left);
    let right_length = range_length_value(right);
    if left_length != right_length {
        return false;
    }
    if left_length.is_zero() {
        return true;
    }
    if left.start != right.start {
        return false;
    }
    left_length == BigInt::from(1_u8) || left.step == right.step
}

fn compare_value_sequences(
    context: &mut RimeraContext,
    left: &[RValue],
    right: &[RValue],
    op: u8,
) -> Result<bool, String> {
    for (left_value, right_value) in left.iter().zip(right) {
        if value_equal(context, *left_value, *right_value)? {
            continue;
        }
        return match op {
            0 => Ok(false),
            1 => Ok(true),
            2..=5 => {
                let result = compare(context, op, *left_value, *right_value)?;
                truthy(context, result)
            }
            _ => Err("unknown comparison operation".to_owned()),
        };
    }
    Ok(match op {
        0 => left.len() == right.len(),
        1 => left.len() != right.len(),
        2 => left.len() < right.len(),
        3 => left.len() <= right.len(),
        4 => left.len() > right.len(),
        5 => left.len() >= right.len(),
        _ => return Err("unknown comparison operation".to_owned()),
    })
}

fn exact_integer_float_order(integer: &BigInt, value: f64) -> Option<Ordering> {
    if value.is_nan() {
        return None;
    }
    if value == f64::INFINITY {
        return Some(Ordering::Less);
    }
    if value == f64::NEG_INFINITY {
        return Some(Ordering::Greater);
    }
    if value == 0.0 {
        return Some(integer.cmp(&BigInt::ZERO));
    }

    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (significand, exponent) = if exponent_bits == 0 {
        (fraction, -1074)
    } else {
        ((1_u64 << 52) | fraction, exponent_bits - 1023 - 52)
    };
    let mut numerator = BigInt::from(significand);
    if negative {
        numerator = -numerator;
    }
    if exponent >= 0 {
        numerator <<= exponent as usize;
        Some(integer.cmp(&numerator))
    } else {
        let scaled_integer = integer << (-exponent) as usize;
        Some(scaled_integer.cmp(&numerator))
    }
}

fn partial_order_result(ordering: Option<Ordering>, op: u8) -> Result<bool, String> {
    Ok(match op {
        0 => ordering == Some(Ordering::Equal),
        1 => ordering != Some(Ordering::Equal),
        2 => ordering == Some(Ordering::Less),
        3 => matches!(ordering, Some(Ordering::Less | Ordering::Equal)),
        4 => ordering == Some(Ordering::Greater),
        5 => matches!(ordering, Some(Ordering::Greater | Ordering::Equal)),
        _ => return Err("unknown comparison operation".to_owned()),
    })
}

fn direct_float(context: &RimeraContext, value: RValue) -> Option<f64> {
    match context.heap.get(value) {
        Some(HeapObject::Float(value)) => Some(*value),
        _ => None,
    }
}

fn direct_complex(context: &RimeraContext, value: RValue) -> Option<(f64, f64)> {
    match context.heap.get(value) {
        Some(HeapObject::Complex { real, imag }) => Some((*real, *imag)),
        _ => None,
    }
}

fn exact_numeric_equality(context: &RimeraContext, left: RValue, right: RValue) -> Option<bool> {
    let left_complex = direct_complex(context, left);
    let right_complex = direct_complex(context, right);
    if left_complex.is_some() || right_complex.is_some() {
        return match (left_complex, right_complex) {
            (Some((left_real, left_imag)), Some((right_real, right_imag))) => {
                Some(left_real == right_real && left_imag == right_imag)
            }
            (Some((_, imag)), None) if imag != 0.0 => (integer(context, right).is_ok()
                || direct_float(context, right).is_some())
            .then_some(false),
            (None, Some((_, imag))) if imag != 0.0 => (integer(context, left).is_ok()
                || direct_float(context, left).is_some())
            .then_some(false),
            (Some((real, _)), None) => {
                if let Some(other) = direct_float(context, right) {
                    Some(real == other)
                } else {
                    integer(context, right).ok().map(|other| {
                        exact_integer_float_order(&other, real) == Some(Ordering::Equal)
                    })
                }
            }
            (None, Some((real, _))) => {
                if let Some(other) = direct_float(context, left) {
                    Some(other == real)
                } else {
                    integer(context, left).ok().map(|other| {
                        exact_integer_float_order(&other, real) == Some(Ordering::Equal)
                    })
                }
            }
            _ => None,
        };
    }

    let left_float = direct_float(context, left);
    let right_float = direct_float(context, right);
    if left_float.is_some() || right_float.is_some() {
        return match (left_float, right_float) {
            (Some(left), Some(right)) => Some(left == right),
            (Some(left), None) => integer(context, right)
                .ok()
                .map(|right| exact_integer_float_order(&right, left) == Some(Ordering::Equal)),
            (None, Some(right)) => integer(context, left)
                .ok()
                .map(|left| exact_integer_float_order(&left, right) == Some(Ordering::Equal)),
            _ => None,
        };
    }
    None
}

pub fn compare(
    context: &mut RimeraContext,
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

    let has_complex_operand =
        direct_complex(context, left).is_some() || direct_complex(context, right).is_some();
    if has_complex_operand {
        if op <= 1
            && let Some(equal) = exact_numeric_equality(context, left, right)
        {
            return Ok(RValue::boolean(if op == 0 { equal } else { !equal }));
        }
        return context.generic_compare(op, left, right);
    }

    let left_float = direct_float(context, left);
    let right_float = direct_float(context, right);
    if left_float.is_some() || right_float.is_some() {
        let ordering = match (left_float, right_float) {
            (Some(left_number), Some(right_number)) => left_number.partial_cmp(&right_number),
            (Some(left_number), None) => {
                let Some(right_integer) = integer(context, right).ok() else {
                    return context.generic_compare(op, left, right);
                };
                exact_integer_float_order(&right_integer, left_number).map(Ordering::reverse)
            }
            (None, Some(right_number)) => {
                let Some(left_integer) = integer(context, left).ok() else {
                    return context.generic_compare(op, left, right);
                };
                exact_integer_float_order(&left_integer, right_number)
            }
            _ => unreachable!(),
        };
        return Ok(RValue::boolean(partial_order_result(ordering, op)?));
    }

    let left_memoryview = matches!(context.heap.get(left), Some(HeapObject::MemoryView(_)));
    let right_memoryview = matches!(context.heap.get(right), Some(HeapObject::MemoryView(_)));
    if left_memoryview || right_memoryview {
        if op > 1 {
            return context.generic_compare(op, left, right);
        }
        let equal = memoryview_logical_equal(context, left, right)?.unwrap_or(false);
        return Ok(RValue::boolean(if op == 0 { equal } else { !equal }));
    }
    let left_buffer = comparable_buffer(context, left)?;
    let right_buffer = comparable_buffer(context, right)?;
    if let (Some((left_bytes, _, _)), Some((right_bytes, _, _))) = (left_buffer, right_buffer) {
        let ordering = left_bytes.cmp(&right_bytes);
        let result = match op {
            0 => ordering == Ordering::Equal,
            1 => ordering != Ordering::Equal,
            2 => ordering == Ordering::Less,
            3 => ordering != Ordering::Greater,
            4 => ordering == Ordering::Greater,
            5 => ordering != Ordering::Less,
            _ => return Err("unknown comparison operation".to_owned()),
        };
        return Ok(RValue::boolean(result));
    }
    if (left_memoryview || right_memoryview) && op <= 1 {
        return Ok(RValue::boolean(op == 1));
    }

    let left_set_like = set_like_values(context, left)?;
    let right_set_like = set_like_values(context, right)?;
    if let (Some(left_values), Some(right_values)) = (left_set_like, right_set_like) {
        let left_subset = |context: &mut RimeraContext| -> Result<bool, String> {
            for value in &left_values {
                if !contains(context, right, *value)? {
                    return Ok(false);
                }
            }
            Ok(true)
        };
        let right_subset = |context: &mut RimeraContext| -> Result<bool, String> {
            for value in &right_values {
                if !contains(context, left, *value)? {
                    return Ok(false);
                }
            }
            Ok(true)
        };
        let result = match op {
            0 => left_values.len() == right_values.len() && left_subset(context)?,
            1 => !(left_values.len() == right_values.len() && left_subset(context)?),
            2 => left_values.len() < right_values.len() && left_subset(context)?,
            3 => left_values.len() <= right_values.len() && left_subset(context)?,
            4 => left_values.len() > right_values.len() && right_subset(context)?,
            5 => left_values.len() >= right_values.len() && right_subset(context)?,
            _ => return Err("unknown comparison operation".to_owned()),
        };
        return Ok(RValue::boolean(result));
    }

    let left_proxy = match context.heap.get(left) {
        Some(HeapObject::MappingProxy(proxy)) => Some(proxy.dictionary),
        _ => None,
    };
    let right_proxy = match context.heap.get(right) {
        Some(HeapObject::MappingProxy(proxy)) => Some(proxy.dictionary),
        _ => None,
    };
    if left_proxy.is_some() || right_proxy.is_some() {
        if op > 1 {
            return context.generic_compare(op, left, right);
        }
        return compare(
            context,
            op,
            left_proxy.unwrap_or(left),
            right_proxy.unwrap_or(right),
        );
    }

    let left_dictionary = match context.heap.get(left) {
        Some(HeapObject::ValueDictionary(dictionary)) => Some(dictionary.table.snapshot()),
        _ => None,
    };
    let right_dictionary = match context.heap.get(right) {
        Some(HeapObject::ValueDictionary(dictionary)) => Some(dictionary.table.snapshot()),
        _ => None,
    };
    if let (Some(left_entries), Some(right_entries)) = (left_dictionary, right_dictionary) {
        if op > 1 {
            return context.generic_compare(op, left, right);
        }
        let mut equal = left_entries.len() == right_entries.len();
        if equal {
            for (_, (key, value)) in left_entries {
                let Some(other) = value_dictionary_lookup(context, right, key)? else {
                    equal = false;
                    break;
                };
                if !value_equal(context, value, other)? {
                    equal = false;
                    break;
                }
            }
        }
        return Ok(RValue::boolean(if op == 0 { equal } else { !equal }));
    }

    let left_slice = match context.heap.get(left) {
        Some(HeapObject::Slice(slice)) => Some(slice.clone()),
        _ => None,
    };
    let right_slice = match context.heap.get(right) {
        Some(HeapObject::Slice(slice)) => Some(slice.clone()),
        _ => None,
    };
    if let (Some(left_slice), Some(right_slice)) = (left_slice, right_slice) {
        let left_values = [
            left_slice.start.unwrap_or(RValue::NONE),
            left_slice.stop.unwrap_or(RValue::NONE),
            left_slice.step.unwrap_or(RValue::NONE),
        ];
        let right_values = [
            right_slice.start.unwrap_or(RValue::NONE),
            right_slice.stop.unwrap_or(RValue::NONE),
            right_slice.step.unwrap_or(RValue::NONE),
        ];
        return Ok(RValue::boolean(compare_value_sequences(
            context,
            &left_values,
            &right_values,
            op,
        )?));
    }

    let left_range = match context.heap.get(left) {
        Some(HeapObject::Range(range)) => Some(range.clone()),
        _ => None,
    };
    let right_range = match context.heap.get(right) {
        Some(HeapObject::Range(range)) => Some(range.clone()),
        _ => None,
    };
    if let (Some(left_range), Some(right_range)) = (left_range, right_range) {
        if op > 1 {
            return context.generic_compare(op, left, right);
        }
        let equal = ranges_equal(&left_range, &right_range);
        return Ok(RValue::boolean(if op == 0 { equal } else { !equal }));
    }

    let ordering = if let (Ok(left), Ok(right)) = (integer(context, left), integer(context, right))
    {
        left.cmp(&right)
    } else if let (Some(left), Some(right)) =
        (string_value(context, left), string_value(context, right))
    {
        left.cmp(right)
    } else if let (Some(left_sequence), Some(right_sequence)) = (
        sequence_values(context, left),
        sequence_values(context, right),
    ) {
        if left_sequence.1 != right_sequence.1 {
            return if op <= 1 {
                Ok(RValue::boolean(op == 1))
            } else {
                context.generic_compare(op, left, right)
            };
        }
        return Ok(RValue::boolean(compare_value_sequences(
            context,
            &left_sequence.0,
            &right_sequence.0,
            op,
        )?));
    } else {
        return context.generic_compare(op, left, right);
    };
    let result = match op {
        0 => ordering == Ordering::Equal,
        1 => ordering != Ordering::Equal,
        2 => ordering == Ordering::Less,
        3 => ordering != Ordering::Greater,
        4 => ordering == Ordering::Greater,
        5 => ordering != Ordering::Less,
        _ => return Err("unknown comparison operation".to_owned()),
    };
    Ok(RValue::boolean(result))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SequenceKind {
    List,
    Tuple,
}

fn sequence_values(context: &RimeraContext, value: RValue) -> Option<(Vec<RValue>, SequenceKind)> {
    match context.heap.get(value) {
        Some(HeapObject::List(values)) => Some((values.clone(), SequenceKind::List)),
        Some(HeapObject::Tuple(values)) => Some((values.to_vec(), SequenceKind::Tuple)),
        _ => None,
    }
}

fn allocate_sequence(
    context: &mut RimeraContext,
    kind: SequenceKind,
    values: Vec<RValue>,
) -> Result<RValue, String> {
    context.with_temporary_roots(&values, |context| match kind {
        SequenceKind::List => context.allocate(HeapObject::List(values.clone())),
        SequenceKind::Tuple => {
            context.allocate(HeapObject::Tuple(values.clone().into_boxed_slice()))
        }
    })
}

fn repeat_sequence(
    context: &mut RimeraContext,
    kind: SequenceKind,
    values: Vec<RValue>,
    count: BigInt,
) -> Result<RValue, String> {
    let count = count.to_usize().unwrap_or(0);
    let repeated = values.repeat(count);
    allocate_sequence(context, kind, repeated)
}

fn value_equal(context: &mut RimeraContext, left: RValue, right: RValue) -> Result<bool, String> {
    let value = compare(context, 0, left, right)?;
    truthy(context, value)
}

pub fn contains(
    context: &mut RimeraContext,
    collection: RValue,
    needle: RValue,
) -> Result<bool, String> {
    if let Some(storage) = instance_storage(context, collection) {
        if let Some(result) =
            context.invoke_special_method(collection, "__contains__", &[needle])?
        {
            return truthy(context, result);
        }
        return contains(context, storage, needle);
    }
    if let Some(HeapObject::MappingProxy(proxy)) = context.heap.get(collection) {
        return contains(context, proxy.dictionary, needle);
    }
    if let Some(HeapObject::Dictionary(dictionary)) = context.heap.get(collection) {
        let key = string_value(context, needle)
            .ok_or_else(|| "dictionary key must be string".to_owned())?;
        return Ok(dictionary.get(key).is_some());
    }
    if matches!(
        context.heap.get(collection),
        Some(HeapObject::ValueDictionary(_))
    ) {
        let hash = hash_i64(context, needle)?;
        return Ok(value_dictionary_find_entry(context, collection, needle, hash)?.is_some());
    }
    if matches!(
        context.heap.get(collection),
        Some(HeapObject::Set(_) | HeapObject::FrozenSet(_))
    ) {
        let hash = hash_i64(context, needle)?;
        return Ok(set_find_entry(context, collection, needle, hash)?.is_some());
    }
    let native_values = match context.heap.get(collection) {
        Some(HeapObject::List(values)) => Some(values.clone()),
        Some(HeapObject::Tuple(values)) => Some(values.to_vec()),
        _ => None,
    };
    if let Some(values) = native_values {
        for value in values {
            if value_equal(context, value, needle)? {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    match context.heap.get(collection) {
        Some(HeapObject::String(value)) => string_value(context, needle)
            .map(|needle| value.contains(needle))
            .ok_or_else(|| "'in <string>' requires string as left operand".to_owned()),
        Some(_) => {
            if let Some(result) =
                context.invoke_special_method(collection, "__contains__", &[needle])?
            {
                return truthy(context, result);
            }
            match iterator_new(context, collection) {
                Ok(iterator) => {
                    while let Some(item) = iterator_next(context, iterator)? {
                        let equal = compare(context, 0, item, needle)?;
                        if truthy(context, equal)? {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                }
                Err(_) => {
                    for index in 0_i64.. {
                        match item_get(context, collection, RValue::small_int(index)) {
                            Ok(item) => {
                                let equal = compare(context, 0, item, needle)?;
                                if truthy(context, equal)? {
                                    return Ok(true);
                                }
                            }
                            Err(error) if context.consume_exception_type("IndexError") => {
                                return Ok(false);
                            }
                            Err(error) => return Err(error),
                        }
                    }
                    unreachable!("an unbounded sequence protocol must eventually raise IndexError")
                }
            }
        }
        None => Err("value contains a stale heap handle".to_owned()),
    }
}

fn normalize_hash_integer(value: &BigInt) -> i64 {
    // CPython's 64-bit integer hash is reduced modulo 2**61 - 1.  Keeping the
    // same reduction gives large native integers and large user __hash__
    // results a stable Py_hash_t-sized value instead of rejecting them.
    let modulus = BigInt::from((1_u64 << 61) - 1);
    let mut result = (value % modulus).to_i64().unwrap_or(0);
    if result == -1 {
        result = -2;
    }
    result
}

fn finish_native_hash(hash: u64) -> i64 {
    let result = hash as i64;
    if result == -1 { -2 } else { result }
}

fn hash_bytes_like(value: &[u8]) -> i64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    finish_native_hash(hasher.finish())
}

fn hash_text(value: &str) -> i64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    finish_native_hash(hasher.finish())
}

fn hash_float_value(value: f64, nan_identity: i64) -> i64 {
    const HASH_MODULUS: u128 = (1_u128 << 61) - 1;
    const HASH_INF: i64 = 314_159;

    if value == 0.0 {
        return 0;
    }
    if value.is_nan() {
        return if nan_identity == -1 { -2 } else { nan_identity };
    }
    if value == f64::INFINITY {
        return HASH_INF;
    }
    if value == f64::NEG_INFINITY {
        return -HASH_INF;
    }

    // Python's numeric hash is the exact rational value reduced modulo
    // 2**61-1. Every finite binary64 is significand * 2**exponent, so a
    // negative exponent can be reduced modulo 61 because 2**61 == 1 mod M.
    let bits = value.to_bits();
    let negative = bits >> 63 != 0;
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (significand, exponent) = if exponent_bits == 0 {
        (fraction, -1074)
    } else {
        ((1_u64 << 52) | fraction, exponent_bits - 1023 - 52)
    };
    let shift = exponent.rem_euclid(61) as u32;
    let magnitude = ((significand as u128) * (1_u128 << shift)) % HASH_MODULUS;
    let mut result = magnitude as i64;
    if negative {
        result = -result;
    }
    if result == -1 { -2 } else { result }
}

fn combine_hashes(kind: u8, hashes: &[i64]) -> i64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut hasher);
    hashes.len().hash(&mut hasher);
    hashes.iter().for_each(|hash| hash.hash(&mut hasher));
    finish_native_hash(hasher.finish())
}

pub(crate) fn hash_i64(context: &mut RimeraContext, value: RValue) -> Result<i64, String> {
    let value = hash(context, value)?;
    let value = integer(context, value)
        .map_err(|_| "__hash__ method should return an integer".to_owned())?;
    Ok(normalize_hash_integer(&value))
}

/// Computes the Python-visible hash for the currently supported object model.
pub fn hash(context: &mut RimeraContext, value: RValue) -> Result<RValue, String> {
    context.with_temporary_roots(&[value], |context| {
        match value.tag {
            tag if tag == RTag::Bool as u32 => {
                return Ok(RValue::small_int(value.payload as i64));
            }
            tag if tag == RTag::SmallInt as u32 => {
                return Ok(RValue::small_int(normalize_hash_integer(&BigInt::from(
                    value.payload.cast_signed(),
                ))));
            }
            tag if tag == RTag::None as u32 => {
                return Ok(RValue::small_int(hash_text("None")));
            }
            tag if tag != RTag::Handle as u32 => {
                return Err("value has an unknown ABI tag".to_owned());
            }
            _ => {}
        }

        if matches!(context.heap.get(value), Some(HeapObject::Instance(_))) {
            if let Some(method) = context.special_method(value, "__hash__")? {
                if method == RValue::NONE {
                    let type_value = context.type_of(value)?;
                    let type_name = match context.heap.get(type_value) {
                        Some(HeapObject::Type(type_object)) => type_object.name.clone(),
                        _ => "object".to_owned(),
                    };
                    return Err(format!("unhashable type: '{type_name}'"));
                }
                let result = context.with_temporary_roots(&[value, method], |context| {
                    crate::call::invoke(context, method, &[], &[])
                })?;
                let result = integer(context, result)
                    .map_err(|_| "__hash__ method should return an integer".to_owned())?;
                return Ok(RValue::small_int(normalize_hash_integer(&result)));
            }
            let mut identity = value.payload as i64;
            if identity == -1 {
                identity = -2;
            }
            return Ok(RValue::small_int(identity));
        }

        enum HashTarget {
            Direct(i64),
            Tuple(Vec<RValue>),
            FrozenSet(Vec<RValue>),
            Slice(crate::object::SliceObject),
            Range(RangeObject),
            MemoryView(MemoryViewObject),
            Identity,
        }

        let target = match context.heap.get(value) {
            Some(HeapObject::NotImplemented) => HashTarget::Identity,
            Some(HeapObject::Float(number)) => {
                HashTarget::Direct(hash_float_value(*number, value.payload as i64))
            }
            Some(HeapObject::Complex { real, imag }) => {
                let identity = value.payload as i64;
                let real = hash_float_value(*real, identity);
                let imag = hash_float_value(*imag, identity);
                let mut result = real.wrapping_add(1_000_003_i64.wrapping_mul(imag));
                if result == -1 {
                    result = -2;
                }
                HashTarget::Direct(result)
            }
            Some(HeapObject::BigInt(number)) => HashTarget::Direct(normalize_hash_integer(number)),
            Some(HeapObject::String(text)) => HashTarget::Direct(hash_text(text)),
            Some(HeapObject::Bytes(bytes)) => HashTarget::Direct(hash_bytes_like(bytes)),
            Some(HeapObject::Tuple(values)) => HashTarget::Tuple(values.to_vec()),
            Some(HeapObject::FrozenSet(values)) => {
                HashTarget::FrozenSet(values.table.values().copied().collect())
            }
            Some(HeapObject::Slice(slice)) => HashTarget::Slice(slice.clone()),
            Some(HeapObject::Range(range)) => HashTarget::Range(range.clone()),
            Some(HeapObject::MemoryView(view)) if !view.readonly => {
                return context.raise_error("ValueError", "cannot hash writable memoryview object");
            }
            Some(HeapObject::MemoryView(view)) => {
                if !matches!(buffer_format_code(&view.format), Ok('B' | 'b' | 'c')) {
                    return context.raise_error(
                        "ValueError",
                        "memoryview: hashing is restricted to formats 'B', 'b' or 'c'",
                    );
                }
                let exporter = memoryview_ultimate_exporter(context, view.exporter)?;
                if matches!(context.heap.get(exporter), Some(HeapObject::ByteArray(_))) {
                    return context.raise_error("TypeError", "unhashable type: 'bytearray'");
                }
                HashTarget::MemoryView(view.clone())
            }
            Some(HeapObject::ByteArray(_)) => {
                return context.raise_error("TypeError", "unhashable type: 'bytearray'");
            }
            Some(HeapObject::List(_)) => {
                return context.raise_error("TypeError", "unhashable type: 'list'");
            }
            Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_)) => {
                return context.raise_error("TypeError", "unhashable type: 'dict'");
            }
            Some(HeapObject::Set(_)) => {
                return context.raise_error("TypeError", "unhashable type: 'set'");
            }
            Some(HeapObject::DictionaryView(_)) => {
                return context.raise_error("TypeError", "unhashable type: 'dict view'");
            }
            Some(HeapObject::ValueArray(_)) => {
                return Err("managed value has no exposed Python hash".to_owned());
            }
            Some(_) => HashTarget::Identity,
            None => return Err("value contains a stale heap handle".to_owned()),
        };

        let result = match target {
            HashTarget::Direct(result) => result,
            HashTarget::Tuple(values) => {
                let mut hashes = Vec::with_capacity(values.len());
                for value in values {
                    hashes.push(hash_i64(context, value)?);
                }
                combine_hashes(1, &hashes)
            }
            HashTarget::FrozenSet(values) => {
                let mut hashes = Vec::with_capacity(values.len());
                for value in values {
                    hashes.push(hash_i64(context, value)?);
                }
                hashes.sort_unstable();
                combine_hashes(2, &hashes)
            }
            HashTarget::Slice(slice) => {
                let values = [
                    slice.start.unwrap_or(RValue::NONE),
                    slice.stop.unwrap_or(RValue::NONE),
                    slice.step.unwrap_or(RValue::NONE),
                ];
                let mut hashes = Vec::with_capacity(3);
                for value in values {
                    hashes.push(hash_i64(context, value)?);
                }
                combine_hashes(3, &hashes)
            }
            HashTarget::Range(range) => {
                let length = range_length_value(&range);
                let mut hashes = vec![normalize_hash_integer(&length)];
                if !length.is_zero() {
                    hashes.push(normalize_hash_integer(&range.start));
                    if length != BigInt::from(1_u8) {
                        hashes.push(normalize_hash_integer(&range.step));
                    }
                }
                combine_hashes(4, &hashes)
            }
            HashTarget::MemoryView(view) => {
                let bytes = memoryview_bytes(context, &view)?;
                hash_bytes_like(&bytes)
            }
            HashTarget::Identity => {
                let mut result = value.payload as i64;
                if result == -1 {
                    result = -2;
                }
                result
            }
        };
        Ok(RValue::small_int(result))
    })
}

fn repr_text(
    context: &mut RimeraContext,
    value: RValue,
    active: &mut Vec<RValue>,
) -> Result<String, String> {
    if let Some(result) = context.invoke_special_method(value, "__repr__", &[])? {
        return match context.heap.get(result) {
            Some(HeapObject::String(text)) => Ok(text.clone()),
            _ => Err("__repr__ returned non-string".to_owned()),
        };
    }
    match value.tag {
        tag if tag == RTag::None as u32 => return Ok("None".to_owned()),
        tag if tag == RTag::Bool as u32 => {
            return Ok(if value.payload == 0 { "False" } else { "True" }.to_owned());
        }
        tag if tag == RTag::SmallInt as u32 => {
            return Ok(value.payload.cast_signed().to_string());
        }
        tag if tag != RTag::Handle as u32 => {
            return Err("value has an unknown ABI tag".to_owned());
        }
        _ => {}
    }
    let object = context
        .heap
        .get(value)
        .cloned()
        .ok_or_else(|| "value contains a stale heap handle".to_owned())?;
    let recursive = matches!(
        object,
        HeapObject::Tuple(_)
            | HeapObject::List(_)
            | HeapObject::Dictionary(_)
            | HeapObject::ValueDictionary(_)
            | HeapObject::DictionaryView(_)
            | HeapObject::MappingProxy(_)
    );
    if recursive && active.contains(&value) {
        return Ok(match object {
            HeapObject::Tuple(_) => "(...)".to_owned(),
            HeapObject::List(_) => "[...]".to_owned(),
            HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_) => "{...}".to_owned(),
            HeapObject::DictionaryView(_) => "...".to_owned(),
            HeapObject::MappingProxy(_) => "mappingproxy({...})".to_owned(),
            _ => unreachable!(),
        });
    }
    if recursive {
        active.push(value);
    }
    let rendered = match object {
        HeapObject::NotImplemented => Ok("NotImplemented".to_owned()),
        HeapObject::Ellipsis => Ok("Ellipsis".to_owned()),
        HeapObject::Float(value) => Ok(python_float_repr(value)),
        HeapObject::Complex { real, imag } => Ok(python_complex_repr(real, imag)),
        HeapObject::BigInt(value) => Ok(value.to_string()),
        HeapObject::String(value) => Ok(python_string_repr(&value)),
        HeapObject::Bytes(value) => Ok(python_bytes_repr(&value)),
        HeapObject::ByteArray(value) => {
            Ok(format!("bytearray({})", python_bytes_repr(&value.bytes)))
        }
        HeapObject::Slice(slice) => {
            let start = repr_text(context, slice.start.unwrap_or(RValue::NONE), active)?;
            let stop = repr_text(context, slice.stop.unwrap_or(RValue::NONE), active)?;
            let step = repr_text(context, slice.step.unwrap_or(RValue::NONE), active)?;
            Ok(format!("slice({start}, {stop}, {step})"))
        }
        HeapObject::Tuple(values) => {
            let mut items = Vec::with_capacity(values.len());
            for item in values.iter().copied() {
                items.push(repr_text(context, item, active)?);
            }
            let mut body = items.join(", ");
            if values.len() == 1 {
                body.push(',');
            }
            Ok(format!("({body})"))
        }
        HeapObject::List(values) => {
            let mut items = Vec::with_capacity(values.len());
            for item in values {
                items.push(repr_text(context, item, active)?);
            }
            Ok(format!("[{}]", items.join(", ")))
        }
        HeapObject::Dictionary(dictionary) => {
            let mut items = Vec::with_capacity(dictionary.entries.len());
            for (key, item) in dictionary.entries {
                items.push(format!(
                    "{}: {}",
                    python_string_repr(&key),
                    repr_text(context, item, active)?
                ));
            }
            Ok(format!("{{{}}}", items.join(", ")))
        }
        HeapObject::ValueDictionary(dictionary) => {
            let entries = dictionary
                .table
                .values()
                .map(|(key, value)| (*key, *value))
                .collect::<Vec<_>>();
            let mut items = Vec::with_capacity(entries.len());
            for (key, item) in entries {
                items.push(format!(
                    "{}: {}",
                    repr_text(context, key, active)?,
                    repr_text(context, item, active)?
                ));
            }
            Ok(format!("{{{}}}", items.join(", ")))
        }
        HeapObject::Set(set) => {
            if set.table.is_empty() {
                Ok("set()".to_owned())
            } else {
                let values = set.table.values().copied().collect::<Vec<_>>();
                let mut items = Vec::with_capacity(values.len());
                for item in values {
                    items.push(repr_text(context, item, active)?);
                }
                Ok(format!("{{{}}}", items.join(", ")))
            }
        }
        HeapObject::FrozenSet(set) => {
            if set.table.is_empty() {
                Ok("frozenset()".to_owned())
            } else {
                let values = set.table.values().copied().collect::<Vec<_>>();
                let mut items = Vec::with_capacity(values.len());
                for item in values {
                    items.push(repr_text(context, item, active)?);
                }
                Ok(format!("frozenset({{{}}})", items.join(", ")))
            }
        }
        HeapObject::MappingProxy(proxy) => {
            let dictionary = repr_text(context, proxy.dictionary, active)?;
            Ok(format!("mappingproxy({dictionary})"))
        }
        HeapObject::DictionaryView(view) => {
            let entries = match context.heap.get(view.dictionary).cloned() {
                Some(HeapObject::Dictionary(dictionary)) => dictionary
                    .entries
                    .into_iter()
                    .map(|(key, value)| {
                        Ok((python_string_repr(&key), repr_text(context, value, active)?))
                    })
                    .collect::<Result<Vec<_>, String>>()?,
                Some(HeapObject::ValueDictionary(dictionary)) => {
                    let entries = dictionary
                        .table
                        .values()
                        .map(|(key, value)| (*key, *value))
                        .collect::<Vec<_>>();
                    let mut rendered = Vec::with_capacity(entries.len());
                    for (key, item) in entries {
                        rendered.push((
                            repr_text(context, key, active)?,
                            repr_text(context, item, active)?,
                        ));
                    }
                    rendered
                }
                _ => return Err("dictionary view source is invalid".to_owned()),
            };
            let body = match view.kind {
                DictionaryViewKind::Keys => entries
                    .iter()
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                DictionaryViewKind::Values => entries
                    .iter()
                    .map(|(_, item)| item.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                DictionaryViewKind::Items => entries
                    .iter()
                    .map(|(key, item)| format!("({key}, {item})"))
                    .collect::<Vec<_>>()
                    .join(", "),
            };
            let name = match view.kind {
                DictionaryViewKind::Keys => "dict_keys",
                DictionaryViewKind::Values => "dict_values",
                DictionaryViewKind::Items => "dict_items",
            };
            Ok(format!("{name}([{body}])"))
        }
        HeapObject::MemoryView(_) => Ok(format!("<memory at 0x{:x}>", value.payload)),
        HeapObject::Range(range) => {
            if range.step == BigInt::from(1_u8) {
                Ok(format!("range({}, {})", range.start, range.stop))
            } else {
                Ok(format!(
                    "range({}, {}, {})",
                    range.start, range.stop, range.step
                ))
            }
        }
        HeapObject::Instance(instance) if instance.storage.is_some() => repr_text(
            context,
            instance.storage.expect("storage was checked"),
            active,
        ),
        other => {
            let _ = other;
            display(context, value)
        }
    };
    if recursive {
        active.pop();
    }
    rendered
}

pub fn stringify(context: &mut RimeraContext, value: RValue) -> Result<String, String> {
    context.with_temporary_roots(&[value], |context| {
        if let Some(HeapObject::String(text)) = context.heap.get(value) {
            return Ok(text.clone());
        }
        if let Some(result) = context.invoke_special_method(value, "__str__", &[])? {
            return match context.heap.get(result) {
                Some(HeapObject::String(text)) => Ok(text.clone()),
                _ => Err("__str__ returned non-string".to_owned()),
            };
        }
        repr_text(context, value, &mut Vec::new())
    })
}

pub fn repr(context: &mut RimeraContext, value: RValue) -> Result<RValue, String> {
    context.with_temporary_roots(&[value], |context| {
        let rendered = repr_text(context, value, &mut Vec::new())?;
        string(context, &rendered)
    })
}

#[derive(Debug, Clone)]
struct ParsedFormatSpec {
    fill: char,
    align: Option<char>,
    sign: Option<char>,
    negative_zero: bool,
    alternate: bool,
    zero: bool,
    width: Option<usize>,
    grouping: Option<char>,
    precision: Option<usize>,
    ty: Option<char>,
}

fn parse_format_spec(spec: &str) -> Result<ParsedFormatSpec, String> {
    let chars = spec.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut fill = ' ';
    let mut align = None;
    if chars.len() >= 2 && matches!(chars[1], '<' | '>' | '=' | '^') {
        fill = chars[0];
        align = Some(chars[1]);
        index = 2;
    } else if chars
        .first()
        .is_some_and(|value| matches!(value, '<' | '>' | '=' | '^'))
    {
        align = Some(chars[0]);
        index = 1;
    }
    let sign = chars
        .get(index)
        .copied()
        .filter(|value| matches!(value, '+' | '-' | ' '));
    if sign.is_some() {
        index += 1;
    }
    let negative_zero = chars.get(index) == Some(&'z');
    if negative_zero {
        index += 1;
    }
    let alternate = chars.get(index) == Some(&'#');
    if alternate {
        index += 1;
    }
    let zero = chars.get(index) == Some(&'0');
    if zero {
        index += 1;
    }
    let width_start = index;
    while chars.get(index).is_some_and(|value| value.is_ascii_digit()) {
        index += 1;
    }
    let width = (index > width_start)
        .then(|| {
            chars[width_start..index]
                .iter()
                .collect::<String>()
                .parse::<usize>()
        })
        .transpose()
        .map_err(|_| "invalid format width".to_owned())?;
    let grouping = chars
        .get(index)
        .copied()
        .filter(|value| matches!(value, ',' | '_'));
    if grouping.is_some() {
        index += 1;
    }
    let precision = if chars.get(index) == Some(&'.') {
        index += 1;
        let start = index;
        while chars.get(index).is_some_and(|value| value.is_ascii_digit()) {
            index += 1;
        }
        if start == index {
            return Err("format specifier missing precision".to_owned());
        }
        Some(
            chars[start..index]
                .iter()
                .collect::<String>()
                .parse::<usize>()
                .map_err(|_| "invalid format precision".to_owned())?,
        )
    } else {
        None
    };
    let ty = chars.get(index).copied();
    if ty.is_some() {
        index += 1;
    }
    if index != chars.len() {
        return Err("invalid format specifier".to_owned());
    }
    Ok(ParsedFormatSpec {
        fill,
        align,
        sign,
        negative_zero,
        alternate,
        zero,
        width,
        grouping,
        precision,
        ty,
    })
}

fn grouped_digits(digits: &str, separator: char, group: usize) -> String {
    if digits.len() <= group {
        return digits.to_owned();
    }
    let first = digits.len() % group;
    let mut parts = Vec::new();
    let mut index = 0;
    if first != 0 {
        parts.push(digits[..first].to_owned());
        index = first;
    }
    while index < digits.len() {
        parts.push(digits[index..index + group].to_owned());
        index += group;
    }
    parts.join(&separator.to_string())
}

fn apply_format_padding(
    prefix: &str,
    body: &str,
    spec: &ParsedFormatSpec,
    default_align: char,
) -> String {
    let width = spec.width.unwrap_or(0);
    let length = prefix.chars().count() + body.chars().count();
    if width <= length {
        return format!("{prefix}{body}");
    }
    let count = width - length;
    let mut fill = spec.fill;
    let mut align = spec.align.unwrap_or(default_align);
    if spec.zero && spec.align.is_none() {
        fill = '0';
        align = '=';
    }
    let padding = std::iter::repeat_n(fill, count).collect::<String>();
    match align {
        '<' => format!("{prefix}{body}{padding}"),
        '^' => {
            let left = count / 2;
            let right = count - left;
            format!(
                "{}{}{}{}",
                std::iter::repeat_n(fill, left).collect::<String>(),
                prefix,
                body,
                std::iter::repeat_n(fill, right).collect::<String>()
            )
        }
        '=' => format!("{prefix}{padding}{body}"),
        _ => format!("{padding}{prefix}{body}"),
    }
}

fn numeric_body_split(body: &str) -> usize {
    body.find(['.', 'e', 'E', '%']).unwrap_or(body.len())
}

fn is_radix_digit_text(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn group_numeric_body(body: &str, separator: char, group: usize) -> String {
    let split = numeric_body_split(body);
    let integer = &body[..split];
    if !is_radix_digit_text(integer) {
        return body.to_owned();
    }
    format!(
        "{}{}",
        grouped_digits(integer, separator, group),
        &body[split..]
    )
}

fn apply_numeric_padding(
    prefix: &str,
    body: &str,
    spec: &ParsedFormatSpec,
    default_align: char,
    group_size: usize,
) -> String {
    let Some(separator) = spec.grouping else {
        return apply_format_padding(prefix, body, spec, default_align);
    };
    let zero_equal =
        (spec.zero && spec.align.is_none()) || (spec.align == Some('=') && spec.fill == '0');
    if zero_equal {
        let split = numeric_body_split(body);
        let integer = &body[..split];
        if is_radix_digit_text(integer) {
            let suffix = &body[split..];
            let width = spec.width.unwrap_or(0);
            let mut digits = integer.to_owned();
            loop {
                let grouped = grouped_digits(&digits, separator, group_size);
                let length =
                    prefix.chars().count() + grouped.chars().count() + suffix.chars().count();
                if length >= width {
                    return format!("{prefix}{grouped}{suffix}");
                }
                digits.insert(0, '0');
            }
        }
    }
    let grouped = group_numeric_body(body, separator, group_size);
    apply_format_padding(prefix, &grouped, spec, default_align)
}

fn format_integer_value(value: &BigInt, spec: &ParsedFormatSpec) -> Result<String, String> {
    if spec.negative_zero {
        return Err(
            "Negative zero coercion (z) not allowed in integer format specifier".to_owned(),
        );
    }
    if spec.precision.is_some() {
        return Err("Precision not allowed in integer format specifier".to_owned());
    }
    let ty = spec.ty.unwrap_or('d');
    if ty == 'c' {
        if spec.sign.is_some() || spec.alternate || spec.grouping.is_some() {
            return Err("Invalid format specifier for integer".to_owned());
        }
        let codepoint = value
            .to_u32()
            .and_then(char::from_u32)
            .ok_or_else(|| "%c arg not in range(0x110000)".to_owned())?;
        return Ok(apply_format_padding("", &codepoint.to_string(), spec, '>'));
    }
    let (radix, prefix, uppercase, group_size) = match ty {
        'b' => (2, "0b", false, 4),
        'o' => (8, "0o", false, 4),
        'x' => (16, "0x", false, 4),
        'X' => (16, "0X", true, 4),
        'd' | 'n' => (10, "", false, 3),
        _ => {
            return Err(format!(
                "Unknown format code '{ty}' for object of type 'int'"
            ));
        }
    };
    if ty == 'n' && spec.grouping.is_some() {
        return Err(format!(
            "Cannot specify '{}' with 'n'.",
            spec.grouping.expect("checked")
        ));
    }
    if spec.grouping == Some(',') && radix != 10 {
        return Err(format!("Cannot specify ',' with '{ty}'"));
    }
    let negative = value.sign() == num_bigint::Sign::Minus;
    let magnitude = value.abs();
    let mut digits = magnitude.to_str_radix(radix);
    if uppercase {
        digits.make_ascii_uppercase();
    }
    let sign = if negative {
        "-"
    } else {
        match spec.sign {
            Some('+') => "+",
            Some(' ') => " ",
            _ => "",
        }
    };
    let base_prefix = if spec.alternate { prefix } else { "" };
    let combined_prefix = format!("{sign}{base_prefix}");
    Ok(apply_numeric_padding(
        &combined_prefix,
        &digits,
        spec,
        '>',
        group_size,
    ))
}

fn normalize_scientific_exponent(mut text: String, uppercase: bool) -> String {
    let marker = if uppercase { 'E' } else { 'e' };
    if let Some(position) = text.find(['e', 'E']) {
        let exponent = text[position + 1..].parse::<i32>().unwrap_or(0);
        text.truncate(position);
        return format!("{text}{marker}{exponent:+03}");
    }
    text
}

fn trim_general_fraction(mut text: String) -> String {
    let exponent = text
        .find(['e', 'E'])
        .map(|position| text.split_off(position));
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if let Some(exponent) = exponent {
        text.push_str(&exponent);
    }
    text
}

fn scientific_exponent(text: &str) -> i32 {
    text.find(['e', 'E'])
        .and_then(|position| text[position + 1..].parse::<i32>().ok())
        .unwrap_or(0)
}

fn format_general_float(
    value: f64,
    precision: usize,
    alternate: bool,
    uppercase: bool,
    upper_exclusive: i32,
) -> String {
    if !value.is_finite() {
        let mut text = python_float_repr(value);
        if uppercase {
            text.make_ascii_uppercase();
        }
        return text;
    }
    let precision = precision.max(1);
    let scientific = format!("{:.*e}", precision.saturating_sub(1), value);
    let exponent = scientific_exponent(&scientific);
    let mut text = if exponent < -4 || exponent >= upper_exclusive {
        normalize_scientific_exponent(scientific, uppercase)
    } else {
        let decimals = (precision as i32 - exponent - 1).max(0) as usize;
        format!("{:.*}", decimals, value)
    };
    if !alternate {
        text = trim_general_fraction(text);
    }
    if uppercase {
        text.make_ascii_uppercase();
    }
    text
}

fn force_decimal_point(text: &mut String) {
    if text.contains('.') || text.eq_ignore_ascii_case("inf") || text.eq_ignore_ascii_case("nan") {
        return;
    }
    if let Some(position) = text.find(['e', 'E']) {
        text.insert(position, '.');
    } else if let Some(position) = text.find('%') {
        text.insert(position, '.');
    } else {
        text.push('.');
    }
}

fn format_float_magnitude(value: f64, spec: &ParsedFormatSpec) -> Result<String, String> {
    let ty = spec.ty;
    if ty == Some('n') && spec.grouping.is_some() {
        return Err(format!(
            "Cannot specify '{}' with 'n'.",
            spec.grouping.expect("checked")
        ));
    }
    if !value.is_finite() {
        let mut body = if value.is_nan() { "nan" } else { "inf" }.to_owned();
        if matches!(ty, Some('F' | 'E' | 'G')) {
            body.make_ascii_uppercase();
        }
        if ty == Some('%') {
            body.push('%');
        }
        return Ok(body);
    }
    let precision = spec.precision.unwrap_or(6);
    let mut body = match ty {
        Some('f' | 'F') => format!("{:.*}", precision, value),
        Some('e' | 'E') => normalize_scientific_exponent(
            format!("{:.*e}", precision, value),
            matches!(ty, Some('E')),
        ),
        Some('g' | 'G' | 'n') => format_general_float(
            value,
            precision,
            spec.alternate,
            matches!(ty, Some('G')),
            precision.max(1) as i32,
        ),
        Some('%') => {
            let mut rendered = format!("{:.*}", precision, value);
            if spec.alternate && value.is_finite() {
                force_decimal_point(&mut rendered);
            }
            rendered.push('%');
            rendered
        }
        None => {
            if let Some(precision) = spec.precision {
                let effective = precision.max(1);
                let mut rendered = format_general_float(
                    value,
                    effective,
                    spec.alternate,
                    false,
                    effective as i32 - 1,
                );
                if value.is_finite() && !rendered.contains(['e', 'E']) && !rendered.contains('.') {
                    rendered.push_str(".0");
                }
                rendered
            } else {
                let mut rendered = python_float_repr(value);
                if spec.alternate && value.is_finite() {
                    force_decimal_point(&mut rendered);
                }
                rendered
            }
        }
        Some(other) => {
            return Err(format!(
                "Unknown format code '{other}' for object of type 'float'"
            ));
        }
    };
    if spec.alternate && value.is_finite() && matches!(ty, Some('f' | 'F' | 'e' | 'E')) {
        force_decimal_point(&mut body);
    }
    if matches!(ty, Some('F' | 'E' | 'G')) {
        body.make_ascii_uppercase();
    }
    Ok(body)
}

fn formatted_numeric_body_is_zero(body: &str) -> bool {
    let mantissa_end = body.find(['e', 'E', '%']).unwrap_or(body.len());
    let mantissa = &body[..mantissa_end];
    !mantissa.is_empty()
        && mantissa
            .chars()
            .all(|character| matches!(character, '0' | '.'))
}

fn format_float_value(value: f64, spec: &ParsedFormatSpec) -> Result<String, String> {
    let percent = spec.ty == Some('%');
    let number = if percent { value * 100.0 } else { value };
    let magnitude = number.abs();
    let body = format_float_magnitude(magnitude, spec)?;
    let rounded_zero = formatted_numeric_body_is_zero(&body);
    let mut negative = number.is_sign_negative() && !number.is_nan();
    if spec.negative_zero && rounded_zero {
        negative = false;
    }
    let sign = if negative {
        "-"
    } else {
        match spec.sign {
            Some('+') => "+",
            Some(' ') => " ",
            _ => "",
        }
    };
    Ok(apply_numeric_padding(sign, &body, spec, '>', 3))
}

fn complex_component(value: f64, spec: &ParsedFormatSpec) -> Result<(String, bool), String> {
    let mut component_spec = spec.clone();
    component_spec.fill = ' ';
    component_spec.align = None;
    component_spec.sign = None;
    component_spec.zero = false;
    component_spec.width = None;
    let mut body = format_float_magnitude(value.abs(), &component_spec)?;
    if let Some(separator) = spec.grouping {
        body = group_numeric_body(&body, separator, 3);
    }
    if spec.ty.is_none() && value.is_finite() {
        if spec.alternate && spec.precision.is_none() && body.ends_with(".0") {
            body.truncate(body.len() - 1);
        } else if !spec.alternate && !body.contains(['e', 'E']) && body.ends_with(".0") {
            body.truncate(body.len() - 2);
        }
    }
    let rounded_zero = formatted_numeric_body_is_zero(&body.replace(['_', ','], ""));
    let mut negative = value.is_sign_negative() && !value.is_nan();
    if spec.negative_zero && rounded_zero {
        negative = false;
    }
    Ok((body, negative))
}

fn format_complex_value(real: f64, imag: f64, spec: &ParsedFormatSpec) -> Result<String, String> {
    if spec.align == Some('=') {
        return Err("'=' alignment flag is not allowed in complex format specifier".to_owned());
    }
    if spec.zero {
        return Err("Zero padding is not allowed in complex format specifier".to_owned());
    }
    if !matches!(
        spec.ty,
        None | Some('e' | 'E' | 'f' | 'F' | 'g' | 'G' | 'n')
    ) {
        return Err(format!(
            "Unknown format code '{}' for object of type 'complex'",
            spec.ty.expect("checked")
        ));
    }
    if spec.ty == Some('n') && spec.grouping.is_some() {
        return Err(format!(
            "Cannot specify '{}' with 'n'.",
            spec.grouping.expect("checked")
        ));
    }
    let (real_body, real_negative) = complex_component(real, spec)?;
    let (imag_body, imag_negative) = complex_component(imag, spec)?;
    let omit_real = spec.ty.is_none() && real == 0.0 && !real.is_sign_negative();
    let body = if omit_real {
        let sign = if imag_negative {
            "-"
        } else {
            match spec.sign {
                Some('+') => "+",
                Some(' ') => " ",
                _ => "",
            }
        };
        format!("{sign}{imag_body}j")
    } else {
        let real_sign = if real_negative {
            "-"
        } else {
            match spec.sign {
                Some('+') => "+",
                Some(' ') => " ",
                _ => "",
            }
        };
        let imag_sign = if imag_negative { '-' } else { '+' };
        let inner = format!("{real_sign}{real_body}{imag_sign}{imag_body}j");
        if spec.ty.is_none() {
            format!("({inner})")
        } else {
            inner
        }
    };
    Ok(apply_format_padding("", &body, spec, '>'))
}

fn format_string_value(value: &str, spec: &ParsedFormatSpec) -> Result<String, String> {
    if spec.align == Some('=') {
        return Err("'=' alignment not allowed in string format specifier".to_owned());
    }
    if spec.sign.is_some() {
        return Err("Sign not allowed in string format specifier".to_owned());
    }
    if spec.negative_zero {
        return Err("Negative zero coercion (z) not allowed in string format specifier".to_owned());
    }
    if spec.alternate {
        return Err("Alternate form (#) not allowed in string format specifier".to_owned());
    }
    if let Some(grouping) = spec.grouping {
        return Err(format!("Cannot specify '{grouping}' with 's'."));
    }
    if !matches!(spec.ty, None | Some('s')) {
        return Err(format!(
            "Unknown format code '{}' for object of type 'str'",
            spec.ty.expect("checked")
        ));
    }
    let body = spec.precision.map_or_else(
        || value.to_owned(),
        |precision| value.chars().take(precision).collect::<String>(),
    );
    let mut string_spec = spec.clone();
    if string_spec.zero && string_spec.fill == ' ' {
        string_spec.fill = '0';
    }
    string_spec.zero = false;
    Ok(apply_format_padding("", &body, &string_spec, '<'))
}

pub fn format(context: &mut RimeraContext, value: RValue, spec: RValue) -> Result<RValue, String> {
    context.with_temporary_roots(&[value, spec], |context| {
        let spec_text = match context.heap.get(spec) {
            Some(HeapObject::String(value)) => value.clone(),
            _ => return context.raise_error("TypeError", "format() argument 2 must be str"),
        };
        if let Some(result) = context.invoke_special_method(value, "__format__", &[spec])? {
            if matches!(context.heap.get(result), Some(HeapObject::String(_))) {
                return Ok(result);
            }
            return context
                .raise_error("TypeError", "__format__ must return a str, not non-string");
        }
        if let Some(storage) = instance_storage(context, value) {
            return format(context, storage, spec);
        }
        let parsed = match parse_format_spec(&spec_text) {
            Ok(parsed) => parsed,
            Err(message) => return context.raise_error("ValueError", message),
        };
        let formatted = if value.tag == RTag::Bool as u32 {
            if spec_text.is_empty() {
                let rendered = stringify(context, value)?;
                return string(context, &rendered);
            }
            format_integer_value(&BigInt::from(value.payload != 0), &parsed)
        } else if value.tag == RTag::SmallInt as u32 {
            format_integer_value(&BigInt::from(value.payload.cast_signed()), &parsed)
        } else {
            match context.heap.get(value).cloned() {
                Some(HeapObject::BigInt(value)) => format_integer_value(&value, &parsed),
                Some(HeapObject::Float(value)) => format_float_value(value, &parsed),
                Some(HeapObject::Complex { real, imag }) => {
                    format_complex_value(real, imag, &parsed)
                }
                Some(HeapObject::String(value)) => format_string_value(&value, &parsed),
                _ if spec_text.is_empty() => {
                    let rendered = stringify(context, value)?;
                    return string(context, &rendered);
                }
                _ => {
                    let type_name = context.type_of(value).ok().map_or_else(
                        || "object".to_owned(),
                        |ty| match context.heap.get(ty) {
                            Some(HeapObject::Type(object)) => object.name.clone(),
                            _ => "object".to_owned(),
                        },
                    );
                    return context.raise_error(
                        "TypeError",
                        format!("unsupported format string passed to {type_name}.__format__"),
                    );
                }
            }
        };
        let rendered = match formatted {
            Ok(rendered) => rendered,
            Err(message) => return context.raise_error("ValueError", message),
        };
        string(context, &rendered)
    })
}

pub fn truthy(context: &mut RimeraContext, value: RValue) -> Result<bool, String> {
    if value.tag == RTag::Handle as u32
        && matches!(context.heap.get(value), Some(HeapObject::Instance(_)))
    {
        if let Some(result) = context.invoke_special_method(value, "__bool__", &[])? {
            if result.tag != RTag::Bool as u32 {
                return Err("__bool__ should return bool".to_owned());
            }
            return Ok(result.payload != 0);
        }
        if let Some(result) = context.invoke_special_method(value, "__len__", &[])? {
            let length = integer(context, result)?;
            if length.sign() == num_bigint::Sign::Minus {
                return Err("__len__() should return >= 0".to_owned());
            }
            return Ok(!length.is_zero());
        }
        if let Some(storage) = instance_storage(context, value) {
            return truthy(context, storage);
        }
    }
    if let Some(HeapObject::DictionaryView(view)) = context.heap.get(value) {
        let dictionary = view.dictionary;
        let length = length(context, dictionary)?;
        return Ok(!integer(context, length)?.is_zero());
    }
    if let Some(HeapObject::MemoryView(view)) = context.heap.get(value) {
        if view.released {
            return Err("operation forbidden on released memoryview object".to_owned());
        }
        return view
            .shape
            .first()
            .copied()
            .map(|length| length != 0)
            .ok_or_else(|| "0-dim memory has no length".to_owned());
    }
    if let Some(HeapObject::Range(range)) = context.heap.get(value) {
        return Ok(!range_length_value(range).is_zero());
    }
    match value.tag {
        tag if tag == RTag::None as u32 => Ok(false),
        tag if tag == RTag::Bool as u32 => Ok(value.payload != 0),
        tag if tag == RTag::SmallInt as u32 => Ok(value.payload.cast_signed() != 0),
        tag if tag == RTag::Handle as u32 => match context.heap.get(value) {
            Some(HeapObject::Float(value)) => Ok(*value != 0.0),
            Some(HeapObject::Complex { real, imag }) => Ok(*real != 0.0 || *imag != 0.0),
            Some(HeapObject::BigInt(value)) => Ok(!value.is_zero()),
            Some(HeapObject::String(value)) => Ok(!value.is_empty()),
            Some(HeapObject::Bytes(value)) => Ok(!value.is_empty()),
            Some(HeapObject::ByteArray(value)) => Ok(!value.bytes.is_empty()),
            Some(HeapObject::Tuple(values)) => Ok(!values.is_empty()),
            Some(HeapObject::List(values)) => Ok(!values.is_empty()),
            Some(HeapObject::Dictionary(dictionary)) => Ok(!dictionary.entries.is_empty()),
            Some(HeapObject::ValueDictionary(dictionary)) => Ok(!dictionary.table.is_empty()),
            Some(HeapObject::Set(set)) => Ok(!set.table.is_empty()),
            Some(HeapObject::FrozenSet(set)) => Ok(!set.table.is_empty()),
            Some(HeapObject::DictionaryView(_)) => Ok(true),
            Some(HeapObject::MappingProxy(proxy)) => truthy(context, proxy.dictionary),
            Some(HeapObject::MemoryView(value)) => Ok(!value.released && !value.shape.contains(&0)),
            Some(HeapObject::Slice(_)) => Ok(true),
            Some(
                HeapObject::NotImplemented
                | HeapObject::Ellipsis
                | HeapObject::Type(_)
                | HeapObject::Instance(_)
                | HeapObject::BoundMethod(_)
                | HeapObject::Super(_)
                | HeapObject::Property(_)
                | HeapObject::PropertyMethod(_)
                | HeapObject::StaticMethod(_)
                | HeapObject::ClassMethod(_)
                | HeapObject::MemberDescriptor(_)
                | HeapObject::Function(_)
                | HeapObject::Generator(_)
                | HeapObject::BuiltinFunction(_)
                | HeapObject::Cell(_)
                | HeapObject::Exception(_)
                | HeapObject::Traceback(_)
                | HeapObject::Range(_)
                | HeapObject::Iterator(_),
            ) => Ok(true),
            Some(HeapObject::ValueArray(_)) => {
                Err("managed value has no exposed Python type".to_owned())
            }
            None => Err("value contains a stale heap handle".to_owned()),
        },
        _ => Err("value has an unknown ABI tag".to_owned()),
    }
}

fn fixed_from_shortest_scientific(coefficient: &str, exponent: i32, force_decimal: bool) -> String {
    let digits = coefficient
        .chars()
        .filter(|character| *character != '.')
        .collect::<String>();
    let decimal_position = exponent + 1;
    let mut rendered = if decimal_position <= 0 {
        format!("0.{}{}", "0".repeat((-decimal_position) as usize), digits)
    } else if decimal_position as usize >= digits.len() {
        let mut rendered = digits;
        rendered.push_str(&"0".repeat(decimal_position as usize - rendered.len()));
        rendered
    } else {
        let position = decimal_position as usize;
        format!("{}.{}", &digits[..position], &digits[position..])
    };
    if force_decimal && !rendered.contains('.') {
        rendered.push_str(".0");
    }
    rendered
}

fn python_float_repr(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value == f64::INFINITY {
        return "inf".to_owned();
    }
    if value == f64::NEG_INFINITY {
        return "-inf".to_owned();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    let negative = value.is_sign_negative();
    let scientific = format!("{:e}", value.abs());
    let (coefficient, exponent) = scientific
        .split_once('e')
        .expect("Rust lower-exp float formatting always includes an exponent");
    let exponent = exponent
        .parse::<i32>()
        .expect("Rust emitted a valid float exponent");
    let body = if (-4..16).contains(&exponent) {
        fixed_from_shortest_scientific(coefficient, exponent, true)
    } else {
        format!("{coefficient}e{exponent:+03}")
    };
    if negative { format!("-{body}") } else { body }
}

fn python_complex_component(value: f64) -> String {
    let mut rendered = python_float_repr(value);
    if value.is_finite() && !rendered.contains(['e', 'E']) && rendered.ends_with(".0") {
        rendered.truncate(rendered.len() - 2);
    }
    rendered
}

fn python_complex_repr(real: f64, imag: f64) -> String {
    if real == 0.0 && !real.is_sign_negative() {
        return format!("{}j", python_complex_component(imag));
    }
    let sign = if imag.is_sign_negative() { '-' } else { '+' };
    format!(
        "({}{}{}j)",
        python_complex_component(real),
        sign,
        python_complex_component(imag.abs())
    )
}

fn slice_repr(
    context: &RimeraContext,
    slice: &crate::object::SliceObject,
) -> Result<String, String> {
    let start = collection_item(context, slice.start.unwrap_or(RValue::NONE))?;
    let stop = collection_item(context, slice.stop.unwrap_or(RValue::NONE))?;
    let step = collection_item(context, slice.step.unwrap_or(RValue::NONE))?;
    Ok(format!("slice({start}, {stop}, {step})"))
}

fn dictionary_view_repr(
    context: &RimeraContext,
    view: &DictionaryViewObject,
) -> Result<String, String> {
    let entries = match context.heap.get(view.dictionary) {
        Some(HeapObject::Dictionary(dictionary)) => dictionary
            .entries
            .iter()
            .map(|(key, value)| Ok((python_string_repr(key), collection_item(context, *value)?)))
            .collect::<Result<Vec<_>, String>>()?,
        Some(HeapObject::ValueDictionary(dictionary)) => dictionary
            .table
            .values()
            .map(|(key, value)| {
                Ok((
                    collection_item(context, *key)?,
                    collection_item(context, *value)?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?,
        _ => return Err("dictionary view source is invalid".to_owned()),
    };
    let body = match view.kind {
        DictionaryViewKind::Keys => entries
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>()
            .join(", "),
        DictionaryViewKind::Values => entries
            .iter()
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>()
            .join(", "),
        DictionaryViewKind::Items => entries
            .iter()
            .map(|(key, value)| format!("({key}, {value})"))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let name = match view.kind {
        DictionaryViewKind::Keys => "dict_keys",
        DictionaryViewKind::Values => "dict_values",
        DictionaryViewKind::Items => "dict_items",
    };
    Ok(format!("{name}([{body}])"))
}

pub fn display(context: &RimeraContext, value: RValue) -> Result<String, String> {
    match value.tag {
        tag if tag == RTag::None as u32 => Ok("None".to_owned()),
        tag if tag == RTag::Bool as u32 => {
            Ok(if value.payload == 0 { "False" } else { "True" }.to_owned())
        }
        tag if tag == RTag::SmallInt as u32 => Ok(value.payload.cast_signed().to_string()),
        tag if tag == RTag::Handle as u32 => match context.heap.get(value) {
            Some(HeapObject::NotImplemented) => Ok("NotImplemented".to_owned()),
            Some(HeapObject::Ellipsis) => Ok("Ellipsis".to_owned()),
            Some(HeapObject::Float(value)) => Ok(python_float_repr(*value)),
            Some(HeapObject::Complex { real, imag }) => Ok(python_complex_repr(*real, *imag)),
            Some(HeapObject::BigInt(value)) => Ok(value.to_string()),
            Some(HeapObject::String(value)) => Ok(value.clone()),
            Some(HeapObject::Bytes(value)) => Ok(python_bytes_repr(value)),
            Some(HeapObject::ByteArray(value)) => {
                Ok(format!("bytearray({})", python_bytes_repr(&value.bytes)))
            }
            Some(HeapObject::Slice(slice)) => slice_repr(context, slice),
            Some(HeapObject::FrozenSet(set)) => {
                if set.table.is_empty() {
                    Ok("frozenset()".to_owned())
                } else {
                    Ok(format!(
                        "frozenset({{{}}})",
                        set.table
                            .values()
                            .map(|value| collection_item(context, *value))
                            .collect::<Result<Vec<_>, _>>()?
                            .join(", ")
                    ))
                }
            }
            Some(HeapObject::DictionaryView(view)) => dictionary_view_repr(context, view),
            Some(HeapObject::MappingProxy(proxy)) => display(context, proxy.dictionary)
                .map(|dictionary| format!("mappingproxy({dictionary})")),
            Some(HeapObject::MemoryView(_)) => Ok(format!("<memory at 0x{:x}>", value.payload)),
            Some(HeapObject::Tuple(values)) => {
                let mut items = values
                    .iter()
                    .map(|value| collection_item(context, *value))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                if values.len() == 1 {
                    items.push(',');
                }
                Ok(format!("({items})"))
            }
            Some(HeapObject::List(values)) => Ok(format!(
                "[{}]",
                values
                    .iter()
                    .map(|value| collection_item(context, *value))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            )),
            Some(HeapObject::Dictionary(dictionary)) => Ok(format!(
                "{{{}}}",
                dictionary
                    .entries
                    .iter()
                    .map(|(name, value)| collection_item(context, *value)
                        .map(|value| format!("{}: {value}", python_string_repr(name))))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            )),
            Some(HeapObject::ValueDictionary(dictionary)) => Ok(format!(
                "{{{}}}",
                dictionary
                    .table
                    .values()
                    .map(|(key, value)| collection_item(context, *key)
                        .and_then(|key| collection_item(context, *value)
                            .map(|value| format!("{key}: {value}"))))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            )),
            Some(HeapObject::Set(set)) => {
                if set.table.is_empty() {
                    Ok("set()".to_owned())
                } else {
                    Ok(format!(
                        "{{{}}}",
                        set.table
                            .values()
                            .map(|value| collection_item(context, *value))
                            .collect::<Result<Vec<_>, _>>()?
                            .join(", ")
                    ))
                }
            }
            Some(HeapObject::Type(object)) => Ok(format!("<class '{}'>", object.qualified_name)),
            Some(HeapObject::Instance(object)) if object.storage.is_some() => {
                display(context, object.storage.expect("storage was checked"))
            }
            Some(HeapObject::Instance(object)) => match context.heap.get(object.class) {
                Some(HeapObject::Type(class)) => Ok(format!("<{} object>", class.qualified_name)),
                _ => Err("instance has an invalid class".to_owned()),
            },
            Some(HeapObject::BoundMethod(_)) => Ok("<bound method>".to_owned()),
            Some(HeapObject::Super(_)) => Ok("<super object>".to_owned()),
            Some(HeapObject::Property(_)) => Ok("<property object>".to_owned()),
            Some(HeapObject::PropertyMethod(_)) => Ok("<property method>".to_owned()),
            Some(HeapObject::StaticMethod(_)) => Ok("<staticmethod object>".to_owned()),
            Some(HeapObject::ClassMethod(_)) => Ok("<classmethod object>".to_owned()),
            Some(HeapObject::MemberDescriptor(_)) => Ok("<member descriptor>".to_owned()),
            Some(HeapObject::Function(object)) => {
                Ok(format!("<function {}>", object.qualified_name))
            }
            Some(HeapObject::Generator(_)) => Ok("<generator object>".to_owned()),
            Some(HeapObject::BuiltinFunction(object)) => {
                Ok(format!("<built-in function {}>", object.name))
            }
            Some(HeapObject::Cell(_)) => Ok("<cell>".to_owned()),
            Some(HeapObject::Exception(object)) => {
                if let Some(message) = &object.group_message {
                    Ok(message.clone())
                } else {
                    match context.heap.get(object.arguments) {
                        Some(HeapObject::Tuple(arguments)) if arguments.len() == 1 => {
                            display(context, arguments[0])
                        }
                        _ => display(context, object.arguments),
                    }
                }
            }
            Some(HeapObject::Traceback(_)) => Ok("<traceback object>".to_owned()),
            Some(HeapObject::Range(range)) => {
                if range.step == BigInt::from(1_u8) {
                    Ok(format!("range({}, {})", range.start, range.stop))
                } else {
                    Ok(format!(
                        "range({}, {}, {})",
                        range.start, range.stop, range.step
                    ))
                }
            }
            Some(HeapObject::Iterator(_)) => Ok("<iterator>".to_owned()),
            Some(HeapObject::ValueArray(_)) => {
                Err("managed value has no exposed Python representation".to_owned())
            }
            None => Err("value contains a stale heap handle".to_owned()),
        },
        _ => Err("value has an unknown ABI tag".to_owned()),
    }
}

pub(crate) fn integer(context: &RimeraContext, value: RValue) -> Result<BigInt, String> {
    if value.tag == RTag::Bool as u32 {
        Ok(BigInt::from(value.payload != 0))
    } else if value.tag == RTag::SmallInt as u32 {
        Ok(BigInt::from(value.payload.cast_signed()))
    } else {
        match context.heap.get(value) {
            Some(HeapObject::BigInt(value)) => Ok(value.clone()),
            _ => Err("operation requires integer operands".to_owned()),
        }
    }
}

pub(crate) fn index_integer(context: &mut RimeraContext, value: RValue) -> Result<BigInt, String> {
    if let Ok(value) = integer(context, value) {
        return Ok(value);
    }
    let type_name =
        context
            .type_of(value)
            .map(|type_value| match context.heap.get(type_value) {
                Some(HeapObject::Type(object)) => object.name.clone(),
                _ => "object".to_owned(),
            })?;
    let Some(result) = context.invoke_special_method(value, "__index__", &[])? else {
        return context.raise_error(
            "TypeError",
            format!("'{type_name}' object cannot be interpreted as an integer"),
        );
    };
    if let Ok(integer) = integer(context, result) {
        return Ok(integer);
    }
    let result_type = context
        .type_of(result)
        .ok()
        .and_then(|type_value| match context.heap.get(type_value) {
            Some(HeapObject::Type(object)) => Some(object.name.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "object".to_owned());
    context.raise_error(
        "TypeError",
        format!("__index__ returned non-int (type '{result_type}')"),
    )
}

fn numeric_float(context: &RimeraContext, value: RValue) -> Option<f64> {
    match value.tag {
        tag if tag == RTag::Bool as u32 => Some(if value.payload == 0 { 0.0 } else { 1.0 }),
        tag if tag == RTag::SmallInt as u32 => Some(value.payload.cast_signed() as f64),
        tag if tag == RTag::Handle as u32 => match context.heap.get(value) {
            Some(HeapObject::Float(value)) => Some(*value),
            Some(HeapObject::BigInt(value)) => value.to_f64(),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn float_family_value(context: &RimeraContext, value: RValue) -> Option<f64> {
    match context.heap.get(value) {
        Some(HeapObject::Float(value)) => Some(*value),
        Some(HeapObject::Instance(instance)) => instance.storage.and_then(|storage| match context
            .heap
            .get(storage)
        {
            Some(HeapObject::Float(value)) => Some(*value),
            _ => None,
        }),
        _ => None,
    }
}

pub(crate) fn complex_family_value(context: &RimeraContext, value: RValue) -> Option<(f64, f64)> {
    match context.heap.get(value) {
        Some(HeapObject::Complex { real, imag }) => Some((*real, *imag)),
        Some(HeapObject::Instance(instance)) => instance.storage.and_then(|storage| match context
            .heap
            .get(storage)
        {
            Some(HeapObject::Complex { real, imag }) => Some((*real, *imag)),
            _ => None,
        }),
        _ => None,
    }
}

pub fn numeric_float_for_constructor(context: &RimeraContext, value: RValue) -> Option<f64> {
    float_family_value(context, value).or_else(|| numeric_float(context, value))
}

fn numeric_complex(context: &RimeraContext, value: RValue) -> Option<(f64, f64)> {
    complex_family_value(context, value)
        .or_else(|| numeric_float(context, value).map(|real| (real, 0.0)))
}

pub(crate) fn string_value(context: &RimeraContext, value: RValue) -> Option<&str> {
    match context.heap.get(value) {
        Some(HeapObject::String(value)) => Some(value),
        _ => None,
    }
}

fn collection_item(context: &RimeraContext, value: RValue) -> Result<String, String> {
    match context.heap.get(value) {
        Some(HeapObject::String(value)) => Ok(python_string_repr(value)),
        _ => display(context, value),
    }
}

fn python_repr_escape_codepoint(character: char) -> Option<String> {
    use unicode_general_category::{GeneralCategory, get_general_category};

    let codepoint = character as u32;
    let non_printable = character != ' '
        && matches!(
            get_general_category(character),
            GeneralCategory::Control
                | GeneralCategory::Format
                | GeneralCategory::LineSeparator
                | GeneralCategory::ParagraphSeparator
                | GeneralCategory::PrivateUse
                | GeneralCategory::SpaceSeparator
                | GeneralCategory::Surrogate
                | GeneralCategory::Unassigned
        );
    if non_printable {
        return Some(if codepoint <= 0xff {
            format!("\\x{codepoint:02x}")
        } else if codepoint <= 0xffff {
            format!("\\u{codepoint:04x}")
        } else {
            format!("\\U{codepoint:08x}")
        });
    }
    None
}

fn python_string_repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut rendered = String::with_capacity(value.len() + 2);
    rendered.push(quote);
    for character in value.chars() {
        match character {
            '\\' => rendered.push_str("\\\\"),
            '\n' => rendered.push_str("\\n"),
            '\r' => rendered.push_str("\\r"),
            '\t' => rendered.push_str("\\t"),
            character if character == quote => {
                rendered.push('\\');
                rendered.push(character);
            }
            character => {
                if let Some(escaped) = python_repr_escape_codepoint(character) {
                    rendered.push_str(&escaped);
                } else {
                    rendered.push(character);
                }
            }
        }
    }
    rendered.push(quote);
    rendered
}

fn python_bytes_repr(value: &[u8]) -> String {
    let quote = if value.contains(&b'\'') && !value.contains(&b'"') {
        b'"'
    } else {
        b'\''
    };
    let mut rendered = String::with_capacity(value.len() + 3);
    rendered.push('b');
    rendered.push(char::from(quote));
    for byte in value {
        match *byte {
            b'\\' => rendered.push_str("\\\\"),
            b'\n' => rendered.push_str("\\n"),
            b'\r' => rendered.push_str("\\r"),
            b'\t' => rendered.push_str("\\t"),
            byte if byte == quote => {
                rendered.push('\\');
                rendered.push(char::from(byte));
            }
            0x20..=0x7e => rendered.push(char::from(*byte)),
            _ => rendered.push_str(&format!("\\x{byte:02x}")),
        }
    }
    rendered.push(char::from(quote));
    rendered
}

pub(crate) fn store_integer(context: &mut RimeraContext, value: BigInt) -> Result<RValue, String> {
    value.to_i64().map_or_else(
        || context.allocate(HeapObject::BigInt(value)),
        |value| Ok(RValue::small_int(value)),
    )
}

fn floor_division(left: &BigInt, right: &BigInt) -> Result<(BigInt, BigInt), String> {
    if right.is_zero() {
        return Err("integer division or modulo by zero".to_owned());
    }
    let mut quotient = left / right;
    let mut remainder = left % right;
    if !remainder.is_zero()
        && ((remainder.sign() == num_bigint::Sign::Minus)
            != (right.sign() == num_bigint::Sign::Minus))
    {
        quotient -= 1;
        remainder += right;
    }
    Ok((quotient, remainder))
}
