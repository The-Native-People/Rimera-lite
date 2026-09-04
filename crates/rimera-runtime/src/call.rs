use std::ffi::c_void;
use std::io::{self, Write};

use num_bigint::BigInt;
use num_traits::{Signed, ToPrimitive};
use rimera_abi::{
    RCallArgumentKind, RGeneratorDelegateOutcome, RGeneratorOperation, RGeneratorOutcome,
    RNativeFunction, RStatus, RValue,
};

use crate::ParameterKind;
use crate::RimeraContext;
use crate::heap::HeapObject;
use crate::object::{
    BuiltinFunctionKind, CallArgumentsObject, CodeObject, DictionaryObject, FunctionObject,
    TYPE_FLAG_BUILTIN, TypeLayout,
};

pub(crate) fn invoke(
    context: &mut RimeraContext,
    callable: RValue,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if let Some(HeapObject::Type(object)) = context.heap.get(callable) {
        let type_name = object.name.clone();
        let flags = object.flags;
        if type_name == "type" {
            return invoke_type(context, positional, keywords);
        }
        if matches!(
            type_name.as_str(),
            "bool"
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
                | "memoryview"
                | "slice"
                | "range"
        ) {
            return invoke_builtin_constructor(context, &type_name, positional, keywords);
        }
        let type_type = context.builtin_type("type");
        if type_type.is_some_and(|type_type| object.mro.contains(&type_type))
            && positional.len() == 3
        {
            return invoke_metaclass(context, callable, positional, keywords);
        }
        if context.builtin_type("super") == Some(callable) {
            if !keywords.is_empty() {
                return Err("super() takes no keyword arguments".to_owned());
            }
            return match positional {
                [start_type, receiver] => context.new_super(*start_type, *receiver),
                [] => context.zero_argument_super(),
                _ => Err(format!(
                    "super() takes at most 2 arguments ({} given)",
                    positional.len()
                )),
            };
        }
        let base_exception = context.builtin_type("BaseException");
        if base_exception.is_some_and(|base| object.mro.contains(&base)) {
            if matches!(type_name.as_str(), "ExceptionGroup" | "BaseExceptionGroup") {
                if positional.len() != 2 {
                    return Err(format!("{type_name}() requires a message and exceptions"));
                }
                let children = match context.heap.get(positional[1]) {
                    Some(HeapObject::Tuple(values)) | Some(HeapObject::ValueArray(values)) => {
                        values.to_vec()
                    }
                    _ => return Err("exception group children must be a sequence".to_owned()),
                };
                return context.new_exception_group(callable, positional[0], &children);
            }
            let instance = context.new_exception(callable, positional)?;
            return context.with_temporary_roots(&[instance, callable], |context| {
                if let Some(initializer) = context.special_method(instance, "__init__")? {
                    let mut roots = positional.to_vec();
                    roots.extend(keywords.iter().map(|(_, value)| *value));
                    roots.extend([instance, initializer]);
                    return context.with_temporary_roots(&roots, |context| {
                        let result = invoke(context, initializer, positional, keywords)?;
                        if result != RValue::NONE {
                            return Err("__init__() should return None".to_owned());
                        }
                        Ok(instance)
                    });
                }
                if !keywords.is_empty() {
                    return Err(format!("{}() does not accept keyword arguments", type_name));
                }
                Ok(instance)
            });
        }
        if flags & crate::object::TYPE_FLAG_INSTANTIABLE != 0 {
            let layout = object.layout;
            let instance = context.new_instance(callable)?;
            return context.with_temporary_roots(&[instance], |context| {
                let initializer = context.special_method(instance, "__init__")?;
                let initializer_roots = initializer.into_iter().collect::<Vec<_>>();
                context.with_temporary_roots(&initializer_roots, |context| {
                    let mutable_layout = matches!(
                        layout,
                        TypeLayout::List
                            | TypeLayout::Dictionary
                            | TypeLayout::Set
                            | TypeLayout::ByteArray
                    );
                    let storage = if layout != TypeLayout::Object
                        && !(mutable_layout && initializer.is_some())
                    {
                        context.initialize_instance_storage(instance, positional, keywords)?
                    } else {
                        match context.heap.get(instance) {
                            Some(HeapObject::Instance(object)) => object.storage,
                            _ => None,
                        }
                    };
                    let storage_roots = storage.into_iter().collect::<Vec<_>>();
                    context.with_temporary_roots(&storage_roots, |context| {
                        if let Some(initializer) = initializer {
                            let result = invoke(context, initializer, positional, keywords)?;
                            if result != RValue::NONE {
                                return Err("__init__() should return None".to_owned());
                            }
                        } else if layout == TypeLayout::Object
                            && (!positional.is_empty() || !keywords.is_empty())
                        {
                            return Err(format!("{type_name}() takes no arguments"));
                        }
                        Ok(instance)
                    })
                })
            });
        }
    }
    if let Some(HeapObject::BuiltinFunction(function)) = context.heap.get(callable) {
        return match function.kind {
            BuiltinFunctionKind::Abs => invoke_abs(context, positional, keywords),
            BuiltinFunctionKind::All => invoke_all(context, positional, keywords),
            BuiltinFunctionKind::Any => invoke_any(context, positional, keywords),
            BuiltinFunctionKind::Len => invoke_len(context, positional, keywords),
            BuiltinFunctionKind::Print => invoke_print(context, positional, keywords),
            BuiltinFunctionKind::Round => invoke_round(context, positional, keywords),
            BuiltinFunctionKind::IsInstance => invoke_isinstance(context, positional, keywords),
            BuiltinFunctionKind::IsSubclass => invoke_issubclass(context, positional, keywords),
            BuiltinFunctionKind::Iter => invoke_iter(context, positional, keywords),
            BuiltinFunctionKind::Next => invoke_next(context, positional, keywords),
            BuiltinFunctionKind::TypePrepare => invoke_type_prepare(context, positional, keywords),
            BuiltinFunctionKind::Property => invoke_property(context, positional, keywords),
            BuiltinFunctionKind::StaticMethod => {
                invoke_static_method(context, positional, keywords)
            }
            BuiltinFunctionKind::ClassMethod => invoke_class_method(context, positional, keywords),
            BuiltinFunctionKind::GetAttr => invoke_getattr(context, positional, keywords),
            BuiltinFunctionKind::SetAttr => invoke_setattr(context, positional, keywords),
            BuiltinFunctionKind::DelAttr => invoke_delattr(context, positional, keywords),
            BuiltinFunctionKind::HasAttr => invoke_hasattr(context, positional, keywords),
            BuiltinFunctionKind::Callable => invoke_callable(context, positional, keywords),
            BuiltinFunctionKind::Hash => invoke_hash(context, positional, keywords),
            BuiltinFunctionKind::Repr => invoke_repr(context, positional, keywords),
            BuiltinFunctionKind::Format => invoke_format(context, positional, keywords),
            BuiltinFunctionKind::Reversed => invoke_reversed(context, positional, keywords),
            BuiltinFunctionKind::Dir => invoke_dir(context, positional, keywords),
            BuiltinFunctionKind::Vars => invoke_vars(context, positional, keywords),
            BuiltinFunctionKind::Globals => invoke_globals(context, positional, keywords),
            BuiltinFunctionKind::Locals => invoke_locals(context, positional, keywords),
            BuiltinFunctionKind::FloatConjugate => {
                invoke_float_conjugate(context, positional, keywords)
            }
            BuiltinFunctionKind::FloatIsInteger => {
                invoke_float_is_integer(context, positional, keywords)
            }
            BuiltinFunctionKind::FloatAsIntegerRatio => {
                invoke_float_as_integer_ratio(context, positional, keywords)
            }
            BuiltinFunctionKind::FloatHex => invoke_float_hex(context, positional, keywords),
            BuiltinFunctionKind::FloatFromHex => {
                invoke_float_fromhex(context, positional, keywords)
            }
            BuiltinFunctionKind::ComplexConjugate => {
                invoke_complex_conjugate(context, positional, keywords)
            }
            BuiltinFunctionKind::RangeCount => {
                invoke_range_lookup(context, positional, keywords, false)
            }
            BuiltinFunctionKind::RangeIndex => {
                invoke_range_lookup(context, positional, keywords, true)
            }
            BuiltinFunctionKind::SliceIndices => {
                invoke_slice_indices(context, positional, keywords)
            }
            BuiltinFunctionKind::Bin => invoke_base(context, positional, keywords, 2, "0b"),
            BuiltinFunctionKind::Hex => invoke_base(context, positional, keywords, 16, "0x"),
            BuiltinFunctionKind::Oct => invoke_base(context, positional, keywords, 8, "0o"),
            BuiltinFunctionKind::Chr => invoke_chr(context, positional, keywords),
            BuiltinFunctionKind::Ord => invoke_ord(context, positional, keywords),
            BuiltinFunctionKind::DivMod => invoke_divmod(context, positional, keywords),
            BuiltinFunctionKind::Pow => invoke_pow(context, positional, keywords),
            BuiltinFunctionKind::Sum => invoke_sum(context, positional, keywords),
            BuiltinFunctionKind::Min => invoke_extreme(context, positional, keywords, false),
            BuiltinFunctionKind::Max => invoke_extreme(context, positional, keywords, true),
            BuiltinFunctionKind::DictKeys => invoke_dict_view(
                context,
                positional,
                keywords,
                crate::object::DictionaryViewKind::Keys,
            ),
            BuiltinFunctionKind::DictValues => invoke_dict_view(
                context,
                positional,
                keywords,
                crate::object::DictionaryViewKind::Values,
            ),
            BuiltinFunctionKind::DictItems => invoke_dict_view(
                context,
                positional,
                keywords,
                crate::object::DictionaryViewKind::Items,
            ),
            BuiltinFunctionKind::DictGet => invoke_dict_get(context, positional, keywords),
            BuiltinFunctionKind::DictSetDefault => {
                invoke_dict_setdefault(context, positional, keywords)
            }
            BuiltinFunctionKind::DictPop => invoke_dict_pop(context, positional, keywords),
            BuiltinFunctionKind::DictPopItem => invoke_dict_popitem(context, positional, keywords),
            BuiltinFunctionKind::DictUpdate => invoke_dict_update(context, positional, keywords),
            BuiltinFunctionKind::DictClear => invoke_dict_clear(context, positional, keywords),
            BuiltinFunctionKind::DictCopy => invoke_dict_copy(context, positional, keywords),
            BuiltinFunctionKind::ListAppend => invoke_list_append(context, positional, keywords),
            BuiltinFunctionKind::ListExtend => invoke_list_extend(context, positional, keywords),
            BuiltinFunctionKind::ListInsert => invoke_list_insert(context, positional, keywords),
            BuiltinFunctionKind::ListPop => invoke_list_pop(context, positional, keywords),
            BuiltinFunctionKind::ListRemove => invoke_list_remove(context, positional, keywords),
            BuiltinFunctionKind::ListClear => invoke_list_clear(context, positional, keywords),
            BuiltinFunctionKind::ListCopy => invoke_list_copy(context, positional, keywords),
            BuiltinFunctionKind::ListCount => invoke_list_count(context, positional, keywords),
            BuiltinFunctionKind::ListIndex => invoke_list_index(context, positional, keywords),
            BuiltinFunctionKind::ListReverse => invoke_list_reverse(context, positional, keywords),
            BuiltinFunctionKind::ListSort => invoke_list_sort(context, positional, keywords),
            BuiltinFunctionKind::GeneratorIter => {
                invoke_generator_iter(context, positional, keywords)
            }
            BuiltinFunctionKind::GeneratorNext => {
                invoke_generator_next(context, positional, keywords)
            }
            BuiltinFunctionKind::GeneratorSend => {
                invoke_generator_send(context, positional, keywords)
            }
            BuiltinFunctionKind::GeneratorThrow => {
                invoke_generator_throw(context, positional, keywords)
            }
            BuiltinFunctionKind::GeneratorClose => {
                invoke_generator_close(context, positional, keywords)
            }
            BuiltinFunctionKind::ExceptionWithTraceback => {
                invoke_exception_with_traceback(context, positional, keywords)
            }
            BuiltinFunctionKind::BuiltinStorageInit => {
                invoke_builtin_storage_init(context, positional, keywords)
            }
            BuiltinFunctionKind::MemoryViewRelease => {
                invoke_memoryview_release(context, positional, keywords)
            }
            BuiltinFunctionKind::MemoryViewToBytes => {
                invoke_memoryview_tobytes(context, positional, keywords)
            }
            BuiltinFunctionKind::MemoryViewToList => {
                invoke_memoryview_tolist(context, positional, keywords)
            }
            BuiltinFunctionKind::MemoryViewToReadOnly => {
                invoke_memoryview_toreadonly(context, positional, keywords)
            }
            BuiltinFunctionKind::MemoryViewHex => {
                invoke_memoryview_hex(context, positional, keywords)
            }
            BuiltinFunctionKind::MemoryViewCast => {
                invoke_memoryview_cast(context, positional, keywords)
            }
            BuiltinFunctionKind::SetAdd => invoke_set_add(context, positional, keywords),
            BuiltinFunctionKind::SetDiscard => invoke_set_discard(context, positional, keywords),
            BuiltinFunctionKind::SetRemove => invoke_set_remove(context, positional, keywords),
            BuiltinFunctionKind::SetPop => invoke_set_pop(context, positional, keywords),
            BuiltinFunctionKind::SetClear => invoke_set_clear(context, positional, keywords),
            BuiltinFunctionKind::SetCopy => invoke_set_copy(context, positional, keywords),
            BuiltinFunctionKind::SetUpdate => invoke_set_update(context, positional, keywords),
            BuiltinFunctionKind::SetIntersectionUpdate => {
                invoke_set_intersection_update(context, positional, keywords)
            }
            BuiltinFunctionKind::SetDifferenceUpdate => {
                invoke_set_difference_update(context, positional, keywords)
            }
            BuiltinFunctionKind::SetSymmetricDifferenceUpdate => {
                invoke_set_symmetric_difference_update(context, positional, keywords)
            }
            BuiltinFunctionKind::SetUnion => invoke_set_union(context, positional, keywords),
            BuiltinFunctionKind::SetIntersection => {
                invoke_set_intersection(context, positional, keywords)
            }
            BuiltinFunctionKind::SetDifference => {
                invoke_set_difference(context, positional, keywords)
            }
            BuiltinFunctionKind::SetSymmetricDifference => {
                invoke_set_symmetric_difference(context, positional, keywords)
            }
            BuiltinFunctionKind::SetIsDisjoint => {
                invoke_set_relation(context, positional, keywords, SetRelation::Disjoint)
            }
            BuiltinFunctionKind::SetIsSubset => {
                invoke_set_relation(context, positional, keywords, SetRelation::Subset)
            }
            BuiltinFunctionKind::SetIsSuperset => {
                invoke_set_relation(context, positional, keywords, SetRelation::Superset)
            }
            BuiltinFunctionKind::Enumerate => invoke_enumerate(context, positional, keywords),
            BuiltinFunctionKind::Zip => invoke_zip(context, positional, keywords),
            BuiltinFunctionKind::Map => invoke_map(context, positional, keywords),
            BuiltinFunctionKind::Filter => invoke_filter(context, positional, keywords),
            BuiltinFunctionKind::Sorted => invoke_sorted(context, positional, keywords),
            BuiltinFunctionKind::Id => invoke_id(context, positional, keywords),
            BuiltinFunctionKind::Ascii => invoke_ascii(context, positional, keywords),
        };
    }
    if let Some(HeapObject::PropertyMethod(method)) = context.heap.get(callable) {
        let property = method.property;
        let kind = method.kind;
        if !keywords.is_empty() || positional.len() != 1 {
            return Err("property decorator takes exactly one argument".to_owned());
        }
        return context.property_replace(property, kind, positional[0]);
    }
    if let Some(HeapObject::BoundMethod(method)) = context.heap.get(callable) {
        let function = method.function;
        let receiver = method.receiver;
        let mut arguments = Vec::with_capacity(positional.len() + 1);
        arguments.push(receiver);
        arguments.extend_from_slice(positional);
        let mut roots = arguments.clone();
        roots.extend([callable, function]);
        return context.with_temporary_roots(&roots, |context| {
            invoke(context, function, &arguments, keywords)
        });
    }
    if matches!(context.heap.get(callable), Some(HeapObject::Instance(_)))
        && let Some(method) = context.special_method(callable, "__call__")?
    {
        let mut roots = positional.to_vec();
        roots.extend(keywords.iter().map(|(_, value)| *value));
        roots.extend([callable, method]);
        return context.with_temporary_roots(&roots, |context| {
            invoke(context, method, positional, keywords)
        });
    }
    let function = match context.heap.get(callable) {
        Some(HeapObject::Function(function)) => function.clone(),
        _ => return Err("object is not callable".to_owned()),
    };
    let code = match context.heap.get(function.code) {
        Some(HeapObject::Code(code)) => code.clone(),
        _ => return Err("function has an invalid code object".to_owned()),
    };
    let mut roots = Vec::with_capacity(positional.len() + keywords.len() + 7);
    roots.extend([callable, function.code]);
    roots.extend_from_slice(positional);
    roots.extend(keywords.iter().map(|(_, value)| *value));
    function
        .closure
        .into_iter()
        .for_each(|value| roots.push(value));
    function
        .defaults
        .into_iter()
        .for_each(|value| roots.push(value));
    function
        .keyword_defaults
        .into_iter()
        .for_each(|value| roots.push(value));
    function
        .annotations
        .into_iter()
        .for_each(|value| roots.push(value));
    context.with_temporary_roots(&roots, |context| {
        let bound = bind(context, &function, &code, positional, keywords)?;
        context.with_temporary_roots(&bound, |context| {
            if matches!(code.kind, crate::object::FunctionKind::Generator { .. }) {
                return context.new_generator(callable, &bound);
            }
            let mut output = RValue::NONE;
            // SAFETY: code objects are created only from code addresses using
            // the stable RNativeFunction ABI and remain valid for the executable.
            let native: RNativeFunction = unsafe { std::mem::transmute(code.code_address) };
            // SAFETY: all pointers describe live storage for the duration of the
            // call, and the context pointer uses the ABI's opaque representation.
            context.push_active_call(callable, bound.first().copied());
            let status = unsafe {
                native(
                    std::ptr::from_mut(context).cast::<c_void>(),
                    &raw const callable,
                    bound.as_ptr(),
                    bound.len(),
                    &raw mut output,
                )
            };
            context.pop_active_call();
            match status {
                RStatus::Ok => Ok(output),
                RStatus::Exception => Err("called function raised an exception".to_owned()),
                RStatus::InvalidArgument => {
                    Err("called function rejected its bound arguments".to_owned())
                }
                RStatus::AbiMismatch => {
                    Err("called function uses an incompatible Rimera ABI".to_owned())
                }
            }
        })
    })
}

pub(crate) fn call_arguments_new(
    context: &mut RimeraContext,
    callable: RValue,
) -> Result<RValue, String> {
    context.with_temporary_roots(&[callable], |context| {
        context.allocate(HeapObject::CallArguments(CallArgumentsObject {
            callable,
            positional: Vec::new(),
            keywords: Vec::new(),
        }))
    })
}

fn prepared_callable_name(context: &RimeraContext, callable: RValue) -> String {
    match context.heap.get(callable) {
        Some(HeapObject::Function(function)) => function.name.clone(),
        Some(HeapObject::BuiltinFunction(function)) => function.name.clone(),
        Some(HeapObject::Type(object)) => object.name.clone(),
        Some(HeapObject::BoundMethod(method)) => match context.heap.get(method.function) {
            Some(HeapObject::Function(function)) => function.name.clone(),
            Some(HeapObject::BuiltinFunction(function)) => function.name.clone(),
            _ => "callable".to_owned(),
        },
        _ => "callable".to_owned(),
    }
}

fn prepared_keyword_duplicate(
    context: &RimeraContext,
    arguments: RValue,
    name: &str,
) -> Result<bool, String> {
    match context.heap.get(arguments) {
        Some(HeapObject::CallArguments(arguments)) => Ok(arguments
            .keywords
            .iter()
            .any(|(current, _)| current == name)),
        _ => Err("prepared call arguments are invalid".to_owned()),
    }
}

pub(crate) fn call_argument_add(
    context: &mut RimeraContext,
    arguments: RValue,
    kind: RCallArgumentKind,
    name: Option<&str>,
    value: RValue,
) -> Result<(), String> {
    let callable = match context.heap.get(arguments) {
        Some(HeapObject::CallArguments(arguments)) => arguments.callable,
        _ => return Err("prepared call arguments are invalid".to_owned()),
    };
    context.with_temporary_roots(&[arguments, callable, value], |context| match kind {
        RCallArgumentKind::Positional => {
            let Some(HeapObject::CallArguments(arguments)) = context.heap.get_mut(arguments) else {
                return Err("prepared call arguments are invalid".to_owned());
            };
            arguments.positional.push(value);
            Ok(())
        }
        RCallArgumentKind::Starred => {
            let iterator = crate::operations::iterator_new(context, value)?;
            context.with_temporary_roots(&[arguments, callable, value, iterator], |context| {
                while let Some(item) = crate::operations::iterator_next(context, iterator)? {
                    let Some(HeapObject::CallArguments(arguments)) =
                        context.heap.get_mut(arguments)
                    else {
                        return Err("prepared call arguments are invalid".to_owned());
                    };
                    arguments.positional.push(item);
                }
                Ok(())
            })
        }
        RCallArgumentKind::Keyword => {
            let name = name.ok_or_else(|| "keyword call part is missing its name".to_owned())?;
            if prepared_keyword_duplicate(context, arguments, name)? {
                let callable = prepared_callable_name(context, callable);
                return Err(format!(
                    "{callable}() got multiple values for keyword argument '{name}'"
                ));
            }
            let Some(HeapObject::CallArguments(arguments)) = context.heap.get_mut(arguments) else {
                return Err("prepared call arguments are invalid".to_owned());
            };
            arguments.keywords.push((name.to_owned(), value));
            Ok(())
        }
        RCallArgumentKind::KeywordUnpack => {
            let keys_method = context
                .attribute_get(value, "keys")
                .map_err(|_| "argument after ** must be a mapping".to_owned())?;
            let keys = invoke(context, keys_method, &[], &[])?;
            let iterator = crate::operations::iterator_new(context, keys)?;
            context.with_temporary_roots(
                &[arguments, callable, value, keys_method, keys, iterator],
                |context| {
                    while let Some(key) = crate::operations::iterator_next(context, iterator)? {
                        let key_name = crate::operations::string_value(context, key).map(ToOwned::to_owned);
                        if let Some(key_name) = key_name {
                            if prepared_keyword_duplicate(context, arguments, &key_name)? {
                                let callable = prepared_callable_name(context, callable);
                                return Err(format!(
                                    "{callable}() got multiple values for keyword argument '{key_name}'"
                                ));
                            }
                            let mapped = context.with_temporary_roots(&[key], |context| {
                                crate::operations::item_get(context, value, key)
                            })?;
                            let Some(HeapObject::CallArguments(arguments)) =
                                context.heap.get_mut(arguments)
                            else {
                                return Err("prepared call arguments are invalid".to_owned());
                            };
                            arguments.keywords.push((key_name, mapped));
                        } else {
                            context.with_temporary_roots(&[key], |context| {
                                crate::operations::item_get(context, value, key)
                            })?;
                            return Err("keywords must be strings".to_owned());
                        }
                    }
                    Ok(())
                },
            )
        }
    })
}

pub(crate) fn invoke_prepared(
    context: &mut RimeraContext,
    callable: RValue,
    arguments: RValue,
) -> Result<RValue, String> {
    let (stored_callable, positional, keywords) = match context.heap.get(arguments) {
        Some(HeapObject::CallArguments(arguments)) => (
            arguments.callable,
            arguments.positional.clone(),
            arguments.keywords.clone(),
        ),
        _ => return Err("prepared call arguments are invalid".to_owned()),
    };
    if stored_callable != callable {
        return Err("prepared call callable changed before invocation".to_owned());
    }
    let mut roots = positional.clone();
    roots.extend(keywords.iter().map(|(_, value)| *value));
    roots.extend([callable, arguments]);
    context.with_temporary_roots(&roots, |context| {
        invoke(context, callable, &positional, &keywords)
    })
}

fn take_stop_iteration_value(context: &mut RimeraContext) -> Option<RValue> {
    let exception = context.raised?;
    if context.exception_type_name(exception) != Some("StopIteration") {
        return None;
    }
    let value = match context.heap.get(exception) {
        Some(HeapObject::Exception(exception)) => match context.heap.get(exception.arguments) {
            Some(HeapObject::Tuple(arguments)) => {
                arguments.first().copied().unwrap_or(RValue::NONE)
            }
            _ => RValue::NONE,
        },
        _ => RValue::NONE,
    };
    let _ = context.consume_exception_type("StopIteration");
    Some(value)
}

pub(crate) fn generator_delegate_start(
    context: &mut RimeraContext,
    iterator: RValue,
) -> Result<(RValue, RGeneratorDelegateOutcome), String> {
    if matches!(context.heap.get(iterator), Some(HeapObject::Generator(_))) {
        return match context.resume_generator(iterator, RGeneratorOperation::Next, RValue::NONE)? {
            crate::context::GeneratorResume {
                value,
                outcome: RGeneratorOutcome::Yielded,
            } => Ok((value, RGeneratorDelegateOutcome::Yielded)),
            crate::context::GeneratorResume {
                value,
                outcome: RGeneratorOutcome::Returned,
            } => Ok((value, RGeneratorDelegateOutcome::Completed)),
        };
    }
    if matches!(context.heap.get(iterator), Some(HeapObject::Iterator(_))) {
        return match crate::operations::iterator_next(context, iterator)? {
            Some(value) => Ok((value, RGeneratorDelegateOutcome::Yielded)),
            None => Ok((RValue::NONE, RGeneratorDelegateOutcome::Completed)),
        };
    }
    match context.invoke_special_method(iterator, "__next__", &[]) {
        Ok(Some(value)) => Ok((value, RGeneratorDelegateOutcome::Yielded)),
        Ok(None) => context.raise_error("TypeError", "object is not an iterator"),
        Err(error) => {
            if let Some(value) = take_stop_iteration_value(context) {
                Ok((value, RGeneratorDelegateOutcome::Completed))
            } else {
                Err(error)
            }
        }
    }
}

fn delegate_method(
    context: &mut RimeraContext,
    iterator: RValue,
    name: &str,
) -> Result<Option<RValue>, String> {
    match context.attribute_get(iterator, name) {
        Ok(method) => Ok(Some(method)),
        Err(error) if context.raised.is_none() => {
            if name == "throw" || name == "close" {
                Ok(None)
            } else {
                context.raise_error("AttributeError", error)
            }
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn generator_delegate_resume(
    context: &mut RimeraContext,
    iterator: RValue,
    operation: RGeneratorOperation,
    input: RValue,
) -> Result<(RValue, RGeneratorDelegateOutcome), String> {
    if matches!(operation, RGeneratorOperation::Next)
        || (matches!(operation, RGeneratorOperation::Send) && input == RValue::NONE)
    {
        return generator_delegate_start(context, iterator);
    }

    let injected = if matches!(
        operation,
        RGeneratorOperation::Throw | RGeneratorOperation::Close
    ) {
        let raised = context.raised.take();
        context.exception = None;
        raised
    } else {
        None
    };

    let result = match operation {
        RGeneratorOperation::Send => {
            let method = delegate_method(context, iterator, "send")?
                .ok_or_else(|| "delegate has no send method".to_owned())?;
            context.with_temporary_roots(&[iterator, method, input], |context| {
                invoke(context, method, &[input], &[])
            })
        }
        RGeneratorOperation::Throw => {
            let Some(method) = delegate_method(context, iterator, "throw")? else {
                if let Some(injected) = injected {
                    context.raise_value(injected, None, false)?;
                }
                return Ok((RValue::NONE, RGeneratorDelegateOutcome::Propagate));
            };
            context.with_temporary_roots(&[iterator, method, input], |context| {
                invoke(context, method, &[input], &[])
            })
        }
        RGeneratorOperation::Close => {
            let Some(method) = delegate_method(context, iterator, "close")? else {
                if let Some(injected) = injected {
                    context.raise_value(injected, None, false)?;
                }
                return Ok((RValue::NONE, RGeneratorDelegateOutcome::Propagate));
            };
            let close_result = context.with_temporary_roots(&[iterator, method], |context| {
                invoke(context, method, &[], &[])
            });
            match close_result {
                Ok(_) => {
                    if let Some(injected) = injected {
                        context.raise_value(injected, None, false)?;
                    }
                    return Ok((RValue::NONE, RGeneratorDelegateOutcome::Propagate));
                }
                Err(error) => return Err(error),
            }
        }
        RGeneratorOperation::Next => unreachable!("next delegation returned early"),
    };

    match result {
        Ok(value) => Ok((value, RGeneratorDelegateOutcome::Yielded)),
        Err(error) => {
            if let Some(value) = take_stop_iteration_value(context) {
                Ok((value, RGeneratorDelegateOutcome::Completed))
            } else if context.raised.is_some() {
                Ok((RValue::NONE, RGeneratorDelegateOutcome::Propagate))
            } else {
                Err(error)
            }
        }
    }
}

fn generator_resume_value(
    context: &mut RimeraContext,
    generator: RValue,
    operation: rimera_abi::RGeneratorOperation,
    input: RValue,
) -> Result<RValue, String> {
    match context.resume_generator(generator, operation, input)? {
        crate::context::GeneratorResume {
            value,
            outcome: rimera_abi::RGeneratorOutcome::Yielded,
        } => Ok(value),
        crate::context::GeneratorResume {
            value,
            outcome: rimera_abi::RGeneratorOutcome::Returned,
        } => {
            context.raise_stop_iteration(value)?;
            Err("generator exhausted".to_owned())
        }
    }
}

fn invoke_generator_iter(
    _context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("generator.__iter__() takes no arguments".to_owned());
    }
    Ok(positional[0])
}

fn invoke_generator_next(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("generator.__next__() takes no arguments".to_owned());
    }
    generator_resume_value(
        context,
        positional[0],
        rimera_abi::RGeneratorOperation::Next,
        RValue::NONE,
    )
}

fn invoke_generator_send(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("generator.send() takes exactly one argument".to_owned());
    }
    generator_resume_value(
        context,
        positional[0],
        rimera_abi::RGeneratorOperation::Send,
        positional[1],
    )
}

fn normalize_generator_throw(
    context: &mut RimeraContext,
    arguments: &[RValue],
) -> Result<RValue, String> {
    if !(1..=3).contains(&arguments.len()) {
        return Err("generator.throw() takes 1 to 3 arguments".to_owned());
    }
    let spec = arguments[0];
    let value = arguments.get(1).copied().unwrap_or(RValue::NONE);
    let traceback = arguments.get(2).copied().unwrap_or(RValue::NONE);
    let exception = if matches!(context.heap.get(spec), Some(HeapObject::Exception(_))) {
        if value != RValue::NONE && value != spec {
            return context.raise_error(
                "TypeError",
                "instance exception may not have a separate value",
            );
        }
        spec
    } else if matches!(context.heap.get(spec), Some(HeapObject::Type(_))) {
        let base_exception = context
            .builtin_type("BaseException")
            .ok_or_else(|| "BaseException type is missing".to_owned())?;
        if !context.is_subclass(spec, base_exception)? {
            return context.raise_error(
                "TypeError",
                "exceptions must be classes or instances deriving from BaseException",
            );
        }
        if value != RValue::NONE
            && matches!(context.heap.get(value), Some(HeapObject::Exception(_)))
            && context.is_instance(value, spec)?
        {
            value
        } else if value == RValue::NONE {
            invoke(context, spec, &[], &[])?
        } else {
            invoke(context, spec, &[value], &[])?
        }
    } else {
        return context.raise_error(
            "TypeError",
            "exceptions must be classes or instances deriving from BaseException",
        );
    };
    if traceback != RValue::NONE {
        context.attribute_set(exception, "__traceback__", traceback)?;
    }
    Ok(exception)
}

fn invoke_generator_throw(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || !(2..=4).contains(&positional.len()) {
        return Err("generator.throw() takes 1 to 3 arguments".to_owned());
    }
    let generator = positional[0];
    let exception = context.with_temporary_roots(&[generator], |context| {
        normalize_generator_throw(context, &positional[1..])
    })?;
    context.with_temporary_roots(&[generator, exception], |context| {
        generator_resume_value(
            context,
            generator,
            rimera_abi::RGeneratorOperation::Throw,
            exception,
        )
    })
}

fn invoke_generator_close(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("generator.close() takes no arguments".to_owned());
    }
    match context.resume_generator(
        positional[0],
        rimera_abi::RGeneratorOperation::Close,
        RValue::NONE,
    )? {
        crate::context::GeneratorResume {
            outcome: rimera_abi::RGeneratorOutcome::Yielded,
            ..
        } => context.raise_error("RuntimeError", "generator ignored GeneratorExit"),
        crate::context::GeneratorResume {
            outcome: rimera_abi::RGeneratorOutcome::Returned,
            ..
        } => Ok(RValue::NONE),
    }
}

fn invoke_exception_with_traceback(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("with_traceback() takes exactly one argument".to_owned());
    }
    let receiver = positional[0];
    let traceback = positional[1];
    if !matches!(context.heap.get(receiver), Some(HeapObject::Exception(_))) {
        return Err("with_traceback() receiver is not an exception".to_owned());
    }
    if traceback != RValue::NONE
        && !matches!(context.heap.get(traceback), Some(HeapObject::Traceback(_)))
    {
        return context.raise_error("TypeError", "__traceback__ must be a traceback or None");
    }
    let Some(HeapObject::Exception(exception)) = context.heap.get_mut(receiver) else {
        unreachable!("exception receiver was checked");
    };
    exception.traceback = (traceback != RValue::NONE).then_some(traceback);
    Ok(receiver)
}

fn invoke_abs(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err(format!(
            "abs() takes exactly one argument ({} given)",
            positional.len()
        ));
    }
    crate::operations::absolute(context, positional[0])
}

fn invoke_all(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err(format!(
            "all() takes exactly one argument ({} given)",
            positional.len()
        ));
    }
    let iterator = crate::operations::iterator_new(context, positional[0])?;
    context.with_temporary_roots(&[iterator], |context| {
        while let Some(value) = crate::operations::iterator_next(context, iterator)? {
            if !crate::operations::truthy(context, value)? {
                return Ok(RValue::boolean(false));
            }
        }
        Ok(RValue::boolean(true))
    })
}

fn invoke_any(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err(format!(
            "any() takes exactly one argument ({} given)",
            positional.len()
        ));
    }
    let iterator = crate::operations::iterator_new(context, positional[0])?;
    context.with_temporary_roots(&[iterator], |context| {
        while let Some(value) = crate::operations::iterator_next(context, iterator)? {
            if crate::operations::truthy(context, value)? {
                return Ok(RValue::boolean(true));
            }
        }
        Ok(RValue::boolean(false))
    })
}

fn invoke_len(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err(format!(
            "len() takes exactly one argument ({} given)",
            positional.len()
        ));
    }
    crate::operations::length(context, positional[0])
}

fn stringify(context: &mut RimeraContext, value: RValue) -> Result<String, String> {
    crate::operations::stringify(context, value)
}

fn print_text_argument(
    context: &RimeraContext,
    value: RValue,
    name: &str,
    default: &str,
) -> Result<String, String> {
    if value == RValue::NONE {
        return Ok(default.to_owned());
    }
    match context.heap.get(value) {
        Some(HeapObject::String(text)) => Ok(text.clone()),
        _ => Err(format!("{name} must be None or a string, not non-string")),
    }
}

fn invoke_print(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    let mut sep = " ".to_owned();
    let mut end = "\n".to_owned();
    let mut file = RValue::NONE;
    let mut flush = false;
    for (name, value) in keywords {
        match name.as_str() {
            "sep" => sep = print_text_argument(context, *value, "sep", " ")?,
            "end" => end = print_text_argument(context, *value, "end", "\n")?,
            "file" => file = *value,
            "flush" => flush = crate::operations::truthy(context, *value)?,
            _ => {
                return Err(format!(
                    "'{}' is an invalid keyword argument for print()",
                    name
                ));
            }
        }
    }
    let rendered = positional
        .iter()
        .map(|value| stringify(context, *value))
        .collect::<Result<Vec<_>, _>>()?;
    let text = format!("{}{}", rendered.join(&sep), end);
    if file == RValue::NONE {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(text.as_bytes())
            .map_err(|error| format!("print() failed to write stdout: {error}"))?;
        if flush {
            stdout
                .flush()
                .map_err(|error| format!("print() failed to flush stdout: {error}"))?;
        }
        return Ok(RValue::NONE);
    }
    let write = context.attribute_get(file, "write")?;
    let text_value = crate::operations::string(context, &text)?;
    context.with_temporary_roots(&[file, write, text_value], |context| {
        invoke(context, write, &[text_value], &[])
    })?;
    if flush {
        let flush_method = context.attribute_get(file, "flush")?;
        context.with_temporary_roots(&[file, flush_method], |context| {
            invoke(context, flush_method, &[], &[])
        })?;
    }
    Ok(RValue::NONE)
}

fn float_abs_ratio(value: f64) -> (BigInt, BigInt) {
    let bits = value.to_bits() & !(1_u64 << 63);
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (significand, exponent) = if exponent_bits == 0 {
        (fraction, -1074)
    } else {
        ((1_u64 << 52) | fraction, exponent_bits - 1023 - 52)
    };
    let mut numerator = BigInt::from(significand);
    if exponent >= 0 {
        numerator <<= exponent as usize;
        (numerator, BigInt::from(1_u8))
    } else {
        (
            numerator,
            BigInt::from(1_u8)
                << usize::try_from(-exponent).expect("finite f64 exponent fits usize"),
        )
    }
}

fn round_positive_ratio_ties_even(numerator: &BigInt, denominator: &BigInt) -> BigInt {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let doubled = &remainder * 2_u8;
    let round_up = doubled > *denominator
        || (doubled == *denominator && (&quotient & BigInt::from(1_u8)) != BigInt::ZERO);
    if round_up { quotient + 1_u8 } else { quotient }
}

fn round_float_to_bigint(value: f64) -> BigInt {
    let (numerator, denominator) = float_abs_ratio(value);
    let rounded = round_positive_ratio_ties_even(&numerator, &denominator);
    if value.is_sign_negative() {
        -rounded
    } else {
        rounded
    }
}

fn decimal_units_to_float(
    units: &BigInt,
    decimal_places: usize,
    negative: bool,
) -> Result<f64, String> {
    if units == &BigInt::ZERO {
        return Ok(if negative { -0.0 } else { 0.0 });
    }
    let digits = units.to_str_radix(10);
    let mut text = if decimal_places == 0 {
        digits
    } else if digits.len() <= decimal_places {
        format!("0.{}{}", "0".repeat(decimal_places - digits.len()), digits)
    } else {
        let split = digits.len() - decimal_places;
        format!("{}.{}", &digits[..split], &digits[split..])
    };
    if negative {
        text.insert(0, '-');
    }
    let value = text
        .parse::<f64>()
        .map_err(|_| "rounded value too large to represent".to_owned())?;
    if !value.is_finite() {
        return Err("rounded value too large to represent".to_owned());
    }
    Ok(value)
}

fn round_float_ndigits(value: f64, ndigits: i32) -> Result<f64, String> {
    if !value.is_finite() {
        return Ok(value);
    }
    let negative = value.is_sign_negative();
    let (numerator, denominator) = float_abs_ratio(value);
    if ndigits >= 0 {
        let places = ndigits as u32;
        let scale = BigInt::from(10_u8).pow(places);
        let units = round_positive_ratio_ties_even(&(numerator * scale), &denominator);
        decimal_units_to_float(&units, places as usize, negative)
    } else {
        let places = ndigits.unsigned_abs();
        let scale = BigInt::from(10_u8).pow(places);
        let units = round_positive_ratio_ties_even(&numerator, &(denominator * scale));
        if units == BigInt::ZERO {
            return Ok(if negative { -0.0 } else { 0.0 });
        }
        let mut text = units.to_str_radix(10);
        text.push_str(&"0".repeat(places as usize));
        if negative {
            text.insert(0, '-');
        }
        let rounded = text
            .parse::<f64>()
            .map_err(|_| "rounded value too large to represent".to_owned())?;
        if !rounded.is_finite() {
            return Err("rounded value too large to represent".to_owned());
        }
        Ok(rounded)
    }
}

fn invoke_round(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if positional.len() > 2 {
        return Err(format!(
            "round() expected at most 2 arguments, got {}",
            positional.len()
        ));
    }
    let mut number = positional.first().copied();
    let mut ndigits = positional.get(1).copied();
    for (name, value) in keywords {
        match name.as_str() {
            "number" if number.replace(*value).is_none() => {}
            "ndigits" if ndigits.replace(*value).is_none() => {}
            _ => {
                return Err(format!(
                    "round() got an unexpected keyword argument '{name}'"
                ));
            }
        }
    }
    let number =
        number.ok_or_else(|| "round() missing required argument 'number' (pos 1)".to_owned())?;
    if let Ok(integer) = crate::operations::integer(context, number) {
        if ndigits.is_none() || ndigits == Some(RValue::NONE) {
            return crate::operations::store_integer(context, integer);
        }
        let digits = crate::operations::index_integer(context, ndigits.expect("checked"))?;
        if digits.sign() != num_bigint::Sign::Minus {
            return crate::operations::store_integer(context, integer);
        }
        let places = (-digits).to_u32().unwrap_or(u32::MAX);
        if places > 100_000 {
            return Ok(RValue::small_int(0));
        }
        let factor = BigInt::from(10_u8).pow(places);
        let quotient = &integer / &factor;
        let remainder = (&integer % &factor).abs();
        let half = &factor / 2_u8;
        let mut rounded = quotient.clone();
        if remainder > half
            || (remainder == half && (&quotient & BigInt::from(1_u8)) != BigInt::ZERO)
        {
            rounded += if integer.sign() == num_bigint::Sign::Minus {
                -1
            } else {
                1
            };
        }
        return crate::operations::store_integer(context, rounded * factor);
    }
    if let Some(HeapObject::Float(number)) = context.heap.get(number) {
        let number = *number;
        if ndigits.is_none() || ndigits == Some(RValue::NONE) {
            if number.is_infinite() {
                return context
                    .raise_error("OverflowError", "cannot convert float infinity to integer");
            }
            if number.is_nan() {
                return context.raise_error("ValueError", "cannot convert float NaN to integer");
            }
            return crate::operations::store_integer(context, round_float_to_bigint(number));
        }
        let digits = crate::operations::index_integer(context, ndigits.expect("checked"))?;
        if !number.is_finite() {
            return crate::operations::float(context, number);
        }
        let digits = if digits.sign() == num_bigint::Sign::Minus {
            let places = (-digits).to_u32().unwrap_or(u32::MAX);
            if places > 308 {
                return crate::operations::float(context, 0.0_f64.copysign(number));
            }
            -(places as i32)
        } else {
            let places = digits.to_u32().unwrap_or(u32::MAX);
            if places > 323 {
                return crate::operations::float(context, number);
            }
            places as i32
        };
        let rounded = match round_float_ndigits(number, digits) {
            Ok(value) => value,
            Err(message) => return context.raise_error("OverflowError", message),
        };
        return crate::operations::float(context, rounded);
    }
    let args = ndigits.map_or_else(Vec::new, |value| vec![value]);
    context
        .invoke_special_method(number, "__round__", &args)?
        .ok_or_else(|| {
            format!(
                "type {} doesn't define __round__ method",
                python_type_name(context, number).unwrap_or_else(|_| "object".to_owned())
            )
        })
}

fn builtin_number_method_receiver(
    context: &RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
    method: &str,
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err(format!("{method}() takes no arguments"));
    }
    let receiver = positional[0];
    match method.split('.').next() {
        Some("float") if matches!(context.heap.get(receiver), Some(HeapObject::Float(_))) => {
            Ok(receiver)
        }
        Some("complex")
            if matches!(context.heap.get(receiver), Some(HeapObject::Complex { .. })) =>
        {
            Ok(receiver)
        }
        _ => Err(format!("invalid receiver for {method}()")),
    }
}

fn invoke_float_conjugate(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    builtin_number_method_receiver(context, positional, keywords, "float.conjugate")
}

fn invoke_complex_conjugate(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    let receiver =
        builtin_number_method_receiver(context, positional, keywords, "complex.conjugate")?;
    let Some(HeapObject::Complex { real, imag }) = context.heap.get(receiver) else {
        unreachable!("complex receiver was validated above");
    };
    crate::operations::complex(context, *real, -*imag)
}

fn invoke_float_is_integer(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    let receiver =
        builtin_number_method_receiver(context, positional, keywords, "float.is_integer")?;
    let Some(HeapObject::Float(value)) = context.heap.get(receiver) else {
        unreachable!("float receiver was validated above");
    };
    Ok(RValue::boolean(value.is_finite() && value.fract() == 0.0))
}

fn float_as_integer_ratio_parts(value: f64) -> (BigInt, BigInt) {
    if value == 0.0 {
        return (BigInt::ZERO, BigInt::from(1_u8));
    }
    let bits = value.to_bits();
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let (mantissa, exponent) = if exponent_bits == 0 {
        (fraction, -1074_i32)
    } else {
        (fraction | (1_u64 << 52), exponent_bits - 1023 - 52)
    };
    let mut numerator = BigInt::from(mantissa);
    let mut denominator = BigInt::from(1_u8);
    if exponent >= 0 {
        numerator <<= exponent as usize;
    } else {
        let denominator_shift = (-exponent) as u32;
        let reduction = mantissa.trailing_zeros().min(denominator_shift);
        numerator >>= reduction as usize;
        denominator <<= (denominator_shift - reduction) as usize;
    }
    if value.is_sign_negative() {
        numerator = -numerator;
    }
    (numerator, denominator)
}

fn invoke_float_as_integer_ratio(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    let receiver =
        builtin_number_method_receiver(context, positional, keywords, "float.as_integer_ratio")?;
    let Some(HeapObject::Float(value)) = context.heap.get(receiver) else {
        unreachable!("float receiver was validated above");
    };
    let value = *value;
    if value.is_nan() {
        return context.raise_error("ValueError", "cannot convert NaN to integer ratio");
    }
    if value.is_infinite() {
        return context.raise_error("OverflowError", "cannot convert Infinity to integer ratio");
    }
    let (numerator, denominator) = float_as_integer_ratio_parts(value);
    let numerator = crate::operations::store_integer(context, numerator)?;
    let denominator = crate::operations::store_integer(context, denominator)?;
    context.with_temporary_roots(&[numerator, denominator], |context| {
        context.allocate(HeapObject::Tuple(
            vec![numerator, denominator].into_boxed_slice(),
        ))
    })
}

fn python_float_hex(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf"
        } else {
            "inf"
        }
        .to_owned();
    }
    let bits = value.to_bits();
    let sign = if bits >> 63 != 0 { "-" } else { "" };
    let exponent_bits = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    if exponent_bits == 0 {
        if fraction == 0 {
            format!("{sign}0x0.0p+0")
        } else {
            format!("{sign}0x0.{fraction:013x}p-1022")
        }
    } else {
        let exponent = exponent_bits - 1023;
        format!("{sign}0x1.{fraction:013x}p{exponent:+}")
    }
}

fn invoke_float_hex(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    let receiver = builtin_number_method_receiver(context, positional, keywords, "float.hex")?;
    let Some(HeapObject::Float(value)) = context.heap.get(receiver) else {
        unreachable!("float receiver was validated above");
    };
    crate::operations::string(context, &python_float_hex(*value))
}

fn parse_hex_float(text: &str) -> Result<f64, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("invalid hexadecimal floating-point string".to_owned());
    }
    let (negative, body) = if let Some(rest) = text.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = text.strip_prefix('+') {
        (false, rest)
    } else {
        (false, text)
    };
    let lower = body.to_ascii_lowercase();
    let special = match lower.as_str() {
        "inf" | "infinity" => Some(f64::INFINITY),
        "nan" => Some(f64::NAN),
        _ => None,
    };
    if let Some(mut value) = special {
        if negative {
            value = -value;
        }
        return Ok(value);
    }
    let body = lower.strip_prefix("0x").unwrap_or(&lower);
    let (mantissa, exponent) = match body.split_once('p') {
        Some((mantissa, exponent)) => {
            if exponent.is_empty() {
                return Err("invalid hexadecimal floating-point string".to_owned());
            }
            let exponent = exponent
                .parse::<i32>()
                .map_err(|_| "invalid hexadecimal floating-point string".to_owned())?;
            (mantissa, exponent)
        }
        None => (body, 0),
    };
    if mantissa.matches('.').count() > 1 {
        return Err("invalid hexadecimal floating-point string".to_owned());
    }
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty() {
        return Err("invalid hexadecimal floating-point string".to_owned());
    }
    let mut value = 0.0_f64;
    let mut digits = 0_usize;
    for character in whole.chars().chain(fraction.chars()) {
        let digit = character
            .to_digit(16)
            .ok_or_else(|| "invalid hexadecimal floating-point string".to_owned())?;
        value = value.mul_add(16.0, f64::from(digit));
        digits += 1;
    }
    if digits == 0 {
        return Err("invalid hexadecimal floating-point string".to_owned());
    }
    let fractional_bits = i32::try_from(fraction.len())
        .map_err(|_| "hexadecimal floating-point string is too large".to_owned())?
        .saturating_mul(4);
    value *= 2.0_f64.powi(exponent.saturating_sub(fractional_bits));
    if negative {
        value = -value;
    }
    Ok(value)
}

fn invoke_float_fromhex(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("float.fromhex() takes exactly one argument".to_owned());
    }
    let class = positional[0];
    let text = match context.heap.get(positional[1]) {
        Some(HeapObject::String(text)) => text.clone(),
        _ => return context.raise_error("TypeError", "bad argument type for built-in operation"),
    };
    let parsed = match parse_hex_float(&text) {
        Ok(value) => value,
        Err(message) => return context.raise_error("ValueError", message),
    };
    let value = crate::operations::float(context, parsed)?;
    let float_type = context
        .builtin_type("float")
        .ok_or_else(|| "float type is missing".to_owned())?;
    if class == float_type {
        Ok(value)
    } else {
        context.with_temporary_roots(&[class, value], |context| {
            invoke(context, class, &[value], &[])
        })
    }
}

fn builtin_text(context: &RimeraContext, value: RValue, what: &str) -> Result<String, String> {
    match context.heap.get(value) {
        Some(HeapObject::String(text)) => Ok(text.clone()),
        _ => Err(format!("{what} must be str")),
    }
}

fn normalize_encoding(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('_', "-")
}

fn encode_text(text: &str, encoding: &str, errors: &str) -> Result<Vec<u8>, String> {
    let encoding = normalize_encoding(encoding);
    match encoding.as_str() {
        "utf-8" | "utf8" => Ok(text.as_bytes().to_vec()),
        "ascii" => {
            let mut out = Vec::with_capacity(text.len());
            for character in text.chars() {
                if character.is_ascii() {
                    out.push(character as u8);
                } else {
                    match errors {
                        "ignore" => {}
                        "replace" => out.push(b'?'),
                        "strict" => return Err("'ascii' codec can't encode character".to_owned()),
                        _ => return Err(format!("unknown error handler name '{errors}'")),
                    }
                }
            }
            Ok(out)
        }
        "latin-1" | "latin1" | "iso-8859-1" => {
            let mut out = Vec::with_capacity(text.len());
            for character in text.chars() {
                if (character as u32) <= 0xff {
                    out.push(character as u8);
                } else {
                    match errors {
                        "ignore" => {}
                        "replace" => out.push(b'?'),
                        "strict" => return Err("'latin-1' codec can't encode character".to_owned()),
                        _ => return Err(format!("unknown error handler name '{errors}'")),
                    }
                }
            }
            Ok(out)
        }
        _ => Err(format!("unknown encoding: {encoding}")),
    }
}

fn decode_bytes(bytes: &[u8], encoding: &str, errors: &str) -> Result<String, String> {
    let encoding = normalize_encoding(encoding);
    match encoding.as_str() {
        "utf-8" | "utf8" => match errors {
            "strict" => String::from_utf8(bytes.to_vec())
                .map_err(|_| "'utf-8' codec can't decode bytes".to_owned()),
            "ignore" => Ok(String::from_utf8_lossy(bytes)
                .chars()
                .filter(|character| *character != '\u{fffd}')
                .collect()),
            "replace" => Ok(String::from_utf8_lossy(bytes).into_owned()),
            _ => Err(format!("unknown error handler name '{errors}'")),
        },
        "ascii" => {
            let mut out = String::new();
            for byte in bytes {
                if *byte <= 0x7f {
                    out.push(char::from(*byte));
                } else {
                    match errors {
                        "ignore" => {}
                        "replace" => out.push('\u{fffd}'),
                        "strict" => return Err("'ascii' codec can't decode byte".to_owned()),
                        _ => return Err(format!("unknown error handler name '{errors}'")),
                    }
                }
            }
            Ok(out)
        }
        "latin-1" | "latin1" | "iso-8859-1" => {
            Ok(bytes.iter().map(|byte| char::from(*byte)).collect())
        }
        _ => Err(format!("unknown encoding: {encoding}")),
    }
}

fn unicode_decimal_digit(character: char) -> Option<u8> {
    const DECIMAL_ZEROES: &[u32] = &[
        0x0030, 0x0660, 0x06f0, 0x07c0, 0x0966, 0x09e6, 0x0a66, 0x0ae6, 0x0b66, 0x0be6, 0x0c66,
        0x0ce6, 0x0d66, 0x0de6, 0x0e50, 0x0ed0, 0x0f20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946,
        0x19d0, 0x1a80, 0x1a90, 0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0,
        0xa9f0, 0xaa50, 0xabf0, 0xff10, 0x104a0, 0x10d30, 0x11066, 0x110f0, 0x11136, 0x111d0,
        0x112f0, 0x11450, 0x114d0, 0x11650, 0x116c0, 0x11730, 0x118e0, 0x11950, 0x11c50, 0x11d50,
        0x11da0, 0x11f50, 0x16a60, 0x16ac0, 0x16b50, 0x1e140, 0x1e2f0, 0x1e4f0, 0x1e950,
    ];
    let codepoint = character as u32;
    if (0x1d7ce..=0x1d7ff).contains(&codepoint) {
        return Some(((codepoint - 0x1d7ce) % 10) as u8);
    }
    DECIMAL_ZEROES.iter().find_map(|zero| {
        (codepoint >= *zero && codepoint < *zero + 10).then(|| (codepoint - *zero) as u8)
    })
}

fn normalize_decimal_digits(text: &str) -> String {
    text.chars()
        .map(|character| {
            unicode_decimal_digit(character)
                .map(|digit| char::from(b'0' + digit))
                .unwrap_or(character)
        })
        .collect()
}

fn integer_digit_value(character: char) -> Option<u32> {
    unicode_decimal_digit(character)
        .map(u32::from)
        .or_else(|| match character {
            'a'..='z' => Some(10 + u32::from(character as u8 - b'a')),
            'A'..='Z' => Some(10 + u32::from(character as u8 - b'A')),
            _ => None,
        })
}

fn parse_integer_text(text: &str, requested_base: u32) -> Result<BigInt, String> {
    if requested_base != 0 && !(2..=36).contains(&requested_base) {
        return Err("int() base must be >= 2 and <= 36, or 0".to_owned());
    }
    let normalized = normalize_decimal_digits(text.trim());
    let (negative, unsigned) = if let Some(rest) = normalized.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = normalized.strip_prefix('+') {
        (false, rest)
    } else {
        (false, normalized.as_str())
    };
    if unsigned.is_empty() {
        return Err(format!(
            "invalid literal for int() with base {requested_base}"
        ));
    }

    let mut base = if requested_base == 0 {
        10
    } else {
        requested_base
    };
    let mut digits = unsigned;
    let mut prefixed = false;
    for (prefix, prefix_base) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if unsigned.starts_with(prefix) && (requested_base == 0 || requested_base == prefix_base) {
            base = prefix_base;
            digits = &unsigned[prefix.len()..];
            prefixed = true;
            break;
        }
    }

    let characters = digits.chars().collect::<Vec<_>>();
    let mut clean = String::with_capacity(digits.len());
    let mut saw_digit = false;
    for (index, character) in characters.iter().copied().enumerate() {
        if character == '_' {
            let next_is_digit = characters
                .get(index + 1)
                .and_then(|next| integer_digit_value(*next))
                .is_some_and(|digit| digit < base);
            let valid = if index == 0 {
                prefixed && next_is_digit
            } else {
                integer_digit_value(characters[index - 1]).is_some_and(|digit| digit < base)
                    && next_is_digit
            };
            if !valid {
                return Err(format!(
                    "invalid literal for int() with base {requested_base}"
                ));
            }
            continue;
        }
        let Some(digit) = integer_digit_value(character) else {
            return Err(format!(
                "invalid literal for int() with base {requested_base}"
            ));
        };
        if digit >= base {
            return Err(format!(
                "invalid literal for int() with base {requested_base}"
            ));
        }
        saw_digit = true;
        clean.push(char::from_digit(digit, base).expect("validated integer digit"));
    }
    if !saw_digit {
        return Err(format!(
            "invalid literal for int() with base {requested_base}"
        ));
    }
    if requested_base == 0
        && !prefixed
        && clean.len() > 1
        && clean.starts_with('0')
        && clean.chars().any(|digit| digit != '0')
    {
        return Err("invalid literal for int() with base 0".to_owned());
    }

    let mut value = BigInt::parse_bytes(clean.as_bytes(), base)
        .ok_or_else(|| format!("invalid literal for int() with base {requested_base}"))?;
    if negative {
        value = -value;
    }
    Ok(value)
}

fn parse_float_text(text: &str) -> Result<f64, String> {
    let normalized = normalize_decimal_digits(text.trim());
    if normalized.is_empty() || normalized.chars().any(char::is_whitespace) {
        return Err("could not convert string to float".to_owned());
    }
    let characters = normalized.chars().collect::<Vec<_>>();
    for (index, character) in characters.iter().copied().enumerate() {
        if character != '_' {
            continue;
        }
        let previous_is_digit = index
            .checked_sub(1)
            .and_then(|previous| characters.get(previous))
            .is_some_and(|character| character.is_ascii_digit());
        let next_is_digit = characters
            .get(index + 1)
            .is_some_and(|character| character.is_ascii_digit());
        if !previous_is_digit || !next_is_digit {
            return Err("could not convert string to float".to_owned());
        }
    }
    let clean = normalized.replace('_', "");
    let (negative, unsigned) = if let Some(rest) = clean.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = clean.strip_prefix('+') {
        (false, rest)
    } else {
        (false, clean.as_str())
    };
    let special = unsigned.to_ascii_lowercase();
    let value = match special.as_str() {
        "inf" | "infinity" => f64::INFINITY,
        "nan" => f64::NAN,
        _ => clean
            .parse::<f64>()
            .map_err(|_| "could not convert string to float".to_owned())?,
    };
    Ok(if negative && special.as_str() != "nan" {
        -value.abs()
    } else if negative {
        -value
    } else {
        value
    })
}

fn ascii_numeric_text(bytes: &[u8]) -> Result<String, String> {
    if !bytes.is_ascii() {
        return Err("invalid non-ASCII numeric bytes".to_owned());
    }
    Ok(String::from_utf8(bytes.to_vec()).expect("ASCII is valid UTF-8"))
}

fn is_byte_text_family(context: &RimeraContext, value: RValue) -> bool {
    match context.heap.get(value) {
        Some(HeapObject::Bytes(_) | HeapObject::ByteArray(_)) => true,
        Some(HeapObject::Instance(instance)) => instance.storage.is_some_and(|storage| {
            matches!(
                context.heap.get(storage),
                Some(HeapObject::Bytes(_) | HeapObject::ByteArray(_))
            )
        }),
        _ => false,
    }
}

fn bytes_payload(context: &mut RimeraContext, value: RValue) -> Result<Vec<u8>, String> {
    let storage = match context.heap.get(value) {
        Some(HeapObject::Instance(instance)) => instance.storage,
        _ => None,
    };
    if let Some(storage) = storage {
        return bytes_payload(context, storage);
    }
    match context.heap.get(value) {
        Some(HeapObject::Bytes(bytes)) => Ok(bytes.clone()),
        Some(HeapObject::ByteArray(bytes)) => Ok(bytes.bytes.clone()),
        Some(HeapObject::MemoryView(_)) => crate::operations::memoryview_to_bytes(context, value)
            .and_then(|bytes| match context.heap.get(bytes) {
                Some(HeapObject::Bytes(bytes)) => Ok(bytes.clone()),
                _ => Err("memoryview conversion failed".to_owned()),
            }),
        Some(_) => Err("a bytes-like object is required".to_owned()),
        None => Err("value contains a stale heap handle".to_owned()),
    }
}

fn numeric_real(context: &mut RimeraContext, value: RValue) -> Result<f64, String> {
    if let Some(value) = crate::operations::numeric_float_for_constructor(context, value) {
        return Ok(value);
    }
    if let Some(result) = context.invoke_special_method(value, "__float__", &[])? {
        let Some(value) = crate::operations::float_family_value(context, result) else {
            return context.raise_error("TypeError", "__float__ returned non-float");
        };
        return Ok(value);
    }
    let indexed = crate::operations::index_integer(context, value)?;
    indexed
        .to_f64()
        .ok_or_else(|| "int too large to convert to float".to_owned())
}

fn numeric_complex_parts(context: &mut RimeraContext, value: RValue) -> Result<(f64, f64), String> {
    if let Some(value) = crate::operations::complex_family_value(context, value) {
        return Ok(value);
    }
    if let Some(result) = context.invoke_special_method(value, "__complex__", &[])? {
        return crate::operations::complex_family_value(context, result)
            .ok_or_else(|| "__complex__ returned non-complex".to_owned())
            .or_else(|message| context.raise_error("TypeError", message));
    }
    Ok((numeric_real(context, value)?, 0.0))
}

fn parse_complex_text(text: &str) -> Result<(f64, f64), String> {
    let mut body = text.trim();
    if body.starts_with('(') {
        if !body.ends_with(')') {
            return Err("complex() arg is a malformed string".to_owned());
        }
        body = body[1..body.len() - 1].trim();
    }
    if body.is_empty() || body.contains(['(', ')']) || body.chars().any(char::is_whitespace) {
        return Err("complex() arg is a malformed string".to_owned());
    }
    let normalized = normalize_decimal_digits(body);
    if !normalized.ends_with(['j', 'J']) {
        return parse_float_text(&normalized)
            .map(|real| (real, 0.0))
            .map_err(|_| "complex() arg is a malformed string".to_owned());
    }

    let imaginary_body = &normalized[..normalized.len() - 1];
    let mut split = None;
    let mut previous = None;
    for (index, character) in imaginary_body.char_indices() {
        if index != 0 && matches!(character, '+' | '-') && !matches!(previous, Some('e' | 'E')) {
            split = Some(index);
        }
        previous = Some(character);
    }
    let (real_text, imag_text) = split
        .map(|index| imaginary_body.split_at(index))
        .unwrap_or(("", imaginary_body));
    let real = if real_text.is_empty() {
        0.0
    } else {
        parse_float_text(real_text).map_err(|_| "complex() arg is a malformed string".to_owned())?
    };
    let imag = match imag_text {
        "" | "+" => 1.0,
        "-" => -1.0,
        value => {
            parse_float_text(value).map_err(|_| "complex() arg is a malformed string".to_owned())?
        }
    };
    Ok((real, imag))
}

pub(crate) fn invoke_builtin_storage_init(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    let Some(receiver) = positional.first().copied() else {
        return Err("descriptor '__init__' requires a receiver".to_owned());
    };
    let mut roots = positional.to_vec();
    roots.extend(keywords.iter().map(|(_, value)| *value));
    context.with_temporary_roots(&roots, |context| {
        context.initialize_instance_storage(receiver, &positional[1..], keywords)?;
        Ok(RValue::NONE)
    })
}

pub(crate) fn invoke_builtin_constructor(
    context: &mut RimeraContext,
    type_name: &str,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if type_name == "dict" {
        if positional.len() > 1 {
            return Err(format!(
                "dict expected at most 1 argument, got {}",
                positional.len()
            ));
        }
        let dictionary = crate::operations::dictionary(context, &[], &[])?;
        let mut roots = vec![dictionary];
        roots.extend_from_slice(positional);
        roots.extend(keywords.iter().map(|(_, value)| *value));
        return context.with_temporary_roots(&roots, |context| {
            if let Some(source) = positional.first().copied() {
                dict_merge_source(context, dictionary, source)?;
            }
            for (key, value) in keywords {
                let key = crate::operations::string(context, key)?;
                context.with_temporary_roots(&[key, *value], |context| {
                    crate::operations::item_set(context, dictionary, key, *value)
                })?;
            }
            Ok(dictionary)
        });
    }

    if type_name == "slice" {
        if !keywords.is_empty() || positional.is_empty() || positional.len() > 3 {
            return Err(format!(
                "slice expected at least 1 argument, got {}",
                positional.len()
            ));
        }
        return match positional {
            [stop] => crate::operations::slice(context, None, Some(*stop), None),
            [start, stop] => crate::operations::slice(context, Some(*start), Some(*stop), None),
            [start, stop, step] => {
                crate::operations::slice(context, Some(*start), Some(*stop), Some(*step))
            }
            _ => unreachable!(),
        };
    }

    if type_name == "range" {
        if !keywords.is_empty() || positional.is_empty() || positional.len() > 3 {
            return Err(format!(
                "range expected 1 to 3 arguments, got {}",
                positional.len()
            ));
        }
        let zero = RValue::small_int(0);
        let one = RValue::small_int(1);
        return match positional {
            [stop] => crate::operations::range(context, zero, *stop, one),
            [start, stop] => crate::operations::range(context, *start, *stop, one),
            [start, stop, step] => crate::operations::range(context, *start, *stop, *step),
            _ => unreachable!(),
        };
    }

    if type_name == "int" {
        let mut base = positional.get(1).copied();
        for (name, value) in keywords {
            if name != "base" || base.replace(*value).is_some() {
                return Err(format!("int() got an unexpected keyword argument '{name}'"));
            }
        }
        if positional.len() > 2 {
            return Err(format!(
                "int() takes at most 2 arguments ({} given)",
                positional.len()
            ));
        }
        let Some(value) = positional.first().copied() else {
            if base.is_some() {
                return Err("int() missing string argument".to_owned());
            }
            return Ok(RValue::small_int(0));
        };
        if let Some(base) = base {
            let base_integer = crate::operations::index_integer(context, base)?;
            let Some(base) = base_integer.to_u32() else {
                return context
                    .raise_error("ValueError", "int() base must be >= 2 and <= 36, or 0");
            };
            let text = match context.heap.get(value) {
                Some(HeapObject::String(text)) => text.clone(),
                _ if is_byte_text_family(context, value) => {
                    let bytes = bytes_payload(context, value)?;
                    match ascii_numeric_text(&bytes) {
                        Ok(text) => text,
                        Err(message) => return context.raise_error("ValueError", message),
                    }
                }
                _ => {
                    return context.raise_error(
                        "TypeError",
                        "int() can't convert non-string with explicit base",
                    );
                }
            };
            let parsed = match parse_integer_text(&text, base) {
                Ok(value) => value,
                Err(message) => return context.raise_error("ValueError", message),
            };
            return crate::operations::store_integer(context, parsed);
        }
        if let Ok(integer) = crate::operations::integer(context, value) {
            return crate::operations::store_integer(context, integer);
        }
        if let Some(HeapObject::Float(number)) = context.heap.get(value) {
            let number = *number;
            if number.is_infinite() {
                return context
                    .raise_error("OverflowError", "cannot convert float infinity to integer");
            }
            if number.is_nan() {
                return context.raise_error("ValueError", "cannot convert float NaN to integer");
            }
            let text = format!("{:.0}", number.trunc());
            return crate::operations::store_integer(
                context,
                BigInt::parse_bytes(text.as_bytes(), 10)
                    .ok_or_else(|| "cannot convert float to integer".to_owned())?,
            );
        }
        let text = if matches!(context.heap.get(value), Some(HeapObject::MemoryView(_)))
            || is_byte_text_family(context, value)
        {
            let bytes = bytes_payload(context, value)?;
            match ascii_numeric_text(&bytes) {
                Ok(text) => Some(text),
                Err(message) => return context.raise_error("ValueError", message),
            }
        } else {
            match context.heap.get(value) {
                Some(HeapObject::String(text)) => Some(text.clone()),
                _ => None,
            }
        };
        if let Some(text) = text {
            let parsed = match parse_integer_text(&text, 10) {
                Ok(value) => value,
                Err(message) => return context.raise_error("ValueError", message),
            };
            return crate::operations::store_integer(context, parsed);
        }
        if let Some(result) = context.invoke_special_method(value, "__int__", &[])? {
            let Ok(integer) = crate::operations::integer(context, result) else {
                return context.raise_error("TypeError", "__int__ returned non-int");
            };
            return crate::operations::store_integer(context, integer);
        }
        let indexed = crate::operations::index_integer(context, value)?;
        return crate::operations::store_integer(context, indexed);
    }

    if type_name == "str" {
        if positional.len() > 3 {
            return Err(format!(
                "str() takes at most 3 arguments ({} given)",
                positional.len()
            ));
        }
        let mut encoding = positional.get(1).copied();
        let mut errors = positional.get(2).copied();
        for (name, value) in keywords {
            match name.as_str() {
                "encoding" if encoding.replace(*value).is_none() => {}
                "errors" if errors.replace(*value).is_none() => {}
                _ => return Err(format!("str() got an unexpected keyword argument '{name}'")),
            }
        }
        let Some(value) = positional.first().copied() else {
            if encoding.is_some() || errors.is_some() {
                return Err("str() missing object argument".to_owned());
            }
            return crate::operations::string(context, "");
        };
        if encoding.is_some() || errors.is_some() {
            let bytes = bytes_payload(context, value)?;
            let encoding = encoding
                .map(|value| builtin_text(context, value, "encoding"))
                .transpose()?
                .unwrap_or_else(|| "utf-8".to_owned());
            let errors = errors
                .map(|value| builtin_text(context, value, "errors"))
                .transpose()?
                .unwrap_or_else(|| "strict".to_owned());
            return crate::operations::string(context, &decode_bytes(&bytes, &encoding, &errors)?);
        }
        if matches!(context.heap.get(value), Some(HeapObject::String(_))) {
            return Ok(value);
        }
        let text = stringify(context, value)?;
        return crate::operations::string(context, &text);
    }

    if matches!(type_name, "bytes" | "bytearray") {
        if positional.len() > 3 {
            return Err(format!(
                "{type_name}() takes at most 3 arguments ({} given)",
                positional.len()
            ));
        }
        let mut encoding = positional.get(1).copied();
        let mut errors = positional.get(2).copied();
        for (name, value) in keywords {
            match name.as_str() {
                "encoding" if encoding.replace(*value).is_none() => {}
                "errors" if errors.replace(*value).is_none() => {}
                _ => {
                    return Err(format!(
                        "{type_name}() got an unexpected keyword argument '{name}'"
                    ));
                }
            }
        }
        let bytes = match positional.first().copied() {
            None => {
                if encoding.is_some() || errors.is_some() {
                    return Err(format!("{type_name}() missing string argument"));
                }
                Vec::new()
            }
            Some(value) if matches!(context.heap.get(value), Some(HeapObject::String(_))) => {
                let text = builtin_text(context, value, "string argument")?;
                let Some(encoding) = encoding else {
                    return Err("string argument without an encoding".to_owned());
                };
                let encoding = builtin_text(context, encoding, "encoding")?;
                let errors = errors
                    .map(|value| builtin_text(context, value, "errors"))
                    .transpose()?
                    .unwrap_or_else(|| "strict".to_owned());
                encode_text(&text, &encoding, &errors)?
            }
            Some(value) => {
                if encoding.is_some() || errors.is_some() {
                    return Err("encoding without a string argument".to_owned());
                }
                let count = if let Ok(count) = crate::operations::integer(context, value) {
                    Some(count)
                } else if context.special_method(value, "__index__")?.is_some() {
                    Some(crate::operations::index_integer(context, value)?)
                } else {
                    None
                };
                if let Some(count) = count {
                    if count.sign() == num_bigint::Sign::Minus {
                        return context.raise_error("ValueError", "negative count");
                    }
                    let count = count
                        .to_usize()
                        .ok_or_else(|| "byte sequence is too large".to_owned())?;
                    vec![0; count]
                } else {
                    crate::operations::byte_values(context, value)?
                }
            }
        };
        return if type_name == "bytes" {
            if positional.len() == 1
                && matches!(context.heap.get(positional[0]), Some(HeapObject::Bytes(_)))
            {
                Ok(positional[0])
            } else {
                crate::operations::bytes(context, &bytes)
            }
        } else {
            crate::operations::bytearray(context, &bytes)
        };
    }

    if type_name == "float" {
        if !keywords.is_empty() || positional.len() > 1 {
            return Err(format!(
                "float() takes at most 1 argument ({} given)",
                positional.len()
            ));
        }
        let Some(value) = positional.first().copied() else {
            return crate::operations::float(context, 0.0);
        };
        if matches!(context.heap.get(value), Some(HeapObject::Float(_))) {
            return Ok(value);
        }
        let text = if matches!(context.heap.get(value), Some(HeapObject::MemoryView(_)))
            || is_byte_text_family(context, value)
        {
            let bytes = bytes_payload(context, value)?;
            match ascii_numeric_text(&bytes) {
                Ok(text) => Some(text),
                Err(message) => return context.raise_error("ValueError", message),
            }
        } else {
            match context.heap.get(value) {
                Some(HeapObject::String(text)) => Some(text.clone()),
                _ => None,
            }
        };
        if let Some(text) = text {
            let number = match parse_float_text(&text) {
                Ok(number) => number,
                Err(_) => {
                    return context.raise_error(
                        "ValueError",
                        format!("could not convert string to float: {text:?}"),
                    );
                }
            };
            return crate::operations::float(context, number);
        }
        let number = numeric_real(context, value)?;
        return crate::operations::float(context, number);
    }

    if type_name == "complex" {
        let mut real = positional.first().copied();
        let mut imag = positional.get(1).copied();
        if positional.len() > 2 {
            return Err(format!(
                "complex() expected at most 2 arguments, got {}",
                positional.len()
            ));
        }
        for (name, value) in keywords {
            match name.as_str() {
                "real" if real.replace(*value).is_none() => {}
                "imag" if imag.replace(*value).is_none() => {}
                _ => {
                    return Err(format!(
                        "complex() got an unexpected keyword argument '{name}'"
                    ));
                }
            }
        }
        let Some(real_value) = real else {
            return crate::operations::complex(context, 0.0, 0.0);
        };
        if let Some(HeapObject::String(text)) = context.heap.get(real_value) {
            let text = text.clone();
            if imag.is_some() {
                return context.raise_error(
                    "TypeError",
                    "complex() can't take second arg if first is a string",
                );
            }
            let (real, imag) = match parse_complex_text(&text) {
                Ok(value) => value,
                Err(message) => return context.raise_error("ValueError", message),
            };
            return crate::operations::complex(context, real, imag);
        }
        let (a, b) = numeric_complex_parts(context, real_value)?;
        if let Some(imag_value) = imag {
            let (c, d) = numeric_complex_parts(context, imag_value)?;
            let imag = if b == 0.0 && c == 0.0 { c } else { b + c };
            return crate::operations::complex(context, a - d, imag);
        }
        if matches!(
            context.heap.get(real_value),
            Some(HeapObject::Complex { .. })
        ) {
            return Ok(real_value);
        }
        return crate::operations::complex(context, a, b);
    }

    if !keywords.is_empty() || positional.len() > 1 {
        return Err(format!("{type_name}() takes at most 1 argument"));
    }
    let value = positional.first().copied();
    match type_name {
        "bool" => Ok(RValue::boolean(
            value
                .map(|value| crate::operations::truthy(context, value))
                .transpose()?
                .unwrap_or(false),
        )),
        "list" | "tuple" | "set" | "frozenset" => {
            if matches!(type_name, "tuple" | "frozenset")
                && value.is_some_and(|value| {
                    matches!(
                        (type_name, context.heap.get(value)),
                        ("tuple", Some(HeapObject::Tuple(_)))
                            | ("frozenset", Some(HeapObject::FrozenSet(_)))
                    )
                })
            {
                return Ok(value.expect("checked"));
            }
            let values = value.map_or_else(
                || Ok(Vec::new()),
                |value| crate::operations::collect_iterable(context, value),
            )?;
            match type_name {
                "list" => crate::operations::list(context, &values),
                "tuple" => crate::operations::tuple(context, &values),
                "set" => crate::operations::set(context, &values),
                "frozenset" => crate::operations::frozenset(context, &values),
                _ => unreachable!(),
            }
        }
        "memoryview" => value
            .ok_or_else(|| "memoryview() missing required argument 'object' (pos 1)".to_owned())
            .and_then(|value| crate::operations::memoryview(context, value)),
        _ => Err("unsupported builtin constructor".to_owned()),
    }
}

fn attribute_name(context: &RimeraContext, value: RValue) -> Result<String, String> {
    match context.heap.get(value) {
        Some(HeapObject::String(name)) => Ok(name.clone()),
        _ => Err("attribute name must be string".to_owned()),
    }
}

fn string_list(context: &mut RimeraContext, names: &[String]) -> Result<RValue, String> {
    let mut values = Vec::with_capacity(names.len());
    for name in names {
        let value = context
            .with_temporary_roots(&values, |context| crate::operations::string(context, name))?;
        values.push(value);
    }
    context.with_temporary_roots(&values, |context| crate::operations::list(context, &values))
}

fn invoke_globals(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !positional.is_empty() || !keywords.is_empty() {
        return Err("globals() takes no arguments".to_owned());
    }
    context.initialize_kernel()?;
    context
        .globals()
        .ok_or_else(|| "module globals are unavailable".to_owned())
}

fn invoke_locals(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !positional.is_empty() || !keywords.is_empty() {
        return Err("locals() takes no arguments".to_owned());
    }
    context.current_locals()
}

fn invoke_vars(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() > 1 {
        return Err(format!(
            "vars expected at most 1 argument, got {}",
            positional.len()
        ));
    }
    if positional.is_empty() {
        return context.current_locals();
    }
    match context.attribute_get(positional[0], "__dict__") {
        Ok(value) => Ok(value),
        Err(error) if context.raised.is_some() => Err(error),
        Err(_) => context.raise_error("TypeError", "vars() argument must have __dict__ attribute"),
    }
}

fn invoke_dir(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() > 1 {
        return Err(format!(
            "dir expected at most 1 argument, got {}",
            positional.len()
        ));
    }
    if positional.is_empty() {
        let locals = context.current_locals()?;
        let mut names = context.namespace_names(locals);
        names.sort();
        names.dedup();
        return string_list(context, &names);
    }
    let receiver = positional[0];
    if let Some(method) = context.special_method(receiver, "__dir__")? {
        let result = context.with_temporary_roots(&[receiver, method], |context| {
            invoke(context, method, &[], &[])
        })?;
        let values = context.with_temporary_roots(&[receiver, result], |context| {
            crate::operations::collect_iterable(context, result)
        })?;
        let list = context
            .with_temporary_roots(&values, |context| crate::operations::list(context, &values))?;
        context.with_temporary_roots(&[receiver, result, list], |context| {
            invoke_list_sort(context, &[list], &[])
        })?;
        return Ok(list);
    }
    let mut names = context.default_dir_names(receiver)?;
    names.sort();
    string_list(context, &names)
}

fn invoke_getattr(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || !(2..=3).contains(&positional.len()) {
        return Err(format!(
            "getattr expected 2 or 3 arguments, got {}",
            positional.len()
        ));
    }
    let name = attribute_name(context, positional[1])?;
    match context.attribute_get(positional[0], &name) {
        Ok(value) => Ok(value),
        Err(_)
            if positional.len() == 3
                && context.raised.is_some_and(|raised| {
                    context.exception_type_name(raised) == Some("AttributeError")
                }) =>
        {
            context.consume_exception_type("AttributeError");
            Ok(positional[2])
        }
        Err(error)
            if positional.len() == 3
                && context.raised.is_none()
                && error.contains("has no attribute") =>
        {
            Ok(positional[2])
        }
        Err(error) if context.raised.is_some() => Err(error),
        Err(error) if error.contains("has no attribute") => {
            context.raise_error("AttributeError", error)
        }
        Err(error) => Err(error),
    }
}

fn invoke_setattr(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 3 {
        return Err(format!(
            "setattr expected 3 arguments, got {}",
            positional.len()
        ));
    }
    let name = attribute_name(context, positional[1])?;
    context.attribute_set(positional[0], &name, positional[2])?;
    Ok(RValue::NONE)
}

fn invoke_delattr(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err(format!(
            "delattr expected 2 arguments, got {}",
            positional.len()
        ));
    }
    let name = attribute_name(context, positional[1])?;
    context.attribute_delete(positional[0], &name)?;
    Ok(RValue::NONE)
}

fn invoke_hasattr(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err(format!(
            "hasattr expected 2 arguments, got {}",
            positional.len()
        ));
    }
    let name = attribute_name(context, positional[1])?;
    match context.attribute_get(positional[0], &name) {
        Ok(_) => Ok(RValue::boolean(true)),
        Err(_)
            if context.raised.is_some_and(|raised| {
                context.exception_type_name(raised) == Some("AttributeError")
            }) =>
        {
            context.consume_exception_type("AttributeError");
            Ok(RValue::boolean(false))
        }
        Err(error) if context.raised.is_none() && error.contains("has no attribute") => {
            Ok(RValue::boolean(false))
        }
        Err(error) => Err(error),
    }
}

fn is_callable_value(context: &mut RimeraContext, value: RValue) -> Result<bool, String> {
    Ok(matches!(
        context.heap.get(value),
        Some(
            HeapObject::Type(_)
                | HeapObject::Function(_)
                | HeapObject::BuiltinFunction(_)
                | HeapObject::BoundMethod(_)
                | HeapObject::PropertyMethod(_)
        )
    ) || context.has_special_method_slot(value, "__call__")?)
}

fn invoke_callable(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err(format!(
            "callable expected 1 argument, got {}",
            positional.len()
        ));
    }
    Ok(RValue::boolean(is_callable_value(context, positional[0])?))
}

fn invoke_hash(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err(format!(
            "hash() takes exactly one argument ({} given)",
            positional.len()
        ));
    }
    crate::operations::hash(context, positional[0])
}

fn invoke_repr(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err(format!(
            "repr() takes exactly one argument ({} given)",
            positional.len()
        ));
    }
    crate::operations::repr(context, positional[0])
}

fn invoke_format(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || !(1..=2).contains(&positional.len()) {
        return Err(format!(
            "format() takes from 1 to 2 positional arguments but {} were given",
            positional.len()
        ));
    }
    let spec = if positional.len() == 2 {
        positional[1]
    } else {
        crate::operations::string(context, "")?
    };
    crate::operations::format(context, positional[0], spec)
}

fn invoke_reversed(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err(format!(
            "reversed() takes exactly one argument ({} given)",
            positional.len()
        ));
    }
    crate::operations::reversed(context, positional[0])
}

fn invoke_range_lookup(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
    want_index: bool,
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        let name = if want_index {
            "range.index"
        } else {
            "range.count"
        };
        return Err(format!("{name}() takes exactly one argument"));
    }
    let position = crate::operations::range_position(context, positional[0], positional[1])?;
    if want_index {
        let Some(position) = position else {
            return context.raise_error("ValueError", "sequence.index(x): x not in range");
        };
        crate::operations::store_integer(context, position)
    } else {
        Ok(RValue::small_int(if position.is_some() { 1 } else { 0 }))
    }
}

fn invoke_slice_indices(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("slice.indices() takes exactly one argument".to_owned());
    }
    crate::operations::slice_indices(context, positional[0], positional[1])
}

fn invoke_base(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
    radix: u32,
    prefix: &str,
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("numeric base conversion takes exactly one argument".to_owned());
    }
    let value = crate::operations::index_integer(context, positional[0])?;
    let (sign, value) = if value.sign() == num_bigint::Sign::Minus {
        ("-", -value)
    } else {
        ("", value)
    };
    crate::operations::string(
        context,
        &format!("{sign}{prefix}{}", value.to_str_radix(radix)),
    )
}

fn invoke_chr(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("chr() takes exactly one argument".to_owned());
    }
    let codepoint = crate::operations::index_integer(context, positional[0])?;
    let Some(codepoint) = codepoint.to_u32() else {
        return context.raise_error("ValueError", "chr() arg not in range(0x110000)");
    };
    if (0xD800..=0xDFFF).contains(&codepoint) {
        return context.raise_error(
            "ValueError",
            "Rimera Gate 3 strings do not support lone surrogate code points",
        );
    }
    let Some(code) = char::from_u32(codepoint) else {
        return context.raise_error("ValueError", "chr() arg not in range(0x110000)");
    };
    crate::operations::string(context, &code.to_string())
}

fn invoke_ord(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("ord() takes exactly one argument".to_owned());
    }
    let text = match context.heap.get(positional[0]) {
        Some(HeapObject::String(text)) => text.clone(),
        Some(HeapObject::Bytes(bytes)) if bytes.len() == 1 => {
            return Ok(RValue::small_int(i64::from(bytes[0])));
        }
        Some(HeapObject::ByteArray(bytes)) if bytes.bytes.len() == 1 => {
            return Ok(RValue::small_int(i64::from(bytes.bytes[0])));
        }
        _ => return Err("ord() expected string of length 1".to_owned()),
    };
    let mut chars = text.chars();
    let character = chars
        .next()
        .filter(|_| chars.next().is_none())
        .ok_or_else(|| {
            format!(
                "ord() expected a character, but string of length {} found",
                text.chars().count()
            )
        })?;
    Ok(RValue::small_int(i64::from(character as u32)))
}

fn native_divmod_float_operand(
    context: &mut RimeraContext,
    value: RValue,
) -> Result<Option<f64>, String> {
    if value.tag == rimera_abi::RTag::Bool as u32 {
        return Ok(Some(if value.payload == 0 { 0.0 } else { 1.0 }));
    }
    if value.tag == rimera_abi::RTag::SmallInt as u32 {
        return Ok(Some(value.payload.cast_signed() as f64));
    }
    match context.heap.get(value) {
        Some(HeapObject::Float(value)) => Ok(Some(*value)),
        Some(HeapObject::BigInt(value)) => match value.to_f64() {
            Some(value) if value.is_finite() => Ok(Some(value)),
            _ => context.raise_error("OverflowError", "int too large to convert to float"),
        },
        _ => Ok(None),
    }
}

fn python_float_divmod(left: f64, right: f64) -> (f64, f64) {
    let mut remainder = left % right;
    let mut quotient = (left - remainder) / right;
    if remainder != 0.0 {
        if (right < 0.0) != (remainder < 0.0) {
            remainder += right;
            quotient -= 1.0;
        }
    } else {
        remainder = 0.0_f64.copysign(right);
    }
    let floor_quotient = if quotient != 0.0 {
        let mut floor = quotient.floor();
        if quotient - floor > 0.5 {
            floor += 1.0;
        }
        floor
    } else {
        0.0_f64.copysign(left / right)
    };
    (floor_quotient, remainder)
}

fn invoke_divmod(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("divmod expected 2 arguments".to_owned());
    }
    let left_is_float = matches!(context.heap.get(positional[0]), Some(HeapObject::Float(_)));
    let right_is_float = matches!(context.heap.get(positional[1]), Some(HeapObject::Float(_)));
    if left_is_float || right_is_float {
        let Some(left) = native_divmod_float_operand(context, positional[0])? else {
            return context.generic_divmod(positional[0], positional[1]);
        };
        let Some(right) = native_divmod_float_operand(context, positional[1])? else {
            return context.generic_divmod(positional[0], positional[1]);
        };
        if right == 0.0 {
            return context.raise_error("ZeroDivisionError", "float divmod()");
        }
        let (quotient, remainder) = python_float_divmod(left, right);
        let quotient = crate::operations::float(context, quotient)?;
        let remainder = context.with_temporary_roots(&[quotient], |context| {
            crate::operations::float(context, remainder)
        })?;
        return context.with_temporary_roots(&[quotient, remainder], |context| {
            crate::operations::tuple(context, &[quotient, remainder])
        });
    }
    let is_native_integer = |context: &RimeraContext, value: RValue| {
        value.tag == rimera_abi::RTag::Bool as u32
            || value.tag == rimera_abi::RTag::SmallInt as u32
            || matches!(context.heap.get(value), Some(HeapObject::BigInt(_)))
    };
    if is_native_integer(context, positional[0]) && is_native_integer(context, positional[1]) {
        let quotient = crate::operations::binary(context, 3, positional[0], positional[1])?;
        let remainder = context.with_temporary_roots(&[quotient], |context| {
            crate::operations::binary(context, 4, positional[0], positional[1])
        })?;
        return context.with_temporary_roots(&[quotient, remainder], |context| {
            crate::operations::tuple(context, &[quotient, remainder])
        });
    }
    context.generic_divmod(positional[0], positional[1])
}

fn positive_modulo(value: BigInt, modulus: &BigInt) -> BigInt {
    let remainder = value % modulus;
    if remainder.sign() == num_bigint::Sign::Minus {
        remainder + modulus
    } else {
        remainder
    }
}

fn modular_inverse(value: &BigInt, modulus: &BigInt) -> Option<BigInt> {
    let mut old_r = modulus.clone();
    let mut r = positive_modulo(value.clone(), modulus);
    let mut old_t = BigInt::from(0_u8);
    let mut t = BigInt::from(1_u8);
    while r != BigInt::from(0_u8) {
        let quotient = &old_r / &r;
        let next_r = old_r - &quotient * &r;
        old_r = r;
        r = next_r;
        let next_t = old_t - quotient * &t;
        old_t = t;
        t = next_t;
    }
    if old_r != BigInt::from(1_u8) {
        return None;
    }
    Some(positive_modulo(old_t, modulus))
}

fn invoke_pow(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if positional.len() > 3 {
        return Err("pow expected at most 3 arguments".to_owned());
    }
    let mut base = positional.first().copied();
    let mut exponent = positional.get(1).copied();
    let mut modulus = positional.get(2).copied();
    for (name, value) in keywords {
        match name.as_str() {
            "base" if base.replace(*value).is_none() => {}
            "exp" if exponent.replace(*value).is_none() => {}
            "mod" if modulus.replace(*value).is_none() => {}
            _ => return Err(format!("pow() got an unexpected keyword argument '{name}'")),
        }
    }
    let base = base.ok_or_else(|| "pow() missing required argument 'base' (pos 1)".to_owned())?;
    let exponent =
        exponent.ok_or_else(|| "pow() missing required argument 'exp' (pos 2)".to_owned())?;
    if modulus.is_none() || modulus == Some(RValue::NONE) {
        return crate::operations::binary(context, 6, base, exponent);
    }

    let modulus_value = modulus.expect("checked");
    if matches!(context.heap.get(base), Some(HeapObject::Complex { .. }))
        || matches!(context.heap.get(exponent), Some(HeapObject::Complex { .. }))
        || matches!(
            context.heap.get(modulus_value),
            Some(HeapObject::Complex { .. })
        )
    {
        return context.raise_error("ValueError", "complex modulo");
    }
    let (base_integer, exponent_integer, modulus_integer) = match (
        crate::operations::integer(context, base),
        crate::operations::integer(context, exponent),
        crate::operations::integer(context, modulus_value),
    ) {
        (Ok(base), Ok(exponent), Ok(modulus)) => (base, exponent, modulus),
        _ => return context.generic_ternary_power(base, exponent, modulus_value),
    };
    let mut exponent = exponent_integer;
    if modulus_integer == BigInt::from(0_u8) {
        return context.raise_error("ValueError", "pow() 3rd argument cannot be 0");
    }
    let negative_modulus = modulus_integer.sign() == num_bigint::Sign::Minus;
    let modulus_abs = modulus_integer.abs();
    let mut base = positive_modulo(base_integer, &modulus_abs);
    if exponent.sign() == num_bigint::Sign::Minus {
        let Some(inverse) = modular_inverse(&base, &modulus_abs) else {
            return context
                .raise_error("ValueError", "base is not invertible for the given modulus");
        };
        base = inverse;
        exponent = -exponent;
    }
    let mut result = base.modpow(&exponent, &modulus_abs);
    if negative_modulus && result != BigInt::from(0_u8) {
        result -= modulus_abs;
    }
    crate::operations::store_integer(context, result)
}

fn invoke_sum(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if positional.is_empty() || positional.len() > 2 {
        return Err("sum expected 1 or 2 arguments".to_owned());
    }
    let mut start = positional.get(1).copied();
    for (name, value) in keywords {
        if name != "start" || start.replace(*value).is_some() {
            return Err(format!("sum() got an unexpected keyword argument '{name}'"));
        }
    }
    let mut total = start.unwrap_or_else(|| RValue::small_int(0));
    match context.heap.get(total) {
        Some(HeapObject::String(_)) => {
            return Err("sum() can't sum strings [use ''.join(seq) instead]".to_owned());
        }
        Some(HeapObject::Bytes(_)) => {
            return Err("sum() can't sum bytes [use b''.join(seq) instead]".to_owned());
        }
        Some(HeapObject::ByteArray(_)) => {
            return Err("sum() can't sum bytearray [use b''.join(seq) instead]".to_owned());
        }
        _ => {}
    }
    let iterator = crate::operations::iterator_new(context, positional[0])?;
    context.with_temporary_roots(&[iterator], |context| {
        loop {
            let next = context.with_temporary_roots(&[iterator, total], |context| {
                crate::operations::iterator_next(context, iterator)
            })?;
            let Some(value) = next else {
                return Ok(total);
            };
            total = context.with_temporary_roots(&[iterator, total, value], |context| {
                crate::operations::binary(context, 0, total, value)
            })?;
        }
    })
}

fn invoke_extreme(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
    maximum: bool,
) -> Result<RValue, String> {
    if positional.is_empty() {
        return Err("min/max requires at least one argument".to_owned());
    }
    let mut key = RValue::NONE;
    let mut default = None;
    for (name, value) in keywords {
        match name.as_str() {
            "key" => key = *value,
            "default" if default.replace(*value).is_none() => {}
            _ => {
                return Err(format!(
                    "min/max got an unexpected keyword argument '{name}'"
                ));
            }
        }
    }
    if positional.len() > 1 && default.is_some() {
        let name = if maximum { "max" } else { "min" };
        return Err(format!(
            "Cannot specify a default for {name}() with multiple positional arguments"
        ));
    }
    let values = if positional.len() == 1 {
        crate::operations::collect_iterable(context, positional[0])?
    } else {
        positional.to_vec()
    };
    if values.is_empty() {
        if let Some(default) = default {
            return Ok(default);
        }
        let name = if maximum { "max" } else { "min" };
        return Err(format!("{name}() iterable argument is empty"));
    }
    let mut roots = values.clone();
    if key != RValue::NONE {
        roots.push(key);
    }
    context.with_temporary_roots(&roots, |context| {
        let mut selected = values[0];
        let mut selected_key = if key == RValue::NONE {
            selected
        } else {
            invoke(context, key, &[selected], &[])?
        };
        for value in values.iter().copied().skip(1) {
            let value_key = if key == RValue::NONE {
                value
            } else {
                context.with_temporary_roots(&[key, value, selected, selected_key], |context| {
                    invoke(context, key, &[value], &[])
                })?
            };
            let comparison = context.with_temporary_roots(
                &[selected, selected_key, value, value_key],
                |context| {
                    crate::operations::compare(
                        context,
                        if maximum { 4 } else { 2 },
                        value_key,
                        selected_key,
                    )
                },
            )?;
            if crate::operations::truthy(context, comparison)? {
                selected = value;
                selected_key = value_key;
            }
        }
        Ok(selected)
    })
}

fn invoke_dict_view(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
    kind: crate::object::DictionaryViewKind,
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("dictionary view method takes no arguments".to_owned());
    }
    crate::operations::dictionary_view(context, positional[0], kind)
}

fn dictionary_lookup(
    context: &mut RimeraContext,
    dictionary: RValue,
    key: RValue,
) -> Result<Option<RValue>, String> {
    match crate::operations::item_get(context, dictionary, key) {
        Ok(value) => Ok(Some(value)),
        Err(_) if context.consume_exception_type("KeyError") => Ok(None),
        Err(error) => Err(error),
    }
}

fn pattern_attribute_get(
    context: &mut RimeraContext,
    receiver: RValue,
    name: &str,
) -> Result<Option<RValue>, String> {
    match context.attribute_get(receiver, name) {
        Ok(value) => Ok(Some(value)),
        Err(_) if context.consume_exception_type("AttributeError") => Ok(None),
        Err(message) if context.raised.is_none() && message.contains("has no attribute") => {
            Ok(None)
        }
        Err(message) => Err(message),
    }
}

fn pattern_mapping_candidate(context: &mut RimeraContext, mapping: RValue) -> Result<bool, String> {
    Ok(match context.heap.get(mapping) {
        Some(
            HeapObject::Dictionary(_)
            | HeapObject::ValueDictionary(_)
            | HeapObject::MappingProxy(_),
        ) => true,
        Some(HeapObject::Instance(instance)) => match instance.storage {
            Some(storage) => matches!(
                context.heap.get(storage),
                Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_))
            ),
            None => pattern_attribute_get(context, mapping, "keys")?.is_some(),
        },
        _ => false,
    })
}

pub(crate) fn pattern_mapping_check(
    context: &mut RimeraContext,
    mapping: RValue,
    minimum_count: usize,
) -> Result<RValue, String> {
    context.with_temporary_roots(&[mapping], |context| {
        if !pattern_mapping_candidate(context, mapping)? {
            return Ok(RValue::boolean(false));
        }
        let length = crate::operations::length(context, mapping)?;
        let length = crate::operations::integer(context, length)?;
        if length.is_negative() {
            return context.raise_error("ValueError", "__len__() should return >= 0");
        }
        let Some(length) = length.to_usize() else {
            return context.raise_error(
                "OverflowError",
                "mapping length is too large for structural pattern matching",
            );
        };
        Ok(RValue::boolean(length >= minimum_count))
    })
}

pub(crate) fn pattern_mapping_extract(
    context: &mut RimeraContext,
    mapping: RValue,
    keys: &[RValue],
    include_rest: bool,
) -> Result<Option<RValue>, String> {
    let mut roots = Vec::with_capacity(keys.len() + 1);
    roots.push(mapping);
    roots.extend_from_slice(keys);
    context.with_temporary_roots(&roots, |context| {
        let sentinel = crate::operations::value_array(context, &[])?;
        let get_method = context.attribute_get(mapping, "get")?;
        let mut extracted = Vec::with_capacity(keys.len() + usize::from(include_rest));
        for (index, key) in keys.iter().copied().enumerate() {
            let hash = crate::operations::hash(context, key)?;
            context.with_temporary_roots(&[hash], |_| Ok::<(), String>(()))?;
            for prior in keys[..index].iter().copied() {
                let equal = crate::operations::compare(context, 0, prior, key)?;
                let duplicate = context.with_temporary_roots(&[equal], |context| {
                    crate::operations::truthy(context, equal)
                })?;
                if duplicate {
                    return context
                        .raise_error("ValueError", "mapping pattern checks duplicate key");
                }
            }
            let mut callback_roots = extracted.clone();
            callback_roots.extend([sentinel, get_method, key]);
            let value = context.with_temporary_roots(&callback_roots, |context| {
                invoke(context, get_method, &[key, sentinel], &[])
            })?;
            if value == sentinel {
                return Ok(None);
            }
            extracted.push(value);
        }

        if include_rest {
            let rest = context.with_temporary_roots(&extracted, |context| {
                crate::operations::dictionary(context, &[], &[])
            })?;
            let mut rest_roots = extracted.clone();
            rest_roots.push(rest);
            context.with_temporary_roots(&rest_roots, |context| {
                dict_merge_mapping_source(context, rest, mapping)?;
                for key in keys.iter().copied() {
                    crate::operations::item_delete(context, rest, key)?;
                }
                Ok::<(), String>(())
            })?;
            extracted.push(rest);
        }

        context
            .with_temporary_roots(&extracted, |context| {
                crate::operations::value_array(context, &extracted)
            })
            .map(Some)
    })
}

pub(crate) fn pattern_class_extract(
    context: &mut RimeraContext,
    subject: RValue,
    class: RValue,
    positional_count: usize,
    keyword_names: &[String],
) -> Result<Option<RValue>, String> {
    let (class_name, class_flags) = match context.heap.get(class) {
        Some(HeapObject::Type(object)) => (object.name.clone(), object.flags),
        _ => {
            return context.raise_error("TypeError", "called match pattern must be a type");
        }
    };
    context.with_temporary_roots(&[subject, class], |context| {
        if !context.is_instance(subject, class)? {
            return Ok(None);
        }

        let builtin_match_self = class_flags & TYPE_FLAG_BUILTIN != 0
            && matches!(
                class_name.as_str(),
                "bool"
                    | "bytearray"
                    | "bytes"
                    | "dict"
                    | "float"
                    | "frozenset"
                    | "int"
                    | "list"
                    | "set"
                    | "str"
                    | "tuple"
            );
        let mut selected_names = Vec::with_capacity(positional_count + keyword_names.len());
        let mut extracted = Vec::with_capacity(positional_count + keyword_names.len());

        if positional_count != 0 {
            match pattern_attribute_get(context, class, "__match_args__")? {
                Some(match_args) => {
                    let items = match context.heap.get(match_args) {
                        Some(HeapObject::Tuple(items)) => items.to_vec(),
                        _ => {
                            let actual = python_type_name(context, match_args)?;
                            return context.raise_error(
                                "TypeError",
                                format!(
                                    "{class_name}.__match_args__ must be a tuple (got {actual})"
                                ),
                            );
                        }
                    };
                    if positional_count > items.len() {
                        return context.raise_error(
                            "TypeError",
                            format!(
                                "{class_name}() accepts {} positional sub-patterns ({} given)",
                                items.len(), positional_count
                            ),
                        );
                    }
                    for item in items.into_iter().take(positional_count) {
                        let Some(name) = crate::operations::string_value(context, item) else {
                            let actual = python_type_name(context, item)?;
                            return context.raise_error(
                                "TypeError",
                                format!("__match_args__ elements must be strings (got {actual})"),
                            );
                        };
                        let name = name.to_owned();
                        if selected_names.contains(&name) {
                            return context.raise_error(
                                "TypeError",
                                format!(
                                    "{class_name}() got multiple sub-patterns for attribute '{name}'"
                                ),
                            );
                        }
                        let value = context.with_temporary_roots(&extracted, |context| {
                            pattern_attribute_get(context, subject, &name)
                        })?;
                        let Some(value) = value else {
                            return Ok(None);
                        };
                        selected_names.push(name);
                        extracted.push(value);
                    }
                }
                None if builtin_match_self => {
                    if positional_count > 1 {
                        return context.raise_error(
                            "TypeError",
                            format!(
                                "{class_name}() accepts 1 positional sub-pattern ({} given)",
                                positional_count
                            ),
                        );
                    }
                    extracted.push(subject);
                }
                None => {
                    return context.raise_error(
                        "TypeError",
                        format!(
                            "{class_name}() accepts 0 positional sub-patterns ({} given)",
                            positional_count
                        ),
                    );
                }
            }
        }

        for name in keyword_names {
            if selected_names.contains(name) {
                return context.raise_error(
                    "TypeError",
                    format!("{class_name}() got multiple sub-patterns for attribute '{name}'"),
                );
            }
            let value = context.with_temporary_roots(&extracted, |context| {
                pattern_attribute_get(context, subject, name)
            })?;
            let Some(value) = value else {
                return Ok(None);
            };
            selected_names.push(name.clone());
            extracted.push(value);
        }

        context
            .with_temporary_roots(&extracted, |context| {
                crate::operations::value_array(context, &extracted)
            })
            .map(Some)
    })
}

pub(crate) fn dict_merge_mapping_source(
    context: &mut RimeraContext,
    dictionary: RValue,
    source: RValue,
) -> Result<(), String> {
    if crate::operations::merge_native_dictionary_source(context, dictionary, source)? {
        return Ok(());
    }
    if let Some(HeapObject::Dictionary(entries)) = context.heap.get(source) {
        let entries = entries.entries.clone();
        for (key, value) in entries {
            let key = crate::operations::string(context, &key)?;
            context.with_temporary_roots(&[dictionary, source, key, value], |context| {
                crate::operations::item_set(context, dictionary, key, value)
            })?;
        }
        return Ok(());
    }
    let keys_method = context
        .attribute_get(source, "keys")
        .map_err(|_| "dictionary display argument after ** must be a mapping".to_owned())?;
    let keys = invoke(context, keys_method, &[], &[])?;
    let iterator = crate::operations::iterator_new(context, keys)?;
    context.with_temporary_roots(
        &[dictionary, source, keys_method, keys, iterator],
        |context| {
            while let Some(key) = crate::operations::iterator_next(context, iterator)? {
                let value = context.with_temporary_roots(&[key], |context| {
                    crate::operations::item_get(context, source, key)
                })?;
                context.with_temporary_roots(&[key, value], |context| {
                    crate::operations::item_set(context, dictionary, key, value)
                })?;
            }
            Ok(())
        },
    )
}

pub(crate) fn dict_merge_source(
    context: &mut RimeraContext,
    dictionary: RValue,
    source: RValue,
) -> Result<(), String> {
    if crate::operations::merge_native_dictionary_source(context, dictionary, source)? {
        return Ok(());
    }
    if let Some(HeapObject::Dictionary(entries)) = context.heap.get(source) {
        let entries = entries.entries.clone();
        for (key, value) in entries {
            let key = crate::operations::string(context, &key)?;
            context.with_temporary_roots(&[key, value], |context| {
                crate::operations::item_set(context, dictionary, key, value)
            })?;
        }
        return Ok(());
    }
    let keys_method = match context.attribute_get(source, "keys") {
        Ok(method) => Some(method),
        Err(error) if error.contains("has no attribute") => None,
        Err(error) => return Err(error),
    };
    if let Some(keys_method) = keys_method {
        let keys = invoke(context, keys_method, &[], &[])?;
        let iterator = crate::operations::iterator_new(context, keys)?;
        return context.with_temporary_roots(&[keys_method, keys, iterator], |context| {
            while let Some(key) = crate::operations::iterator_next(context, iterator)? {
                let value = context.with_temporary_roots(&[key], |context| {
                    crate::operations::item_get(context, source, key)
                })?;
                context.with_temporary_roots(&[key, value], |context| {
                    crate::operations::item_set(context, dictionary, key, value)
                })?;
            }
            Ok(())
        });
    }
    let iterator = crate::operations::iterator_new(context, source)?;
    context.with_temporary_roots(&[iterator], |context| {
        let mut index = 0usize;
        while let Some(entry) = crate::operations::iterator_next(context, iterator)? {
            let pair = crate::operations::collect_iterable(context, entry)?;
            if pair.len() != 2 {
                return Err(format!(
                    "dictionary update sequence element #{index} has length {}; 2 is required",
                    pair.len()
                ));
            }
            context.with_temporary_roots(&pair, |context| {
                crate::operations::item_set(context, dictionary, pair[0], pair[1])
            })?;
            index += 1;
        }
        Ok(())
    })
}

fn invoke_dict_get(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || !(2..=3).contains(&positional.len()) {
        return Err("dict.get() expected a key and optional default".to_owned());
    }
    Ok(dictionary_lookup(context, positional[0], positional[1])?
        .unwrap_or_else(|| positional.get(2).copied().unwrap_or(RValue::NONE)))
}

fn invoke_dict_setdefault(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || !(2..=3).contains(&positional.len()) {
        return Err("dict.setdefault() expected a key and optional default".to_owned());
    }
    if let Some(value) = dictionary_lookup(context, positional[0], positional[1])? {
        return Ok(value);
    }
    let default = positional.get(2).copied().unwrap_or(RValue::NONE);
    crate::operations::item_set(context, positional[0], positional[1], default)?;
    Ok(default)
}

fn invoke_dict_pop(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || !(2..=3).contains(&positional.len()) {
        return Err("dict.pop() expected a key and optional default".to_owned());
    }
    if let Some(value) =
        crate::operations::dictionary_pop_value(context, positional[0], positional[1])?
    {
        return Ok(value);
    }
    match positional.get(2).copied() {
        Some(default) => Ok(default),
        None => context.raise_error("KeyError", "dictionary key not found"),
    }
}

fn invoke_dict_popitem(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("dict.popitem() takes no arguments".to_owned());
    }
    let dictionary = positional[0];
    let pair = match context.heap.get(dictionary) {
        Some(HeapObject::ValueDictionary(values)) => values
            .table
            .snapshot()
            .last()
            .map(|(entry, pair)| (*entry, *pair)),
        Some(HeapObject::Dictionary(values)) => {
            let Some((key, value)) = values.entries.last().cloned() else {
                return Err("popitem(): dictionary is empty".to_owned());
            };
            let key = crate::operations::string(context, &key)?;
            return context.with_temporary_roots(&[key, value], |context| {
                let Some(HeapObject::Dictionary(values)) = context.heap.get_mut(dictionary) else {
                    unreachable!()
                };
                values.entries.pop();
                crate::operations::tuple(context, &[key, value])
            });
        }
        _ => return Err("dict.popitem() requires a dict".to_owned()),
    };
    let Some((entry, (key, value))) = pair else {
        return Err("popitem(): dictionary is empty".to_owned());
    };
    let Some(HeapObject::ValueDictionary(values)) = context.heap.get_mut(dictionary) else {
        unreachable!()
    };
    values.table.remove(entry);
    crate::operations::tuple(context, &[key, value])
}

fn invoke_dict_update(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if positional.is_empty() || positional.len() > 2 {
        return Err("dict.update() expected at most 1 argument".to_owned());
    }
    if let Some(source) = positional.get(1).copied() {
        dict_merge_source(context, positional[0], source)?;
    }
    for (key, value) in keywords {
        let key = crate::operations::string(context, key)?;
        crate::operations::item_set(context, positional[0], key, *value)?;
    }
    Ok(RValue::NONE)
}

fn invoke_dict_clear(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("dict.clear() takes no arguments".to_owned());
    }
    match context.heap.get_mut(positional[0]) {
        Some(HeapObject::ValueDictionary(values)) => *values = Default::default(),
        Some(HeapObject::Dictionary(values)) => values.entries.clear(),
        _ => return Err("dict.clear() requires a dict".to_owned()),
    }
    Ok(RValue::NONE)
}

fn invoke_dict_copy(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("dict.copy() takes no arguments".to_owned());
    }
    let object = match context.heap.get(positional[0]) {
        Some(HeapObject::ValueDictionary(values)) => HeapObject::ValueDictionary(values.clone()),
        Some(HeapObject::Dictionary(values)) => HeapObject::Dictionary(values.clone()),
        _ => return Err("dict.copy() requires a dict".to_owned()),
    };
    context.allocate(object)
}

fn list_values(context: &RimeraContext, receiver: RValue) -> Result<Vec<RValue>, String> {
    match context.heap.get(receiver) {
        Some(HeapObject::List(values)) => Ok(values.clone()),
        _ => Err("list method requires a list".to_owned()),
    }
}

fn invoke_list_append(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("list.append() takes exactly one argument".to_owned());
    }
    let Some(HeapObject::List(values)) = context.heap.get_mut(positional[0]) else {
        return Err("list.append() requires a list".to_owned());
    };
    values.push(positional[1]);
    Ok(RValue::NONE)
}

fn invoke_list_extend(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("list.extend() takes exactly one argument".to_owned());
    }
    let extension = crate::operations::collect_iterable(context, positional[1])?;
    let Some(HeapObject::List(values)) = context.heap.get_mut(positional[0]) else {
        return Err("list.extend() requires a list".to_owned());
    };
    values.extend(extension);
    Ok(RValue::NONE)
}

fn invoke_list_insert(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 3 {
        return Err("list.insert() takes exactly 2 arguments".to_owned());
    }
    let raw = crate::operations::index_integer(context, positional[1])?
        .to_isize()
        .unwrap_or_else(|| {
            if crate::operations::integer(context, positional[1])
                .ok()
                .is_some_and(|v| v.sign() == num_bigint::Sign::Minus)
            {
                isize::MIN
            } else {
                isize::MAX
            }
        });
    let Some(HeapObject::List(values)) = context.heap.get_mut(positional[0]) else {
        return Err("list.insert() requires a list".to_owned());
    };
    let length = values.len() as isize;
    let index = if raw < 0 {
        (length + raw).max(0)
    } else {
        raw.min(length)
    } as usize;
    values.insert(index, positional[2]);
    Ok(RValue::NONE)
}

fn invoke_list_pop(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || !(1..=2).contains(&positional.len()) {
        return Err("list.pop() expected at most 1 argument".to_owned());
    }
    let raw = positional
        .get(1)
        .copied()
        .map(|value| crate::operations::index_integer(context, value))
        .transpose()?;
    let Some(HeapObject::List(values)) = context.heap.get_mut(positional[0]) else {
        return Err("list.pop() requires a list".to_owned());
    };
    if values.is_empty() {
        return context.raise_error("IndexError", "pop from empty list");
    }
    let length = BigInt::from(values.len());
    let mut index = raw.unwrap_or_else(|| BigInt::from(-1_i8));
    if index.sign() == num_bigint::Sign::Minus {
        index += &length;
    }
    let Some(index) = index.to_usize().filter(|index| *index < values.len()) else {
        return context.raise_error("IndexError", "pop index out of range");
    };
    Ok(values.remove(index))
}

fn invoke_list_remove(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("list.remove() takes exactly one argument".to_owned());
    }
    let values = list_values(context, positional[0])?;
    let mut found = None;
    for (index, value) in values.into_iter().enumerate() {
        let equal = crate::operations::compare(context, 0, value, positional[1])?;
        if crate::operations::truthy(context, equal)? {
            found = Some(index);
            break;
        }
    }
    let Some(index) = found else {
        return context.raise_error("ValueError", "list.remove(x): x not in list");
    };
    let Some(HeapObject::List(values)) = context.heap.get_mut(positional[0]) else {
        unreachable!()
    };
    values.remove(index);
    Ok(RValue::NONE)
}

fn invoke_list_clear(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("list.clear() takes no arguments".to_owned());
    }
    let Some(HeapObject::List(values)) = context.heap.get_mut(positional[0]) else {
        return Err("list.clear() requires a list".to_owned());
    };
    values.clear();
    Ok(RValue::NONE)
}

fn invoke_list_copy(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("list.copy() takes no arguments".to_owned());
    }
    let values = list_values(context, positional[0])?;
    crate::operations::list(context, &values)
}

fn invoke_list_count(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("list.count() takes exactly one argument".to_owned());
    }
    let values = list_values(context, positional[0])?;
    let mut count = 0usize;
    for value in values {
        let equal = crate::operations::compare(context, 0, value, positional[1])?;
        if crate::operations::truthy(context, equal)? {
            count += 1;
        }
    }
    crate::operations::store_integer(context, BigInt::from(count))
}

fn invoke_list_index(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || !(2..=4).contains(&positional.len()) {
        return Err("list.index() expected value, optional start and stop".to_owned());
    }
    let values = list_values(context, positional[0])?;
    let length = BigInt::from(values.len());
    let normalize = |mut value: BigInt| {
        if value.sign() == num_bigint::Sign::Minus {
            value += &length;
        }
        value
            .max(BigInt::ZERO)
            .min(length.clone())
            .to_usize()
            .unwrap_or(values.len())
    };
    let start = positional
        .get(2)
        .copied()
        .map(|value| crate::operations::index_integer(context, value))
        .transpose()?
        .map(normalize)
        .unwrap_or(0);
    let stop = positional
        .get(3)
        .copied()
        .map(|value| crate::operations::index_integer(context, value))
        .transpose()?
        .map(normalize)
        .unwrap_or(values.len());
    for (index, value) in values.iter().copied().enumerate().take(stop).skip(start) {
        let equal = crate::operations::compare(context, 0, value, positional[1])?;
        if crate::operations::truthy(context, equal)? {
            return crate::operations::store_integer(context, BigInt::from(index));
        }
    }
    context.raise_error("ValueError", "list.index(x): x not in list")
}

fn invoke_list_reverse(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("list.reverse() takes no arguments".to_owned());
    }
    let Some(HeapObject::List(values)) = context.heap.get_mut(positional[0]) else {
        return Err("list.reverse() requires a list".to_owned());
    };
    values.reverse();
    Ok(RValue::NONE)
}

fn invoke_list_sort(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if positional.len() != 1 {
        return Err("list.sort() takes no positional arguments".to_owned());
    }
    let sorted = invoke_sorted(context, positional, keywords)?;
    let values = match context.heap.get(sorted) {
        Some(HeapObject::List(values)) => values.clone(),
        _ => return Err("sorted() returned a non-list".to_owned()),
    };
    let Some(HeapObject::List(receiver)) = context.heap.get_mut(positional[0]) else {
        return Err("list.sort() requires a list".to_owned());
    };
    *receiver = values;
    Ok(RValue::NONE)
}

fn set_output(
    context: &mut RimeraContext,
    receiver: RValue,
    values: &[RValue],
) -> Result<RValue, String> {
    if matches!(context.heap.get(receiver), Some(HeapObject::FrozenSet(_))) {
        crate::operations::frozenset(context, values)
    } else {
        crate::operations::set(context, values)
    }
}

fn invoke_set_remove(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("set.remove() takes exactly one argument".to_owned());
    }
    if !crate::operations::set_remove_value(context, positional[0], positional[1])? {
        return context.raise_error("KeyError", "set element not found");
    }
    Ok(RValue::NONE)
}

fn invoke_set_pop(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("set.pop() takes no arguments".to_owned());
    }
    let receiver = positional[0];
    let entry = match context.heap.get(receiver) {
        Some(HeapObject::Set(values)) => values.table.snapshot().first().cloned(),
        _ => return Err("set.pop() requires a set".to_owned()),
    };
    let Some((entry, value)) = entry else {
        return Err("pop from an empty set".to_owned());
    };
    let Some(HeapObject::Set(values)) = context.heap.get_mut(receiver) else {
        unreachable!()
    };
    values.table.remove(entry);
    Ok(value)
}

fn invoke_set_clear(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("set.clear() takes no arguments".to_owned());
    }
    let Some(HeapObject::Set(values)) = context.heap.get_mut(positional[0]) else {
        return Err("set.clear() requires a set".to_owned());
    };
    *values = Default::default();
    Ok(RValue::NONE)
}

fn invoke_set_copy(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("set.copy() takes no arguments".to_owned());
    }
    if matches!(
        context.heap.get(positional[0]),
        Some(HeapObject::FrozenSet(_))
    ) {
        return Ok(positional[0]);
    }
    let values = crate::operations::collect_iterable(context, positional[0])?;
    crate::operations::set(context, &values)
}

fn invoke_set_update(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.is_empty() {
        return Err("set.update() takes no keyword arguments".to_owned());
    }
    for source in positional.iter().copied().skip(1) {
        let values = crate::operations::collect_iterable(context, source)?;
        for value in values {
            crate::operations::set_add(context, positional[0], value)?;
        }
    }
    Ok(RValue::NONE)
}

fn replace_set_from_result(
    context: &mut RimeraContext,
    receiver: RValue,
    result: RValue,
) -> Result<RValue, String> {
    let mut replacement = match context.heap.get(result) {
        Some(HeapObject::Set(values)) => values.clone(),
        _ => return Err("set update operation produced a non-set result".to_owned()),
    };
    let Some(HeapObject::Set(values)) = context.heap.get_mut(receiver) else {
        return Err("set update operation requires a mutable set".to_owned());
    };
    replacement.table.version = values.table.version.wrapping_add(1);
    *values = replacement;
    Ok(RValue::NONE)
}

fn invoke_set_intersection_update(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.is_empty() {
        return Err("set.intersection_update() takes no keyword arguments".to_owned());
    }
    let result = invoke_set_intersection(context, positional, &[])?;
    replace_set_from_result(context, positional[0], result)
}

fn invoke_set_difference_update(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.is_empty() {
        return Err("set.difference_update() takes no keyword arguments".to_owned());
    }
    let result = invoke_set_difference(context, positional, &[])?;
    replace_set_from_result(context, positional[0], result)
}

fn invoke_set_symmetric_difference_update(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("set.symmetric_difference_update() takes exactly one argument".to_owned());
    }
    let result = invoke_set_symmetric_difference(context, positional, &[])?;
    replace_set_from_result(context, positional[0], result)
}

fn invoke_set_union(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.is_empty() {
        return Err("set.union() takes no keyword arguments".to_owned());
    }
    let mut values = crate::operations::collect_iterable(context, positional[0])?;
    for source in positional.iter().copied().skip(1) {
        values.extend(crate::operations::collect_iterable(context, source)?);
    }
    set_output(context, positional[0], &values)
}

fn invoke_set_intersection(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.is_empty() {
        return Err("set.intersection() takes no keyword arguments".to_owned());
    }
    let mut values = crate::operations::collect_iterable(context, positional[0])?;
    for source in positional.iter().copied().skip(1) {
        let source_values = crate::operations::collect_iterable(context, source)?;
        let source_set = crate::operations::set(context, &source_values)?;
        values = context.with_temporary_roots(&[source_set], |context| {
            values
                .iter()
                .copied()
                .filter_map(
                    |value| match crate::operations::contains(context, source_set, value) {
                        Ok(true) => Some(Ok(value)),
                        Ok(false) => None,
                        Err(error) => Some(Err(error)),
                    },
                )
                .collect::<Result<Vec<_>, _>>()
        })?;
    }
    set_output(context, positional[0], &values)
}

fn invoke_set_difference(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.is_empty() {
        return Err("set.difference() takes no keyword arguments".to_owned());
    }
    let mut values = crate::operations::collect_iterable(context, positional[0])?;
    for source in positional.iter().copied().skip(1) {
        let source_values = crate::operations::collect_iterable(context, source)?;
        let source_set = crate::operations::set(context, &source_values)?;
        values = context.with_temporary_roots(&[source_set], |context| {
            values
                .iter()
                .copied()
                .filter_map(
                    |value| match crate::operations::contains(context, source_set, value) {
                        Ok(true) => None,
                        Ok(false) => Some(Ok(value)),
                        Err(error) => Some(Err(error)),
                    },
                )
                .collect::<Result<Vec<_>, _>>()
        })?;
    }
    set_output(context, positional[0], &values)
}

fn invoke_set_symmetric_difference(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("set.symmetric_difference() takes exactly one argument".to_owned());
    }
    let left = crate::operations::collect_iterable(context, positional[0])?;
    let right = crate::operations::collect_iterable(context, positional[1])?;
    let right_set = crate::operations::set(context, &right)?;
    let mut values = context.with_temporary_roots(&[right_set], |context| {
        left.iter()
            .copied()
            .filter_map(
                |value| match crate::operations::contains(context, right_set, value) {
                    Ok(true) => None,
                    Ok(false) => Some(Ok(value)),
                    Err(error) => Some(Err(error)),
                },
            )
            .collect::<Result<Vec<_>, _>>()
    })?;
    for value in right {
        if !crate::operations::contains(context, positional[0], value)? {
            values.push(value);
        }
    }
    set_output(context, positional[0], &values)
}

#[derive(Clone, Copy)]
enum SetRelation {
    Disjoint,
    Subset,
    Superset,
}

fn invoke_set_relation(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
    relation: SetRelation,
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("set relation method takes exactly one argument".to_owned());
    }
    match relation {
        SetRelation::Disjoint => {
            let iterator = crate::operations::iterator_new(context, positional[1])?;
            context.with_temporary_roots(&[iterator], |context| {
                while let Some(value) = crate::operations::iterator_next(context, iterator)? {
                    if crate::operations::contains(context, positional[0], value)? {
                        return Ok(RValue::boolean(false));
                    }
                }
                Ok(RValue::boolean(true))
            })
        }
        SetRelation::Subset => {
            let other = crate::operations::collect_iterable(context, positional[1])?;
            let other = crate::operations::set(context, &other)?;
            context.with_temporary_roots(&[other], |context| {
                let values = crate::operations::collect_iterable(context, positional[0])?;
                for value in values {
                    if !crate::operations::contains(context, other, value)? {
                        return Ok(RValue::boolean(false));
                    }
                }
                Ok(RValue::boolean(true))
            })
        }
        SetRelation::Superset => {
            let iterator = crate::operations::iterator_new(context, positional[1])?;
            context.with_temporary_roots(&[iterator], |context| {
                while let Some(value) = crate::operations::iterator_next(context, iterator)? {
                    if !crate::operations::contains(context, positional[0], value)? {
                        return Ok(RValue::boolean(false));
                    }
                }
                Ok(RValue::boolean(true))
            })
        }
    }
}

fn invoke_memoryview_release(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("memoryview.release() takes no arguments".to_owned());
    }
    crate::operations::memoryview_release(context, positional[0])?;
    Ok(RValue::NONE)
}

fn invoke_memoryview_tobytes(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !(1..=2).contains(&positional.len()) {
        return Err("memoryview.tobytes() takes at most one argument".to_owned());
    }
    let mut order = positional.get(1).copied();
    for (name, value) in keywords {
        match name.as_str() {
            "order" if order.replace(*value).is_none() => {}
            "order" => {
                return Err("memoryview.tobytes() got multiple values for 'order'".to_owned());
            }
            _ => {
                return Err(format!(
                    "memoryview.tobytes() got an unexpected keyword argument '{name}'"
                ));
            }
        }
    }
    let order = match order {
        None | Some(RValue::NONE) => "C".to_owned(),
        Some(value) => match context.heap.get(value) {
            Some(HeapObject::String(value)) => value.clone(),
            _ => return Err("tobytes() argument 'order' must be str or None".to_owned()),
        },
    };
    crate::operations::memoryview_to_bytes_order(context, positional[0], &order)
}

fn invoke_memoryview_tolist(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("memoryview.tolist() takes no arguments".to_owned());
    }
    crate::operations::memoryview_to_list(context, positional[0])
}

fn invoke_memoryview_toreadonly(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("memoryview.toreadonly() takes no arguments".to_owned());
    }
    crate::operations::memoryview_to_readonly(context, positional[0])
}

fn invoke_memoryview_hex(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !(1..=3).contains(&positional.len()) {
        return Err("memoryview.hex() takes at most 2 arguments".to_owned());
    }
    let mut separator = positional.get(1).copied();
    let mut bytes_per_sep = positional.get(2).copied();
    for (name, value) in keywords {
        match name.as_str() {
            "sep" if separator.replace(*value).is_none() => {}
            "bytes_per_sep" if bytes_per_sep.replace(*value).is_none() => {}
            "sep" | "bytes_per_sep" => {
                return Err(format!(
                    "memoryview.hex() got multiple values for argument '{name}'"
                ));
            }
            _ => {
                return Err(format!(
                    "memoryview.hex() got an unexpected keyword argument '{name}'"
                ));
            }
        }
    }
    let separator = match separator {
        None => None,
        Some(value) => match context.heap.get(value) {
            Some(HeapObject::String(value)) if value.len() == 1 && value.is_ascii() => {
                Some(value.as_bytes()[0])
            }
            Some(HeapObject::Bytes(value)) if value.len() == 1 && value[0].is_ascii() => {
                Some(value[0])
            }
            Some(HeapObject::String(value)) if value.len() != 1 => {
                return Err("sep must be length 1.".to_owned());
            }
            Some(HeapObject::Bytes(value)) if value.len() != 1 => {
                return Err("sep must be length 1.".to_owned());
            }
            Some(HeapObject::String(_)) | Some(HeapObject::Bytes(_)) => {
                return Err("sep must be ASCII.".to_owned());
            }
            _ => return Err("sep must be str or bytes".to_owned()),
        },
    };
    let bytes_per_sep = bytes_per_sep
        .map(|value| crate::operations::index_integer(context, value).map(|value| value.to_isize()))
        .transpose()?
        .flatten()
        .unwrap_or(1);
    crate::operations::memoryview_hex(context, positional[0], separator, bytes_per_sep)
}

fn invoke_memoryview_cast(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if positional.len() > 3 || positional.is_empty() {
        return Err("memoryview.cast() takes a format and optional shape".to_owned());
    }
    let mut format = positional.get(1).copied();
    let mut shape = positional.get(2).copied();
    for (name, value) in keywords {
        match name.as_str() {
            "format" if format.replace(*value).is_none() => {}
            "shape" if shape.replace(*value).is_none() => {}
            "format" | "shape" => {
                return Err(format!(
                    "memoryview.cast() got multiple values for argument '{name}'"
                ));
            }
            _ => {
                return Err(format!(
                    "memoryview.cast() got an unexpected keyword argument '{name}'"
                ));
            }
        }
    }
    let format =
        format.ok_or_else(|| "memoryview.cast() missing required argument 'format'".to_owned())?;
    let format = match context.heap.get(format) {
        Some(HeapObject::String(value)) => value.clone(),
        _ => {
            return Err(
                "memoryview: destination format must be a native single character format"
                    .to_owned(),
            );
        }
    };
    crate::operations::memoryview_cast(context, positional[0], &format, shape)
}

fn invoke_set_add(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("set.add() takes exactly one argument".to_owned());
    }
    crate::operations::set_add(context, positional[0], positional[1])?;
    Ok(RValue::NONE)
}

fn invoke_set_discard(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("set.discard() takes exactly one argument".to_owned());
    }
    crate::operations::set_discard(context, positional[0], positional[1])?;
    Ok(RValue::NONE)
}

fn invoke_enumerate(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if positional.len() > 2 {
        return Err("enumerate expected at most 2 arguments".to_owned());
    }
    let mut iterable = positional.first().copied();
    let mut start = positional.get(1).copied();
    for (name, value) in keywords {
        match name.as_str() {
            "iterable" if iterable.replace(*value).is_none() => {}
            "start" if start.replace(*value).is_none() => {}
            _ => {
                return Err(format!(
                    "enumerate() got an unexpected keyword argument '{name}'"
                ));
            }
        }
    }
    crate::operations::enumerate(
        context,
        iterable
            .ok_or_else(|| "enumerate() missing required argument 'iterable' (pos 1)".to_owned())?,
        start.unwrap_or_else(|| RValue::small_int(0)),
    )
}

fn invoke_zip(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    let mut strict = false;
    for (name, value) in keywords {
        if name != "strict" {
            return Err(format!("zip() got an unexpected keyword argument '{name}'"));
        }
        strict = crate::operations::truthy(context, *value)?;
    }
    crate::operations::zip_with_strict(context, positional, strict)
}
fn invoke_map(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() < 2 {
        return Err("map() must have at least two arguments".to_owned());
    }
    crate::operations::map(context, positional[0], &positional[1..])
}

fn invoke_filter(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 2 {
        return Err("filter expected 2 arguments".to_owned());
    }
    crate::operations::filter(context, positional[0], positional[1])
}
fn invoke_sorted(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if positional.len() != 1 {
        return Err("sorted expected 1 argument".to_owned());
    }
    let mut reverse = false;
    let mut key = RValue::NONE;
    for (name, value) in keywords {
        match name.as_str() {
            "reverse" => reverse = crate::operations::truthy(context, *value)?,
            "key" => key = *value,
            _ => {
                return Err(format!(
                    "sorted() got an unexpected keyword argument '{name}'"
                ));
            }
        }
    }
    let mut values = crate::operations::collect_iterable(context, positional[0])?;
    context.with_temporary_roots(&values.clone(), |context| {
        let mut keys = Vec::with_capacity(values.len());
        for value in values.iter().copied() {
            let value_key = if key == RValue::NONE {
                value
            } else {
                let mut roots = values.clone();
                roots.extend(keys.iter().copied());
                roots.extend([key, value]);
                context
                    .with_temporary_roots(&roots, |context| invoke(context, key, &[value], &[]))?
            };
            keys.push(value_key);
        }
        let mut roots = values.clone();
        roots.extend(keys.iter().copied());
        context.with_temporary_roots(&roots, |context| {
            for index in 1..values.len() {
                let value = values[index];
                let value_key = keys[index];
                let mut position = index;
                while position > 0 {
                    let op = if reverse { 4 } else { 2 };
                    let comparison =
                        crate::operations::compare(context, op, value_key, keys[position - 1])?;
                    if !crate::operations::truthy(context, comparison)? {
                        break;
                    }
                    values[position] = values[position - 1];
                    keys[position] = keys[position - 1];
                    position -= 1;
                }
                values[position] = value;
                keys[position] = value_key;
            }
            crate::operations::list(context, &values)
        })
    })
}

fn invoke_id(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("id() takes exactly one argument".to_owned());
    }
    let value = positional[0];
    // Stable identity token for the complete opaque value, not a machine
    // address. Generation-bearing handles therefore cannot collide with
    // immediate values that happen to share the same payload bits.
    let identity = (BigInt::from(value.tag) << 64) | BigInt::from(value.payload);
    crate::operations::store_integer(context, identity)
}

fn invoke_ascii(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("ascii() takes exactly one argument".to_owned());
    }
    crate::operations::ascii(context, positional[0])
}
fn invoke_iter(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() {
        return Err("iter() takes no keyword arguments".to_owned());
    }
    match positional {
        [iterable] => crate::operations::iterator_new(context, *iterable),
        [callable, sentinel] => {
            if !is_callable_value(context, *callable)? {
                return Err("iter(v, w): v must be callable".to_owned());
            }
            crate::operations::call_sentinel_iterator(context, *callable, *sentinel)
        }
        _ => Err(format!(
            "iter expected 1 or 2 arguments, got {}",
            positional.len()
        )),
    }
}

fn invoke_next(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() {
        return Err("next() takes no keyword arguments".to_owned());
    }
    if !(1..=2).contains(&positional.len()) {
        return Err(format!(
            "next expected 1 or 2 arguments, got {}",
            positional.len()
        ));
    }
    if matches!(
        context.heap.get(positional[0]),
        Some(HeapObject::Generator(_))
    ) {
        return match context.resume_generator(
            positional[0],
            rimera_abi::RGeneratorOperation::Next,
            RValue::NONE,
        )? {
            crate::context::GeneratorResume {
                value,
                outcome: rimera_abi::RGeneratorOutcome::Yielded,
            } => Ok(value),
            crate::context::GeneratorResume {
                outcome: rimera_abi::RGeneratorOutcome::Returned,
                ..
            } if positional.len() == 2 => Ok(positional[1]),
            crate::context::GeneratorResume {
                value,
                outcome: rimera_abi::RGeneratorOutcome::Returned,
            } => {
                context.raise_stop_iteration(value)?;
                Err("generator exhausted".to_owned())
            }
        };
    }
    match crate::operations::iterator_next(context, positional[0])? {
        Some(value) => Ok(value),
        None if positional.len() == 2 => Ok(positional[1]),
        None => {
            context.raise_builtin("StopIteration", "")?;
            Err("iterator exhausted".to_owned())
        }
    }
}
fn invoke_type_prepare(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 3 {
        return Err("type.__prepare__() takes exactly 3 arguments".to_owned());
    }
    if !matches!(context.heap.get(positional[0]), Some(HeapObject::Type(_))) {
        return Err("type.__prepare__() argument 1 must be a type".to_owned());
    }
    if !matches!(context.heap.get(positional[1]), Some(HeapObject::String(_))) {
        return Err("type.__prepare__() argument 2 must be str".to_owned());
    }
    if !matches!(context.heap.get(positional[2]), Some(HeapObject::Tuple(_))) {
        return Err("type.__prepare__() argument 3 must be tuple".to_owned());
    }
    context.namespace_new()
}

fn invoke_property(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() > 1 {
        return Err("property() takes at most 1 argument".to_owned());
    }
    context.new_property(positional.first().copied(), None, None)
}

fn invoke_static_method(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("staticmethod() takes exactly one argument".to_owned());
    }
    context.new_static_method(positional[0])
}

fn invoke_class_method(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || positional.len() != 1 {
        return Err("classmethod() takes exactly one argument".to_owned());
    }
    context.new_class_method(positional[0])
}

fn invoke_type(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() || !(positional.len() == 1 || positional.len() == 3) {
        return Err("type() takes 1 or 3 arguments".to_owned());
    }
    if positional.len() == 1 {
        return context.type_of(positional[0]);
    }
    let name = match context.heap.get(positional[0]) {
        Some(HeapObject::String(name)) => name.clone(),
        _ => {
            return Err(format!(
                "type.__new__() argument 1 must be str, not {}",
                python_type_name(context, positional[0])?
            ));
        }
    };
    let bases = match context.heap.get(positional[1]) {
        Some(HeapObject::Tuple(bases)) => bases.to_vec(),
        _ => {
            return Err(format!(
                "type.__new__() argument 2 must be tuple, not {}",
                python_type_name(context, positional[1])?
            ));
        }
    };
    if bases
        .iter()
        .any(|base| !matches!(context.heap.get(*base), Some(HeapObject::Type(_))))
    {
        return Err("metaclass conflict: the metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all its bases".to_owned());
    }
    if !matches!(
        context.heap.get(positional[2]),
        Some(HeapObject::Dictionary(_) | HeapObject::ValueDictionary(_))
    ) {
        return Err(format!(
            "type.__new__() argument 3 must be dict, not {}",
            python_type_name(context, positional[2])?
        ));
    }
    context.new_class(&name, &bases, positional[2])
}

fn invoke_metaclass(
    context: &mut RimeraContext,
    metaclass: RValue,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if positional.len() != 3 {
        return Err("type() takes 1 or 3 arguments".to_owned());
    }
    context.invoke_metaclass_constructor(
        metaclass,
        positional[0],
        positional[1],
        positional[2],
        keywords,
    )
}

fn python_type_name(context: &mut RimeraContext, value: RValue) -> Result<String, String> {
    let type_value = context.type_of(value)?;
    match context.heap.get(type_value) {
        Some(HeapObject::Type(object)) => Ok(object.name.clone()),
        _ => Err("value has an invalid runtime type".to_owned()),
    }
}

fn invoke_isinstance(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() {
        return Err("isinstance() takes no keyword arguments".to_owned());
    }
    if positional.len() != 2 {
        return Err(format!(
            "isinstance expected 2 arguments, got {}",
            positional.len()
        ));
    }
    Ok(RValue::boolean(
        context.is_instance_reflective(positional[0], positional[1])?,
    ))
}

fn invoke_issubclass(
    context: &mut RimeraContext,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<RValue, String> {
    if !keywords.is_empty() {
        return Err("issubclass() takes no keyword arguments".to_owned());
    }
    if positional.len() != 2 {
        return Err(format!(
            "issubclass expected 2 arguments, got {}",
            positional.len()
        ));
    }
    Ok(RValue::boolean(
        context.is_subclass_reflective(positional[0], positional[1])?,
    ))
}

fn bind(
    context: &mut RimeraContext,
    function: &FunctionObject,
    code: &CodeObject,
    positional: &[RValue],
    keywords: &[(String, RValue)],
) -> Result<Vec<RValue>, String> {
    let mut bound = vec![None; code.parameters.len()];
    let fixed = code
        .parameters
        .iter()
        .enumerate()
        .filter(|(_, parameter)| {
            matches!(
                parameter.kind,
                ParameterKind::PositionalOnly | ParameterKind::PositionalOrKeyword
            )
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let varargs = code
        .parameters
        .iter()
        .position(|parameter| parameter.kind == ParameterKind::VarArgs);
    let var_keywords = code
        .parameters
        .iter()
        .position(|parameter| parameter.kind == ParameterKind::VarKeywords);

    for (position, value) in positional.iter().copied().enumerate().take(fixed.len()) {
        bound[fixed[position]] = Some(value);
    }
    let mut extra_keywords = Vec::new();
    for (name, value) in keywords {
        let matching = code
            .parameters
            .iter()
            .position(|parameter| parameter.name == *name);
        match matching {
            Some(index)
                if code.parameters[index].kind == ParameterKind::PositionalOnly
                    && var_keywords.is_none() =>
            {
                return Err(format!(
                    "{}() got some positional-only arguments passed as keyword arguments: '{}'",
                    function.name, name
                ));
            }
            Some(index)
                if matches!(
                    code.parameters[index].kind,
                    ParameterKind::PositionalOrKeyword | ParameterKind::KeywordOnly
                ) =>
            {
                if bound[index].replace(*value).is_some() {
                    return Err(format!(
                        "{}() got multiple values for argument '{name}'",
                        function.name
                    ));
                }
            }
            _ if var_keywords.is_some() => extra_keywords.push((name.clone(), *value)),
            _ => {
                return Err(format!(
                    "{}() got an unexpected keyword argument '{name}'",
                    function.name
                ));
            }
        }
    }

    if positional.len() > fixed.len() && varargs.is_none() {
        let keyword_only = code
            .parameters
            .iter()
            .enumerate()
            .filter(|(index, parameter)| {
                parameter.kind == ParameterKind::KeywordOnly && bound[*index].is_some()
            })
            .count();
        let detail = if keyword_only == 0 {
            String::new()
        } else {
            format!(
                " positional arguments (and {keyword_only} keyword-only argument{})",
                if keyword_only == 1 { "" } else { "s" }
            )
        };
        return Err(if detail.is_empty() {
            format!(
                "{}() takes {} positional argument{} but {} were given",
                function.name,
                fixed.len(),
                if fixed.len() == 1 { "" } else { "s" },
                positional.len()
            )
        } else {
            format!(
                "{}() takes {} positional argument{} but {}{} were given",
                function.name,
                fixed.len(),
                if fixed.len() == 1 { "" } else { "s" },
                positional.len(),
                detail
            )
        });
    }

    if let Some(index) = varargs {
        let extras = positional
            .get(fixed.len()..)
            .unwrap_or_default()
            .to_vec()
            .into_boxed_slice();
        bound[index] = Some(context.allocate(HeapObject::Tuple(extras))?);
    }
    if let Some(index) = var_keywords {
        let published = bound.iter().flatten().copied().collect::<Vec<_>>();
        bound[index] = Some(context.with_temporary_roots(&published, |context| {
            context.allocate(HeapObject::Dictionary(DictionaryObject {
                entries: extra_keywords,
            }))
        })?);
    }

    let positional_defaults = function
        .defaults
        .and_then(|defaults| match context.heap.get(defaults) {
            Some(HeapObject::Tuple(values)) => Some(values.to_vec()),
            _ => None,
        })
        .unwrap_or_default();
    let effective_positional_defaults = positional_defaults.len().min(fixed.len());
    let positional_defaults_start = fixed.len().saturating_sub(effective_positional_defaults);
    let positional_defaults_value_start = positional_defaults
        .len()
        .saturating_sub(effective_positional_defaults);
    for (position, parameter_index) in fixed.iter().copied().enumerate() {
        if bound[parameter_index].is_none() && position >= positional_defaults_start {
            let default_index =
                positional_defaults_value_start + position - positional_defaults_start;
            bound[parameter_index] = positional_defaults.get(default_index).copied();
        }
    }
    for (index, parameter) in code.parameters.iter().enumerate() {
        if bound[index].is_none()
            && parameter.kind == ParameterKind::KeywordOnly
            && let Some(defaults) = function.keyword_defaults
        {
            bound[index] = context.namespace_value(defaults, &parameter.name);
        }
    }

    let mut missing_positional = Vec::new();
    let mut missing_keyword_only = Vec::new();
    for (index, parameter) in code.parameters.iter().enumerate() {
        if bound[index].is_none()
            && !matches!(
                parameter.kind,
                ParameterKind::VarArgs | ParameterKind::VarKeywords
            )
        {
            if parameter.kind == ParameterKind::KeywordOnly {
                missing_keyword_only.push(parameter.name.as_str());
            } else {
                missing_positional.push(parameter.name.as_str());
            }
        }
    }
    if !missing_positional.is_empty() {
        return Err(format!(
            "{}() missing {} required positional argument{}: {}",
            function.name,
            missing_positional.len(),
            if missing_positional.len() == 1 {
                ""
            } else {
                "s"
            },
            format_name_list(&missing_positional)
        ));
    }
    if !missing_keyword_only.is_empty() {
        return Err(format!(
            "{}() missing {} required keyword-only argument{}: {}",
            function.name,
            missing_keyword_only.len(),
            if missing_keyword_only.len() == 1 {
                ""
            } else {
                "s"
            },
            format_name_list(&missing_keyword_only)
        ));
    }
    Ok(bound.into_iter().flatten().collect())
}

fn format_name_list(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [name] => format!("'{name}'"),
        [first, second] => format!("'{first}' and '{second}'"),
        _ => {
            let (last, initial) = names.split_last().expect("non-empty name list");
            format!(
                "{}, and '{}'",
                initial
                    .iter()
                    .map(|name| format!("'{name}'"))
                    .collect::<Vec<_>>()
                    .join(", "),
                last
            )
        }
    }
}
