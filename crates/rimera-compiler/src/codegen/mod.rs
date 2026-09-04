use std::collections::{BTreeMap, BTreeSet};

use crate::lir::NativeProgram;
use crate::mir::{self as mir, OperationKind, Terminator};
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{
    AbiParam, FuncRef, InstBuilder, StackSlot, StackSlotData, StackSlotKind, Value, types,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use rimera_abi::ABI_VERSION;

const VALUE_SIZE: u32 = 16;

pub fn emit_object(
    program: &NativeProgram,
    release: bool,
    heap_limit_bytes: Option<u64>,
) -> Result<Vec<u8>, String> {
    mir::verify(&program.mir)?;
    let entry = &program.mir.functions[program.mir.entry.0 as usize];
    let root_plan = mir::safepoint_plan(entry)?;
    let mut flags = settings::builder();
    flags
        .set("is_pic", "true")
        .map_err(|error| error.to_string())?;
    flags
        .set("opt_level", if release { "speed_and_size" } else { "none" })
        .map_err(|error| error.to_string())?;
    let isa = cranelift_native::builder()
        .map_err(|error| error.to_string())?
        .finish(settings::Flags::new(flags))
        .map_err(|error| error.to_string())?;
    if isa.triple().to_string() != "aarch64-apple-darwin" {
        return Err(format!(
            "native emission requires aarch64-apple-darwin, found {}",
            isa.triple()
        ));
    }
    let builder = ObjectBuilder::new(isa, "rimera", cranelift_module::default_libcall_names())
        .map_err(|error| error.to_string())?;
    let mut module = ObjectModule::new(builder);
    let pointer = module.target_config().pointer_type();
    let imports = Imports::declare(&mut module, pointer, &program.mir)?;
    let native_functions = program
        .mir
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| {
            if index == program.mir.entry.0 as usize {
                return Ok(None);
            }
            let mut signature = module.make_signature();
            if function.kind == mir::FunctionKind::Generator {
                signature.params.extend([
                    AbiParam::new(pointer),
                    AbiParam::new(pointer),
                    AbiParam::new(types::I8),
                    AbiParam::new(pointer),
                    AbiParam::new(pointer),
                    AbiParam::new(pointer),
                ]);
            } else {
                signature
                    .params
                    .extend((0..5).map(|_| AbiParam::new(pointer)));
            }
            signature.returns.push(AbiParam::new(types::I32));
            module
                .declare_function(
                    &format!("rimera_native_{index}"),
                    Linkage::Local,
                    &signature,
                )
                .map(Some)
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let data = define_constants(&mut module, &program.mir)?;
    for (index, function) in program.mir.functions.iter().enumerate() {
        let Some(function_id) = native_functions[index] else {
            continue;
        };
        let root_plan = mir::safepoint_plan(function)?;
        define_native_function(
            &mut module,
            &program.mir,
            function,
            function_id,
            &root_plan,
            pointer,
            &imports,
            &data,
            &native_functions,
            index as u32,
        )?;
    }
    define_main(
        &mut module,
        entry,
        &root_plan,
        pointer,
        &imports,
        &data,
        &program.mir,
        &native_functions,
        program.mir.entry.0,
        heap_limit_bytes,
    )?;
    Ok(module
        .finish()
        .emit()
        .map_err(|error| error.to_string())?
        .to_vec())
}

#[allow(clippy::too_many_arguments)]
fn define_generator_function(
    module: &mut ObjectModule,
    module_program: &mir::Program,
    program: &mir::Function,
    function_id: cranelift_module::FuncId,
    root_plan: &mir::SafepointPlan,
    pointer: cranelift_codegen::ir::Type,
    imports: &Imports,
    data: &ConstantData,
    native_functions: &[Option<cranelift_module::FuncId>],
    function_index: u32,
) -> Result<(), String> {
    let persistent = mir::generator_persistent_values(program)?;
    let mut slots = BTreeMap::new();
    for (index, parameter) in program.parameters.iter().enumerate() {
        slots.insert(parameter.value, index);
    }
    let mut next_slot = program.parameters.len();
    for value in persistent {
        if let std::collections::btree_map::Entry::Vacant(entry) = slots.entry(value) {
            entry.insert(next_slot);
            next_slot += 1;
        }
    }

    let mut signature = module.make_signature();
    signature.params.extend([
        AbiParam::new(pointer),
        AbiParam::new(pointer),
        AbiParam::new(types::I8),
        AbiParam::new(pointer),
        AbiParam::new(pointer),
        AbiParam::new(pointer),
    ]);
    signature.returns.push(AbiParam::new(types::I32));
    let mut context = module.make_context();
    context.func.signature = signature;
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let prologue = builder.create_block();
        builder.append_block_params_for_function_params(prologue);
        builder.switch_to_block(prologue);
        let parameters = builder.block_params(prologue).to_vec();
        let context_value = parameters[0];
        let generator = parameters[1];
        let operation = parameters[2];
        let input = parameters[3];
        let output = parameters[4];
        let outcome = parameters[5];

        let value_bytes = program.value_count.max(1) * VALUE_SIZE;
        let values_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            value_bytes,
            3,
        ));
        for offset in (0..value_bytes).step_by(8) {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().stack_store(
                zero,
                values_slot,
                i32::try_from(offset).map_err(|_| "value frame is too large")?,
            );
        }
        let root_capacity = root_plan.max_roots();
        let root_bytes = u32::try_from(root_capacity.max(1).saturating_mul(VALUE_SIZE as usize))
            .map_err(|_| "root frame is too large")?;
        let roots_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            root_bytes,
            3,
        ));
        for offset in (0..root_bytes).step_by(8) {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().stack_store(
                zero,
                roots_slot,
                i32::try_from(offset).map_err(|_| "root frame is too large")?,
            );
        }
        let frame_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 24, 3));
        let failure_line_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 4, 2));
        let function_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            VALUE_SIZE,
            3,
        ));
        let state_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 4, 2));
        let initial_line = builder.ins().iconst(types::I32, 1);
        builder
            .ins()
            .stack_store(initial_line, failure_line_slot, 0);
        let roots_pointer = builder.ins().stack_addr(pointer, roots_slot, 0);
        let null = builder.ins().iconst(pointer, 0);
        builder.ins().stack_store(null, frame_slot, 0);
        builder.ins().stack_store(roots_pointer, frame_slot, 8);
        let root_count = builder.ins().iconst(
            pointer,
            i64::try_from(root_capacity).map_err(|_| "too many GC roots")?,
        );
        builder.ins().stack_store(root_count, frame_slot, 16);
        let frame_pointer = builder.ins().stack_addr(pointer, frame_slot, 0);
        let failure = builder.create_block();
        let propagate = builder.create_block();
        let invalid_state = builder.create_block();
        let push_ref = module.declare_func_in_func(imports.roots_push, builder.func);
        emit_status_call(
            &mut builder,
            push_ref,
            &[context_value, frame_pointer],
            failure,
        );
        let function_pointer = builder.ins().stack_addr(pointer, function_slot, 0);
        let get_function =
            module.declare_func_in_func(imports.generator_function_get, builder.func);
        emit_status_call(
            &mut builder,
            get_function,
            &[context_value, generator, function_pointer],
            failure,
        );
        let state_pointer = builder.ins().stack_addr(pointer, state_slot, 0);
        let get_state = module.declare_func_in_func(imports.generator_state_get, builder.func);
        emit_status_call(
            &mut builder,
            get_state,
            &[context_value, generator, state_pointer],
            failure,
        );
        let state = builder.ins().stack_load(types::I32, state_slot, 0);
        let refs = FunctionRefs::new(module, builder.func, imports);
        let block_map = program
            .blocks
            .iter()
            .map(|_| builder.create_block())
            .collect::<Vec<_>>();

        let initial_dispatch = builder.create_block();
        let is_initial = builder.ins().icmp_imm(IntCC::Equal, state, 0);
        builder
            .ins()
            .brif(is_initial, initial_dispatch, &[], invalid_state, &[]);
        builder.switch_to_block(initial_dispatch);
        for (index, parameter) in program.parameters.iter().enumerate() {
            let slot_index = builder.ins().iconst(
                pointer,
                i64::try_from(index).map_err(|_| "generator slot index is too large")?,
            );
            let destination = value_pointer(&mut builder, pointer, values_slot, parameter.value);
            let slot_get = module.declare_func_in_func(imports.generator_slot_get, builder.func);
            emit_status_call(
                &mut builder,
                slot_get,
                &[context_value, generator, slot_index, destination],
                failure,
            );
        }
        builder.ins().jump(block_map[program.entry.0 as usize], &[]);

        let yield_states = program
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| match block.terminator {
                Terminator::Yield {
                    resume_value,
                    resume_target,
                    exception_target,
                    delegate,
                    ..
                } => Some((
                    index,
                    resume_value,
                    resume_target,
                    exception_target,
                    delegate,
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut check_state = invalid_state;
        for (yield_block, resume_value, resume_target, exception_target, delegate) in
            yield_states.iter().copied()
        {
            builder.switch_to_block(check_state);
            let dispatch = builder.create_block();
            let next_check = builder.create_block();
            let state_id =
                u32::try_from(yield_block + 1).map_err(|_| "too many generator states")?;
            let matches = builder
                .ins()
                .icmp_imm(IntCC::Equal, state, i64::from(state_id));
            builder.ins().brif(matches, dispatch, &[], next_check, &[]);
            builder.switch_to_block(dispatch);
            if let Some(roots) = root_plan.terminator_roots(yield_block) {
                for value in roots {
                    let Some(slot) = slots.get(value).copied() else {
                        continue;
                    };
                    let slot_index = builder.ins().iconst(
                        pointer,
                        i64::try_from(slot).map_err(|_| "generator slot index is too large")?,
                    );
                    let destination = value_pointer(&mut builder, pointer, values_slot, *value);
                    let slot_get =
                        module.declare_func_in_func(imports.generator_slot_get, builder.func);
                    emit_status_call(
                        &mut builder,
                        slot_get,
                        &[context_value, generator, slot_index, destination],
                        failure,
                    );
                }
            }
            if delegate.is_some() {
                let delegate_value = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    VALUE_SIZE,
                    3,
                ));
                let delegate_outcome = builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    1,
                    0,
                ));
                let delegate_value_pointer = builder.ins().stack_addr(pointer, delegate_value, 0);
                let delegate_outcome_pointer =
                    builder.ins().stack_addr(pointer, delegate_outcome, 0);
                let delegate_resume =
                    module.declare_func_in_func(imports.generator_delegate_resume, builder.func);
                emit_status_call(
                    &mut builder,
                    delegate_resume,
                    &[
                        context_value,
                        generator,
                        operation,
                        input,
                        delegate_value_pointer,
                        delegate_outcome_pointer,
                    ],
                    failure,
                );
                let delegate_outcome_value =
                    builder.ins().stack_load(types::I8, delegate_outcome, 0);
                let delegated_yield = builder.create_block();
                let delegated_not_yield = builder.create_block();
                let is_yielded = builder
                    .ins()
                    .icmp_imm(IntCC::Equal, delegate_outcome_value, 0);
                builder
                    .ins()
                    .brif(is_yielded, delegated_yield, &[], delegated_not_yield, &[]);

                builder.switch_to_block(delegated_yield);
                let first = builder.ins().stack_load(types::I64, delegate_value, 0);
                let second = builder.ins().stack_load(types::I64, delegate_value, 8);
                builder
                    .ins()
                    .store(cranelift_codegen::ir::MemFlags::trusted(), first, output, 0);
                builder.ins().store(
                    cranelift_codegen::ir::MemFlags::trusted(),
                    second,
                    output,
                    8,
                );
                let yielded = builder.ins().iconst(types::I8, 0);
                builder.ins().store(
                    cranelift_codegen::ir::MemFlags::trusted(),
                    yielded,
                    outcome,
                    0,
                );
                builder
                    .ins()
                    .call(refs.roots_pop, &[context_value, frame_pointer]);
                let ok = builder.ins().iconst(types::I32, 0);
                builder.ins().return_(&[ok]);

                builder.switch_to_block(delegated_not_yield);
                let delegated_complete = builder.create_block();
                let delegated_propagate = builder.create_block();
                let is_completed = builder
                    .ins()
                    .icmp_imm(IntCC::Equal, delegate_outcome_value, 1);
                builder.ins().brif(
                    is_completed,
                    delegated_complete,
                    &[],
                    delegated_propagate,
                    &[],
                );

                builder.switch_to_block(delegated_complete);
                if let Some(resume_value) = resume_value {
                    let destination = value_offset(resume_value);
                    let first = builder.ins().stack_load(types::I64, delegate_value, 0);
                    let second = builder.ins().stack_load(types::I64, delegate_value, 8);
                    builder.ins().stack_store(first, values_slot, destination);
                    builder
                        .ins()
                        .stack_store(second, values_slot, destination + 8);
                }
                builder.ins().jump(block_map[resume_target.0 as usize], &[]);

                builder.switch_to_block(delegated_propagate);
                let (filename, filename_len) = metadata_pointer(
                    &mut builder,
                    module,
                    data,
                    &module_program.filename,
                    pointer,
                )?;
                let (function_name, function_name_len) =
                    metadata_pointer(&mut builder, module, data, &program.name, pointer)?;
                let source_offset = program.blocks[yield_block]
                    .operations
                    .last()
                    .map_or(0, |operation| operation.span.start);
                let line = source_line(&module_program.line_starts, source_offset);
                let line = builder.ins().iconst(types::I32, i64::from(line));
                let column = builder.ins().iconst(types::I32, 0);
                builder.ins().call(
                    refs.traceback_append,
                    &[
                        context_value,
                        filename,
                        filename_len,
                        function_name,
                        function_name_len,
                        line,
                        column,
                    ],
                );
                if let Some(exception_target) = exception_target {
                    builder
                        .ins()
                        .jump(block_map[exception_target.0 as usize], &[]);
                } else {
                    builder.ins().jump(propagate, &[]);
                }
                check_state = next_check;
                continue;
            }

            let injected = builder
                .ins()
                .icmp_imm(IntCC::UnsignedGreaterThanOrEqual, operation, 2);
            let injected_block = builder.create_block();
            let normal_block = builder.create_block();
            builder
                .ins()
                .brif(injected, injected_block, &[], normal_block, &[]);

            builder.switch_to_block(normal_block);
            if let Some(resume_value) = resume_value {
                let destination = value_offset(resume_value);
                let first = builder.ins().load(
                    types::I64,
                    cranelift_codegen::ir::MemFlags::trusted(),
                    input,
                    0,
                );
                let second = builder.ins().load(
                    types::I64,
                    cranelift_codegen::ir::MemFlags::trusted(),
                    input,
                    8,
                );
                builder.ins().stack_store(first, values_slot, destination);
                builder
                    .ins()
                    .stack_store(second, values_slot, destination + 8);
            }
            builder.ins().jump(block_map[resume_target.0 as usize], &[]);

            builder.switch_to_block(injected_block);
            let (filename, filename_len) = metadata_pointer(
                &mut builder,
                module,
                data,
                &module_program.filename,
                pointer,
            )?;
            let (function_name, function_name_len) =
                metadata_pointer(&mut builder, module, data, &program.name, pointer)?;
            let source_offset = program.blocks[yield_block]
                .operations
                .last()
                .map_or(0, |operation| operation.span.start);
            let line = source_line(&module_program.line_starts, source_offset);
            let line = builder.ins().iconst(types::I32, i64::from(line));
            let column = builder.ins().iconst(types::I32, 0);
            builder.ins().call(
                refs.traceback_append,
                &[
                    context_value,
                    filename,
                    filename_len,
                    function_name,
                    function_name_len,
                    line,
                    column,
                ],
            );
            if let Some(exception_target) = exception_target {
                builder
                    .ins()
                    .jump(block_map[exception_target.0 as usize], &[]);
            } else {
                builder.ins().jump(propagate, &[]);
            }
            check_state = next_check;
        }
        builder.switch_to_block(check_state);
        let invalid = builder.ins().iconst(types::I32, 2);
        builder
            .ins()
            .call(refs.roots_pop, &[context_value, frame_pointer]);
        builder.ins().return_(&[invalid]);

        let mut traced_exception_edges = Vec::new();
        for (block_index, block) in program.blocks.iter().enumerate() {
            builder.switch_to_block(block_map[block_index]);
            for (operation_index, operation) in block.operations.iter().enumerate() {
                let source_line = source_line(&module_program.line_starts, operation.span.start);
                let line_value = builder.ins().iconst(types::I32, i64::from(source_line));
                builder.ins().stack_store(line_value, failure_line_slot, 0);
                let preserves_traceback = matches!(
                    operation.kind,
                    OperationKind::Reraise | OperationKind::Propagate
                );
                let operation_failure = if let Some(target) = program
                    .exception_edges
                    .get(&(block_index as u32, operation_index as u32))
                {
                    if preserves_traceback {
                        block_map[target.0 as usize]
                    } else {
                        let traced = builder.create_block();
                        traced_exception_edges.push((
                            traced,
                            block_map[target.0 as usize],
                            source_line,
                        ));
                        traced
                    }
                } else if preserves_traceback {
                    propagate
                } else {
                    failure
                };
                emit_operation(
                    &mut builder,
                    module,
                    imports,
                    data,
                    module_program,
                    native_functions,
                    function_index,
                    block_index as u32,
                    operation_index,
                    operation,
                    pointer,
                    values_slot,
                    roots_slot,
                    root_capacity,
                    root_plan.operation_roots(block_index, operation_index),
                    context_value,
                    Some(function_pointer),
                    operation_failure,
                )?;
            }
            emit_generator_terminator(
                &mut builder,
                module,
                imports,
                &refs,
                program,
                &block_map,
                block_index,
                &module_program.line_starts,
                &block.terminator,
                pointer,
                values_slot,
                roots_slot,
                root_capacity,
                root_plan.terminator_roots(block_index),
                &slots,
                context_value,
                generator,
                output,
                outcome,
                frame_pointer,
                failure,
            )?;
        }
        for (traced, target, line) in traced_exception_edges {
            builder.switch_to_block(traced);
            let (filename, filename_len) = metadata_pointer(
                &mut builder,
                module,
                data,
                &module_program.filename,
                pointer,
            )?;
            let (function_name, function_name_len) =
                metadata_pointer(&mut builder, module, data, &program.name, pointer)?;
            let line = builder.ins().iconst(types::I32, i64::from(line));
            let column = builder.ins().iconst(types::I32, 0);
            builder.ins().call(
                refs.traceback_append,
                &[
                    context_value,
                    filename,
                    filename_len,
                    function_name,
                    function_name_len,
                    line,
                    column,
                ],
            );
            builder.ins().jump(target, &[]);
        }

        builder.switch_to_block(propagate);
        builder
            .ins()
            .call(refs.roots_pop, &[context_value, frame_pointer]);
        let exception = builder.ins().iconst(types::I32, 1);
        builder.ins().return_(&[exception]);

        builder.switch_to_block(failure);
        let (filename, filename_len) = metadata_pointer(
            &mut builder,
            module,
            data,
            &module_program.filename,
            pointer,
        )?;
        let (function_name, function_name_len) =
            metadata_pointer(&mut builder, module, data, &program.name, pointer)?;
        let line = builder.ins().stack_load(types::I32, failure_line_slot, 0);
        let column = builder.ins().iconst(types::I32, 0);
        builder.ins().call(
            refs.traceback_append,
            &[
                context_value,
                filename,
                filename_len,
                function_name,
                function_name_len,
                line,
                column,
            ],
        );
        builder
            .ins()
            .call(refs.roots_pop, &[context_value, frame_pointer]);
        let exception = builder.ins().iconst(types::I32, 1);
        builder.ins().return_(&[exception]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    module
        .define_function(function_id, &mut context)
        .map_err(|error| error.to_string())?;
    module.clear_context(&mut context);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn define_native_function(
    module: &mut ObjectModule,
    module_program: &mir::Program,
    program: &mir::Function,
    function_id: cranelift_module::FuncId,
    root_plan: &mir::SafepointPlan,
    pointer: cranelift_codegen::ir::Type,
    imports: &Imports,
    data: &ConstantData,
    native_functions: &[Option<cranelift_module::FuncId>],
    function_index: u32,
) -> Result<(), String> {
    if program.kind == mir::FunctionKind::Generator {
        return define_generator_function(
            module,
            module_program,
            program,
            function_id,
            root_plan,
            pointer,
            imports,
            data,
            native_functions,
            function_index,
        );
    }
    let mut signature = module.make_signature();
    signature
        .params
        .extend((0..5).map(|_| AbiParam::new(pointer)));
    signature.returns.push(AbiParam::new(types::I32));
    let mut context = module.make_context();
    context.func.signature = signature;
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let prologue = builder.create_block();
        builder.append_block_params_for_function_params(prologue);
        builder.switch_to_block(prologue);
        let parameters = builder.block_params(prologue).to_vec();
        let context_value = parameters[0];
        let function_value = parameters[1];
        let bound_arguments = parameters[2];
        let bound_count = parameters[3];
        let output = parameters[4];
        let expected = builder.ins().iconst(
            pointer,
            i64::try_from(program.parameters.len()).map_err(|_| "too many parameters")?,
        );
        let count_matches = builder.ins().icmp(IntCC::Equal, bound_count, expected);
        let count_valid = builder.create_block();
        let count_invalid = builder.create_block();
        builder
            .ins()
            .brif(count_matches, count_valid, &[], count_invalid, &[]);
        builder.switch_to_block(count_invalid);
        let invalid = builder.ins().iconst(types::I32, 2);
        builder.ins().return_(&[invalid]);
        builder.switch_to_block(count_valid);

        let value_bytes = program.value_count.max(1) * VALUE_SIZE;
        let values_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            value_bytes,
            3,
        ));
        for offset in (0..value_bytes).step_by(8) {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().stack_store(
                zero,
                values_slot,
                i32::try_from(offset).map_err(|_| "value frame is too large")?,
            );
        }
        for (index, parameter) in program.parameters.iter().enumerate() {
            let offset = i32::try_from(index * VALUE_SIZE as usize)
                .map_err(|_| "bound argument offset is too large")?;
            let first = builder.ins().load(
                types::I64,
                cranelift_codegen::ir::MemFlags::trusted(),
                bound_arguments,
                offset,
            );
            let second = builder.ins().load(
                types::I64,
                cranelift_codegen::ir::MemFlags::trusted(),
                bound_arguments,
                offset + 8,
            );
            builder
                .ins()
                .stack_store(first, values_slot, value_offset(parameter.value));
            builder
                .ins()
                .stack_store(second, values_slot, value_offset(parameter.value) + 8);
        }
        let root_capacity = root_plan.max_roots();
        let root_bytes = u32::try_from(root_capacity.max(1).saturating_mul(VALUE_SIZE as usize))
            .map_err(|_| "root frame is too large")?;
        let roots_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            root_bytes,
            3,
        ));
        for offset in (0..root_bytes).step_by(8) {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().stack_store(
                zero,
                roots_slot,
                i32::try_from(offset).map_err(|_| "root frame is too large")?,
            );
        }
        let frame_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 24, 3));
        let failure_line_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 4, 2));
        let initial_line = builder.ins().iconst(types::I32, 1);
        builder
            .ins()
            .stack_store(initial_line, failure_line_slot, 0);
        let roots_pointer = builder.ins().stack_addr(pointer, roots_slot, 0);
        let null = builder.ins().iconst(pointer, 0);
        builder.ins().stack_store(null, frame_slot, 0);
        builder.ins().stack_store(roots_pointer, frame_slot, 8);
        let root_count = builder.ins().iconst(
            pointer,
            i64::try_from(root_capacity).map_err(|_| "too many GC roots")?,
        );
        builder.ins().stack_store(root_count, frame_slot, 16);
        let frame_pointer = builder.ins().stack_addr(pointer, frame_slot, 0);
        let failure = builder.create_block();
        let propagated_failure = builder.create_block();
        let push_ref = module.declare_func_in_func(imports.roots_push, builder.func);
        emit_status_call(
            &mut builder,
            push_ref,
            &[context_value, frame_pointer],
            failure,
        );
        let refs = FunctionRefs::new(module, builder.func, imports);
        let block_map = program
            .blocks
            .iter()
            .map(|_| builder.create_block())
            .collect::<Vec<_>>();
        builder.ins().jump(block_map[program.entry.0 as usize], &[]);
        let mut traced_exception_edges = Vec::new();
        for (block_index, block) in program.blocks.iter().enumerate() {
            builder.switch_to_block(block_map[block_index]);
            for (operation_index, operation) in block.operations.iter().enumerate() {
                let source_line = source_line(&module_program.line_starts, operation.span.start);
                let line_value = builder.ins().iconst(types::I32, i64::from(source_line));
                builder.ins().stack_store(line_value, failure_line_slot, 0);
                let preserves_traceback = matches!(
                    operation.kind,
                    OperationKind::Reraise | OperationKind::Propagate
                );
                let operation_failure = if let Some(target) = program
                    .exception_edges
                    .get(&(block_index as u32, operation_index as u32))
                {
                    if preserves_traceback {
                        block_map[target.0 as usize]
                    } else {
                        let traced = builder.create_block();
                        traced_exception_edges.push((
                            traced,
                            block_map[target.0 as usize],
                            source_line,
                        ));
                        traced
                    }
                } else if preserves_traceback {
                    propagated_failure
                } else {
                    failure
                };
                emit_operation(
                    &mut builder,
                    module,
                    imports,
                    data,
                    module_program,
                    native_functions,
                    function_index,
                    block_index as u32,
                    operation_index,
                    operation,
                    pointer,
                    values_slot,
                    roots_slot,
                    root_capacity,
                    root_plan.operation_roots(block_index, operation_index),
                    context_value,
                    Some(function_value),
                    operation_failure,
                )?;
            }
            emit_terminator(
                &mut builder,
                &refs,
                program,
                &block_map,
                &block.terminator,
                pointer,
                values_slot,
                roots_slot,
                root_capacity,
                root_plan.terminator_roots(block_index),
                context_value,
                frame_pointer,
                ReturnMode::Native { output },
                failure,
            )?;
        }
        if !matches!(
            program.name.as_str(),
            "<listcomp>" | "<setcomp>" | "<dictcomp>"
        ) {
            for (traced, target, line) in traced_exception_edges {
                builder.switch_to_block(traced);
                let (filename, filename_len) = metadata_pointer(
                    &mut builder,
                    module,
                    data,
                    &module_program.filename,
                    pointer,
                )?;
                let (function_name, function_name_len) =
                    metadata_pointer(&mut builder, module, data, &program.name, pointer)?;
                let line = builder.ins().iconst(types::I32, i64::from(line));
                let column = builder.ins().iconst(types::I32, 0);
                builder.ins().call(
                    refs.traceback_append,
                    &[
                        context_value,
                        filename,
                        filename_len,
                        function_name,
                        function_name_len,
                        line,
                        column,
                    ],
                );
                builder.ins().jump(target, &[]);
            }
        }
        builder.switch_to_block(failure);
        let traceback_transparent_comprehension = matches!(
            program.name.as_str(),
            "<listcomp>" | "<setcomp>" | "<dictcomp>"
        );
        if !traceback_transparent_comprehension {
            let (filename, filename_len) = metadata_pointer(
                &mut builder,
                module,
                data,
                &module_program.filename,
                pointer,
            )?;
            let (function_name, function_name_len) =
                metadata_pointer(&mut builder, module, data, &program.name, pointer)?;
            let line = builder.ins().stack_load(types::I32, failure_line_slot, 0);
            let column = builder.ins().iconst(types::I32, 0);
            builder.ins().call(
                refs.traceback_append,
                &[
                    context_value,
                    filename,
                    filename_len,
                    function_name,
                    function_name_len,
                    line,
                    column,
                ],
            );
        }
        builder
            .ins()
            .call(refs.roots_pop, &[context_value, frame_pointer]);
        let exception = builder.ins().iconst(types::I32, 1);
        builder.ins().return_(&[exception]);
        builder.switch_to_block(propagated_failure);
        builder
            .ins()
            .call(refs.roots_pop, &[context_value, frame_pointer]);
        let exception = builder.ins().iconst(types::I32, 1);
        builder.ins().return_(&[exception]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    module
        .define_function(function_id, &mut context)
        .map_err(|error| error.to_string())?;
    module.clear_context(&mut context);
    Ok(())
}

fn required_runtime_imports(program: &mir::Program) -> BTreeSet<&'static str> {
    let mut required = BTreeSet::from([
        "rimera_context_set_heap_limit",
        "rimera_kernel_initialize",
        "rimera_context_free",
        "rimera_roots_push",
        "rimera_roots_pop",
        "rimera_traceback_append_module",
        "rimera_render_error",
    ]);
    if program.functions.len() > 1 {
        required.insert("rimera_traceback_append");
    }
    for function in &program.functions {
        if function.kind == mir::FunctionKind::Generator {
            required.extend([
                "rimera_generator_function_new",
                "rimera_generator_function_get",
                "rimera_generator_state_get",
                "rimera_generator_state_set",
                "rimera_generator_frame_line_set",
                "rimera_generator_slot_get",
                "rimera_generator_slot_set",
            ]);
        }
        for block in &function.blocks {
            if matches!(block.terminator, Terminator::Branch { .. }) {
                required.insert("rimera_truthy");
            }
            if matches!(
                block.terminator,
                Terminator::Yield {
                    delegate: Some(_),
                    ..
                }
            ) {
                required.insert("rimera_generator_delegate_set");
                required.insert("rimera_generator_delegate_resume");
            }
            for operation in &block.operations {
                let name = match &operation.kind {
                    OperationKind::Constant { value, .. } => match value {
                        mir::Constant::Int(_) => Some("rimera_int_from_decimal"),
                        mir::Constant::Float(_) => Some("rimera_float_new"),
                        mir::Constant::String(_) => Some("rimera_string_new"),
                        mir::Constant::Bytes(_) => Some("rimera_bytes_new"),
                        mir::Constant::Complex { .. } => Some("rimera_complex_new"),
                        mir::Constant::None | mir::Constant::Bool(_) => None,
                    },
                    OperationKind::Copy { .. } | OperationKind::CallModuleChunk { .. } => None,
                    OperationKind::Unary { .. } => Some("rimera_unary"),
                    OperationKind::Binary { .. } => Some("rimera_binary"),
                    OperationKind::InPlace { .. } => Some("rimera_inplace"),
                    OperationKind::Compare { .. } => Some("rimera_compare"),
                    OperationKind::FormatValue { .. } => Some("rimera_format_value"),
                    OperationKind::ValueArray { .. } => Some("rimera_value_array_new"),
                    OperationKind::Tuple { .. } => Some("rimera_tuple_new"),
                    OperationKind::List { .. } => Some("rimera_list_new"),
                    OperationKind::ListAppend { .. } => Some("rimera_list_append"),
                    OperationKind::Dictionary { .. } => Some("rimera_dict_new"),
                    OperationKind::DictionaryMerge { .. } => Some("rimera_dictionary_merge"),
                    OperationKind::DictionaryInsert { .. } => Some("rimera_dictionary_insert"),
                    OperationKind::Set { .. } => Some("rimera_set_new"),
                    OperationKind::SetInsert { .. } => Some("rimera_set_insert"),
                    OperationKind::SliceNew { .. } => Some("rimera_slice_new"),
                    OperationKind::Contains { .. } => Some("rimera_contains"),
                    OperationKind::Unpack { .. } => Some("rimera_unpack_ex"),
                    OperationKind::ItemGet { .. } => Some("rimera_item_get"),
                    OperationKind::Length { .. } => Some("rimera_length"),
                    OperationKind::IteratorNew { .. } => Some("rimera_iterator_new"),
                    OperationKind::IteratorNext { .. } => Some("rimera_iterator_next"),
                    OperationKind::YieldFromNext { .. } => Some("rimera_generator_delegate_start"),
                    OperationKind::Range { .. } => Some("rimera_range_new"),
                    OperationKind::ItemSet { .. } => Some("rimera_item_set"),
                    OperationKind::ItemDelete { .. } => Some("rimera_item_delete"),
                    OperationKind::ValueArrayGet { .. } => Some("rimera_value_array_get"),
                    OperationKind::PatternSequence { .. } => Some("rimera_pattern_sequence"),
                    OperationKind::PatternMappingCheck { .. } => {
                        Some("rimera_pattern_mapping_check")
                    }
                    OperationKind::PatternMapping { .. } => Some("rimera_pattern_mapping"),
                    OperationKind::PatternClass { .. } => Some("rimera_pattern_class"),
                    OperationKind::MakeFunction { .. } => Some("rimera_function_new"),
                    OperationKind::TypeParameterNew { .. } => Some("rimera_type_parameter_new"),
                    OperationKind::TypeAliasNew { .. } => Some("rimera_type_alias_new"),
                    OperationKind::ClassNamespaceNew { .. } => Some("rimera_namespace_new"),
                    OperationKind::AnnotationsEnsure { .. } => Some("rimera_annotations_ensure"),
                    OperationKind::ClassNamespaceSet { .. } => Some("rimera_namespace_set"),
                    OperationKind::ClassNamespaceGet { .. } => Some("rimera_namespace_get"),
                    OperationKind::ClassNameGet { .. } => Some("rimera_class_name_get"),
                    OperationKind::ClassFreeGet { .. } => Some("rimera_class_free_get"),
                    OperationKind::ClassNamespaceDelete { .. } => Some("rimera_namespace_delete"),
                    OperationKind::ClassNew { .. } => Some("rimera_class_new"),
                    OperationKind::AttributeGet { .. } => Some("rimera_attr_get"),
                    OperationKind::AttributeSet { .. } => Some("rimera_attr_set"),
                    OperationKind::AttributeDelete { .. } => Some("rimera_attr_delete"),
                    OperationKind::Call { .. } => Some("rimera_call"),
                    OperationKind::CallArgumentsNew { .. } => Some("rimera_call_arguments_new"),
                    OperationKind::CallArgumentAdd { .. } => Some("rimera_call_argument_add"),
                    OperationKind::CallPrepared { .. } => Some("rimera_call_prepared"),
                    OperationKind::CellNew { .. } => Some("rimera_cell_new"),
                    OperationKind::ReflectionScopeConfigure { .. } => {
                        Some("rimera_reflection_scope_configure")
                    }
                    OperationKind::ReflectionLocalRegister { .. } => {
                        Some("rimera_reflection_local_register")
                    }
                    OperationKind::CellGet { name, .. } => Some(if name.is_some() {
                        "rimera_cell_get_named"
                    } else {
                        "rimera_cell_get"
                    }),
                    OperationKind::ClosureGet { .. } => Some("rimera_function_closure_get"),
                    OperationKind::CellSet { .. } => Some("rimera_cell_set"),
                    OperationKind::CellClear { .. } => Some("rimera_cell_clear"),
                    OperationKind::ImportName { .. } => Some("rimera_import_name"),
                    OperationKind::GlobalGet { .. } => Some("rimera_global_get"),
                    OperationKind::GlobalSet { .. } => Some("rimera_global_set"),
                    OperationKind::GlobalDelete { .. } => Some("rimera_global_delete"),
                    OperationKind::ExceptionActive { .. } => Some("rimera_exception_active"),
                    OperationKind::ExceptionMatches { .. } => Some("rimera_exception_matches"),
                    OperationKind::ExceptionSplit { .. } => Some("rimera_exception_split"),
                    OperationKind::ExceptionSetActive { .. } => Some("rimera_exception_set_active"),
                    OperationKind::ExceptionMerge { .. } => Some("rimera_exception_merge_active"),
                    OperationKind::ExceptionCombine { .. } => Some("rimera_exception_combine"),
                    OperationKind::ExceptionClearActive => Some("rimera_exception_clear_active"),
                    OperationKind::HandlerEnter { .. } => Some("rimera_handler_enter"),
                    OperationKind::HandlerLeave => Some("rimera_handler_leave"),
                    OperationKind::Raise { .. } => Some("rimera_raise"),
                    OperationKind::Reraise => Some("rimera_reraise"),
                    OperationKind::Propagate => Some("rimera_exception_propagate"),
                    OperationKind::Print { .. } => Some("rimera_print"),
                    OperationKind::PrintLiteral { .. } => Some("rimera_print_literal"),
                    OperationKind::Collect => Some("rimera_collect"),
                };
                if let Some(name) = name {
                    required.insert(name);
                }
            }
        }
    }
    required
}

struct Imports {
    context_new: cranelift_module::FuncId,
    context_set_heap_limit: cranelift_module::FuncId,
    kernel_initialize: cranelift_module::FuncId,
    context_free: cranelift_module::FuncId,
    roots_push: cranelift_module::FuncId,
    roots_pop: cranelift_module::FuncId,
    int_from_decimal: cranelift_module::FuncId,
    float_new: cranelift_module::FuncId,
    complex_new: cranelift_module::FuncId,
    string_new: cranelift_module::FuncId,
    bytes_new: cranelift_module::FuncId,
    slice_new: cranelift_module::FuncId,
    value_array_new: cranelift_module::FuncId,
    tuple_new: cranelift_module::FuncId,
    list_new: cranelift_module::FuncId,
    list_append: cranelift_module::FuncId,
    dict_new: cranelift_module::FuncId,
    dict_merge: cranelift_module::FuncId,
    dict_insert: cranelift_module::FuncId,
    set_new: cranelift_module::FuncId,
    set_insert: cranelift_module::FuncId,
    contains: cranelift_module::FuncId,
    unpack: cranelift_module::FuncId,
    item_get: cranelift_module::FuncId,
    item_set: cranelift_module::FuncId,
    item_delete: cranelift_module::FuncId,
    length: cranelift_module::FuncId,
    range_new: cranelift_module::FuncId,
    iterator_new: cranelift_module::FuncId,
    iterator_next: cranelift_module::FuncId,
    value_array_get: cranelift_module::FuncId,
    pattern_sequence: cranelift_module::FuncId,
    pattern_mapping_check: cranelift_module::FuncId,
    pattern_mapping: cranelift_module::FuncId,
    pattern_class: cranelift_module::FuncId,
    function_new: cranelift_module::FuncId,
    type_parameter_new: cranelift_module::FuncId,
    type_alias_new: cranelift_module::FuncId,
    generator_function_new: cranelift_module::FuncId,
    generator_function_get: cranelift_module::FuncId,
    generator_state_get: cranelift_module::FuncId,
    generator_state_set: cranelift_module::FuncId,
    generator_frame_line_set: cranelift_module::FuncId,
    generator_slot_get: cranelift_module::FuncId,
    generator_slot_set: cranelift_module::FuncId,
    generator_delegate_start: cranelift_module::FuncId,
    generator_delegate_set: cranelift_module::FuncId,
    generator_delegate_resume: cranelift_module::FuncId,
    namespace_new: cranelift_module::FuncId,
    namespace_set: cranelift_module::FuncId,
    namespace_get: cranelift_module::FuncId,
    class_name_get: cranelift_module::FuncId,
    class_free_get: cranelift_module::FuncId,
    namespace_delete: cranelift_module::FuncId,
    class_new: cranelift_module::FuncId,
    attr_get: cranelift_module::FuncId,
    attr_set: cranelift_module::FuncId,
    attr_delete: cranelift_module::FuncId,
    call: cranelift_module::FuncId,
    call_arguments_new: cranelift_module::FuncId,
    call_argument_add: cranelift_module::FuncId,
    call_prepared: cranelift_module::FuncId,
    cell_new: cranelift_module::FuncId,
    reflection_scope_configure: cranelift_module::FuncId,
    reflection_local_register: cranelift_module::FuncId,
    cell_get: cranelift_module::FuncId,
    cell_get_named: cranelift_module::FuncId,
    cell_set: cranelift_module::FuncId,
    cell_clear: cranelift_module::FuncId,
    closure_get: cranelift_module::FuncId,
    import_name: cranelift_module::FuncId,
    global_get: cranelift_module::FuncId,
    global_set: cranelift_module::FuncId,
    global_delete: cranelift_module::FuncId,
    exception_active: cranelift_module::FuncId,
    exception_matches: cranelift_module::FuncId,
    exception_split: cranelift_module::FuncId,
    exception_set_active: cranelift_module::FuncId,
    exception_merge: cranelift_module::FuncId,
    exception_combine: cranelift_module::FuncId,
    exception_clear_active: cranelift_module::FuncId,
    handler_enter: cranelift_module::FuncId,
    handler_leave: cranelift_module::FuncId,
    raise: cranelift_module::FuncId,
    reraise: cranelift_module::FuncId,
    propagate: cranelift_module::FuncId,
    traceback_append: cranelift_module::FuncId,
    traceback_append_module: cranelift_module::FuncId,
    unary: cranelift_module::FuncId,
    binary: cranelift_module::FuncId,
    inplace: cranelift_module::FuncId,
    compare: cranelift_module::FuncId,
    format_value: cranelift_module::FuncId,
    annotations_ensure: cranelift_module::FuncId,
    truthy: cranelift_module::FuncId,
    print: cranelift_module::FuncId,
    print_literal: cranelift_module::FuncId,
    collect: cranelift_module::FuncId,
    render_error: cranelift_module::FuncId,
}

impl Imports {
    fn declare(
        module: &mut ObjectModule,
        pointer: cranelift_codegen::ir::Type,
        program: &mir::Program,
    ) -> Result<Self, String> {
        let required = required_runtime_imports(program);
        fn raw_declaration(
            module: &mut ObjectModule,
            name: &str,
            parameters: &[cranelift_codegen::ir::Type],
            returns_status: bool,
        ) -> Result<cranelift_module::FuncId, String> {
            let mut signature = module.make_signature();
            signature
                .params
                .extend(parameters.iter().copied().map(AbiParam::new));
            if returns_status {
                signature.returns.push(AbiParam::new(types::I32));
            }
            module
                .declare_function(name, Linkage::Import, &signature)
                .map_err(|error| error.to_string())
        }
        let context_new =
            raw_declaration(module, "rimera_context_new", &[types::I32, pointer], true)?;
        let declaration = |module: &mut ObjectModule,
                           name: &str,
                           parameters: &[cranelift_codegen::ir::Type],
                           returns_status: bool|
         -> Result<cranelift_module::FuncId, String> {
            if required.contains(name) {
                raw_declaration(module, name, parameters, returns_status)
            } else {
                Ok(context_new)
            }
        };
        Ok(Self {
            context_new,
            context_set_heap_limit: declaration(
                module,
                "rimera_context_set_heap_limit",
                &[pointer, pointer],
                true,
            )?,
            kernel_initialize: declaration(module, "rimera_kernel_initialize", &[pointer], true)?,
            context_free: declaration(module, "rimera_context_free", &[pointer], false)?,
            roots_push: declaration(module, "rimera_roots_push", &[pointer, pointer], true)?,
            roots_pop: declaration(module, "rimera_roots_pop", &[pointer, pointer], true)?,
            int_from_decimal: declaration(
                module,
                "rimera_int_from_decimal",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            float_new: declaration(
                module,
                "rimera_float_new",
                &[pointer, types::I64, pointer],
                true,
            )?,
            complex_new: declaration(
                module,
                "rimera_complex_new",
                &[pointer, types::I64, types::I64, pointer],
                true,
            )?,
            string_new: declaration(
                module,
                "rimera_string_new",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            bytes_new: declaration(
                module,
                "rimera_bytes_new",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            slice_new: declaration(
                module,
                "rimera_slice_new",
                &[pointer, pointer, pointer, pointer, pointer],
                true,
            )?,
            value_array_new: declaration(
                module,
                "rimera_value_array_new",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            tuple_new: declaration(
                module,
                "rimera_tuple_new",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            list_new: declaration(
                module,
                "rimera_list_new",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            list_append: declaration(
                module,
                "rimera_list_append",
                &[pointer, pointer, pointer],
                true,
            )?,
            dict_new: declaration(
                module,
                "rimera_dict_new",
                &[pointer, pointer, pointer, pointer, pointer],
                true,
            )?,
            dict_merge: declaration(
                module,
                "rimera_dictionary_merge",
                &[pointer, pointer, pointer],
                true,
            )?,
            dict_insert: declaration(
                module,
                "rimera_dictionary_insert",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            set_new: declaration(
                module,
                "rimera_set_new",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            set_insert: declaration(
                module,
                "rimera_set_insert",
                &[pointer, pointer, pointer],
                true,
            )?,
            contains: declaration(
                module,
                "rimera_contains",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            unpack: declaration(
                module,
                "rimera_unpack_ex",
                &[pointer, pointer, pointer, pointer, types::I8, pointer],
                true,
            )?,
            item_get: declaration(
                module,
                "rimera_item_get",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            item_set: declaration(
                module,
                "rimera_item_set",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            item_delete: declaration(
                module,
                "rimera_item_delete",
                &[pointer, pointer, pointer],
                true,
            )?,
            length: declaration(module, "rimera_length", &[pointer, pointer, pointer], true)?,
            range_new: declaration(
                module,
                "rimera_range_new",
                &[pointer, pointer, pointer, pointer, pointer],
                true,
            )?,
            iterator_new: declaration(
                module,
                "rimera_iterator_new",
                &[pointer, pointer, pointer],
                true,
            )?,
            iterator_next: declaration(
                module,
                "rimera_iterator_next",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            generator_delegate_start: declaration(
                module,
                "rimera_generator_delegate_start",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            generator_delegate_set: declaration(
                module,
                "rimera_generator_delegate_set",
                &[pointer, pointer, pointer],
                true,
            )?,
            generator_delegate_resume: declaration(
                module,
                "rimera_generator_delegate_resume",
                &[pointer, pointer, types::I8, pointer, pointer, pointer],
                true,
            )?,
            value_array_get: declaration(
                module,
                "rimera_value_array_get",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            pattern_sequence: declaration(
                module,
                "rimera_pattern_sequence",
                &[
                    pointer,
                    pointer,
                    pointer,
                    pointer,
                    types::I8,
                    pointer,
                    pointer,
                ],
                true,
            )?,
            pattern_mapping_check: declaration(
                module,
                "rimera_pattern_mapping_check",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            pattern_mapping: declaration(
                module,
                "rimera_pattern_mapping",
                &[
                    pointer,
                    pointer,
                    pointer,
                    pointer,
                    types::I8,
                    pointer,
                    pointer,
                ],
                true,
            )?,
            pattern_class: declaration(
                module,
                "rimera_pattern_class",
                &[
                    pointer, pointer, pointer, pointer, pointer, pointer, pointer, pointer,
                ],
                true,
            )?,
            function_new: declaration(
                module,
                "rimera_function_new",
                &[
                    pointer, pointer, pointer, pointer, pointer, pointer, pointer, pointer,
                    pointer, pointer, pointer, pointer,
                ],
                true,
            )?,
            type_parameter_new: declaration(
                module,
                "rimera_type_parameter_new",
                &[pointer, types::I8, pointer, pointer, pointer],
                true,
            )?,
            type_alias_new: declaration(
                module,
                "rimera_type_alias_new",
                &[pointer, pointer, pointer, pointer, pointer, pointer],
                true,
            )?,
            generator_function_new: declaration(
                module,
                "rimera_generator_function_new",
                &[
                    pointer, pointer, pointer, pointer, pointer, pointer, pointer, pointer,
                    pointer, pointer, pointer, pointer, pointer,
                ],
                true,
            )?,
            generator_function_get: declaration(
                module,
                "rimera_generator_function_get",
                &[pointer, pointer, pointer],
                true,
            )?,
            generator_state_get: declaration(
                module,
                "rimera_generator_state_get",
                &[pointer, pointer, pointer],
                true,
            )?,
            generator_state_set: declaration(
                module,
                "rimera_generator_state_set",
                &[pointer, pointer, types::I32],
                true,
            )?,
            generator_frame_line_set: declaration(
                module,
                "rimera_generator_frame_line_set",
                &[pointer, pointer, types::I32],
                true,
            )?,
            generator_slot_get: declaration(
                module,
                "rimera_generator_slot_get",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            generator_slot_set: declaration(
                module,
                "rimera_generator_slot_set",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            namespace_new: declaration(module, "rimera_namespace_new", &[pointer, pointer], true)?,
            annotations_ensure: declaration(
                module,
                "rimera_annotations_ensure",
                &[pointer, pointer],
                true,
            )?,
            namespace_set: declaration(
                module,
                "rimera_namespace_set",
                &[pointer, pointer, pointer, pointer, pointer],
                true,
            )?,
            namespace_get: declaration(
                module,
                "rimera_namespace_get",
                &[pointer, pointer, pointer, pointer, pointer],
                true,
            )?,
            class_name_get: declaration(
                module,
                "rimera_class_name_get",
                &[pointer, pointer, pointer, pointer, pointer],
                true,
            )?,
            class_free_get: declaration(
                module,
                "rimera_class_free_get",
                &[pointer, pointer, pointer, pointer, pointer, pointer],
                true,
            )?,
            namespace_delete: declaration(
                module,
                "rimera_namespace_delete",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            class_new: declaration(
                module,
                "rimera_class_new",
                &[
                    pointer, pointer, pointer, pointer, pointer, pointer, pointer,
                ],
                true,
            )?,
            attr_get: declaration(
                module,
                "rimera_attr_get",
                &[pointer, pointer, pointer, pointer, pointer],
                true,
            )?,
            attr_set: declaration(
                module,
                "rimera_attr_set",
                &[pointer, pointer, pointer, pointer, pointer],
                true,
            )?,
            attr_delete: declaration(
                module,
                "rimera_attr_delete",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            call: declaration(
                module,
                "rimera_call",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            call_arguments_new: declaration(
                module,
                "rimera_call_arguments_new",
                &[pointer, pointer, pointer],
                true,
            )?,
            call_argument_add: declaration(
                module,
                "rimera_call_argument_add",
                &[pointer, pointer, types::I8, pointer, pointer, pointer],
                true,
            )?,
            call_prepared: declaration(
                module,
                "rimera_call_prepared",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            cell_new: declaration(
                module,
                "rimera_cell_new",
                &[pointer, pointer, pointer],
                true,
            )?,
            reflection_scope_configure: declaration(
                module,
                "rimera_reflection_scope_configure",
                &[pointer, pointer, types::I8],
                true,
            )?,
            reflection_local_register: declaration(
                module,
                "rimera_reflection_local_register",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            cell_get: declaration(
                module,
                "rimera_cell_get",
                &[pointer, pointer, pointer],
                true,
            )?,
            cell_get_named: declaration(
                module,
                "rimera_cell_get_named",
                &[pointer, pointer, pointer, pointer, types::I8, pointer],
                true,
            )?,
            cell_set: declaration(
                module,
                "rimera_cell_set",
                &[pointer, pointer, pointer],
                true,
            )?,
            cell_clear: declaration(module, "rimera_cell_clear", &[pointer, pointer], true)?,
            closure_get: declaration(
                module,
                "rimera_function_closure_get",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            import_name: declaration(
                module,
                "rimera_import_name",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            global_get: declaration(
                module,
                "rimera_global_get",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            global_set: declaration(
                module,
                "rimera_global_set",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            global_delete: declaration(
                module,
                "rimera_global_delete",
                &[pointer, pointer, pointer],
                true,
            )?,
            exception_active: declaration(
                module,
                "rimera_exception_active",
                &[pointer, pointer],
                true,
            )?,
            exception_matches: declaration(
                module,
                "rimera_exception_matches",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            exception_split: declaration(
                module,
                "rimera_exception_split",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            exception_set_active: declaration(
                module,
                "rimera_exception_set_active",
                &[pointer, pointer],
                true,
            )?,
            exception_merge: declaration(
                module,
                "rimera_exception_merge_active",
                &[pointer, pointer],
                true,
            )?,
            exception_combine: declaration(
                module,
                "rimera_exception_combine",
                &[pointer, pointer, pointer, pointer],
                true,
            )?,
            exception_clear_active: declaration(
                module,
                "rimera_exception_clear_active",
                &[pointer],
                true,
            )?,
            handler_enter: declaration(module, "rimera_handler_enter", &[pointer, pointer], true)?,
            handler_leave: declaration(module, "rimera_handler_leave", &[pointer], true)?,
            raise: declaration(
                module,
                "rimera_raise",
                &[pointer, pointer, pointer, types::I8],
                true,
            )?,
            reraise: declaration(module, "rimera_reraise", &[pointer], true)?,
            propagate: declaration(module, "rimera_exception_propagate", &[pointer], true)?,
            traceback_append: declaration(
                module,
                "rimera_traceback_append",
                &[
                    pointer,
                    pointer,
                    pointer,
                    pointer,
                    pointer,
                    types::I32,
                    types::I32,
                ],
                true,
            )?,
            traceback_append_module: declaration(
                module,
                "rimera_traceback_append_module",
                &[
                    pointer,
                    pointer,
                    pointer,
                    pointer,
                    pointer,
                    types::I32,
                    types::I32,
                ],
                true,
            )?,
            unary: declaration(
                module,
                "rimera_unary",
                &[pointer, types::I8, pointer, pointer],
                true,
            )?,
            binary: declaration(
                module,
                "rimera_binary",
                &[pointer, types::I8, pointer, pointer, pointer],
                true,
            )?,
            inplace: declaration(
                module,
                "rimera_inplace",
                &[pointer, types::I8, pointer, pointer, pointer],
                true,
            )?,
            compare: declaration(
                module,
                "rimera_compare",
                &[pointer, types::I8, pointer, pointer, pointer],
                true,
            )?,
            format_value: declaration(
                module,
                "rimera_format_value",
                &[pointer, types::I8, pointer, pointer, pointer],
                true,
            )?,
            truthy: declaration(module, "rimera_truthy", &[pointer, pointer, pointer], true)?,
            print: declaration(module, "rimera_print", &[pointer, pointer, pointer], true)?,
            print_literal: declaration(
                module,
                "rimera_print_literal",
                &[pointer, pointer, pointer],
                true,
            )?,
            collect: declaration(module, "rimera_collect", &[pointer], true)?,
            render_error: declaration(module, "rimera_render_error", &[pointer], false)?,
        })
    }
}

#[derive(Default)]
struct ConstantData {
    values: BTreeMap<(u32, u32, usize), (DataId, usize)>,
    strings: BTreeMap<String, (DataId, usize)>,
}

fn define_constants(
    module: &mut ObjectModule,
    program: &mir::Program,
) -> Result<ConstantData, String> {
    let mut data = ConstantData::default();
    for (function_index, function) in program.functions.iter().enumerate() {
        define_string(module, &mut data, &program.filename)?;
        define_string(module, &mut data, &function.name)?;
        define_string(module, &mut data, &function.qualified_name)?;
        for parameter in &function.parameters {
            define_string(module, &mut data, &parameter.name)?;
        }
        for (block_index, block) in function.blocks.iter().enumerate() {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                let bytes = match &operation.kind {
                    OperationKind::Constant {
                        value: mir::Constant::Int(value),
                        ..
                    }
                    | OperationKind::Constant {
                        value: mir::Constant::String(value),
                        ..
                    } => Some(value.as_bytes()),
                    OperationKind::Constant {
                        value: mir::Constant::Bytes(value),
                        ..
                    } => Some(value.as_slice()),
                    OperationKind::PrintLiteral { value } => Some(value.as_bytes()),
                    _ => None,
                };
                if let Some(bytes) = bytes {
                    let name =
                        format!("rimera_constant_{function_index}_{block_index}_{operation_index}");
                    let id = module
                        .declare_data(&name, Linkage::Local, false, false)
                        .map_err(|error| error.to_string())?;
                    let mut description = DataDescription::new();
                    description.define(bytes.to_vec().into_boxed_slice());
                    module
                        .define_data(id, &description)
                        .map_err(|error| error.to_string())?;
                    data.values.insert(
                        (function_index as u32, block_index as u32, operation_index),
                        (id, bytes.len()),
                    );
                }
                match &operation.kind {
                    OperationKind::Call { keywords, .. } => {
                        for (name, _) in keywords {
                            define_string(module, &mut data, name)?;
                        }
                    }
                    OperationKind::CallArgumentAdd {
                        name: Some(name), ..
                    } => {
                        define_string(module, &mut data, name)?;
                    }
                    OperationKind::MakeFunction {
                        local_names,
                        cell_names,
                        free_names,
                        ..
                    } => {
                        for name in local_names.iter().chain(cell_names).chain(free_names) {
                            define_string(module, &mut data, name)?;
                        }
                    }
                    OperationKind::TypeParameterNew { name, .. }
                    | OperationKind::TypeAliasNew { name, .. }
                    | OperationKind::ReflectionLocalRegister { name, .. }
                    | OperationKind::ImportName { name, .. }
                    | OperationKind::GlobalGet { name, .. }
                    | OperationKind::GlobalSet { name, .. }
                    | OperationKind::GlobalDelete { name }
                    | OperationKind::ClassNamespaceSet { name, .. }
                    | OperationKind::ClassNamespaceGet { name, .. }
                    | OperationKind::ClassNameGet { name, .. }
                    | OperationKind::ClassNamespaceDelete { name, .. }
                    | OperationKind::ClassNew { name, .. }
                    | OperationKind::AttributeGet { name, .. }
                    | OperationKind::AttributeSet { name, .. }
                    | OperationKind::AttributeDelete { name, .. } => {
                        define_string(module, &mut data, name)?;
                    }
                    OperationKind::CellGet {
                        name: Some(name), ..
                    } => {
                        define_string(module, &mut data, name)?;
                    }
                    OperationKind::PatternClass { keyword_names, .. }
                        if !keyword_names.is_empty() =>
                    {
                        define_string(module, &mut data, &keyword_names.join("\0"))?;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(data)
}

fn define_string(
    module: &mut ObjectModule,
    data: &mut ConstantData,
    value: &str,
) -> Result<(), String> {
    if data.strings.contains_key(value) {
        return Ok(());
    }
    let name = format!("rimera_metadata_{}", data.strings.len());
    let id = module
        .declare_data(&name, Linkage::Local, false, false)
        .map_err(|error| error.to_string())?;
    let mut description = DataDescription::new();
    description.define(value.as_bytes().to_vec().into_boxed_slice());
    module
        .define_data(id, &description)
        .map_err(|error| error.to_string())?;
    data.strings.insert(value.to_owned(), (id, value.len()));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn define_main(
    module: &mut ObjectModule,
    program: &mir::Function,
    root_plan: &mir::SafepointPlan,
    pointer: cranelift_codegen::ir::Type,
    imports: &Imports,
    data: &ConstantData,
    module_program: &mir::Program,
    native_functions: &[Option<cranelift_module::FuncId>],
    function_index: u32,
    heap_limit_bytes: Option<u64>,
) -> Result<(), String> {
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(types::I32));
    signature.params.push(AbiParam::new(pointer));
    signature.returns.push(AbiParam::new(types::I32));
    let function_id = module
        .declare_function("main", Linkage::Export, &signature)
        .map_err(|error| error.to_string())?;
    let mut context = module.make_context();
    context.func.signature = signature;
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let prologue = builder.create_block();
        builder.append_block_params_for_function_params(prologue);
        builder.switch_to_block(prologue);

        let context_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        let value_bytes = program.value_count.max(1) * VALUE_SIZE;
        let values_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            value_bytes,
            3,
        ));
        let root_capacity = root_plan.max_roots();
        let root_bytes = u32::try_from(root_capacity.max(1).saturating_mul(VALUE_SIZE as usize))
            .map_err(|_| "root frame is too large")?;
        let roots_slot = builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            root_bytes,
            3,
        ));
        let frame_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 24, 3));
        let failure_line_slot =
            builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 4, 2));
        let initial_line = builder.ins().iconst(types::I32, 1);
        builder
            .ins()
            .stack_store(initial_line, failure_line_slot, 0);
        for offset in (0..value_bytes).step_by(8) {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().stack_store(
                zero,
                values_slot,
                i32::try_from(offset).map_err(|_| "value frame is too large")?,
            );
        }
        for offset in (0..root_bytes).step_by(8) {
            let zero = builder.ins().iconst(types::I64, 0);
            builder.ins().stack_store(
                zero,
                roots_slot,
                i32::try_from(offset).map_err(|_| "root frame is too large")?,
            );
        }
        let context_output = builder.ins().stack_addr(pointer, context_slot, 0);
        let context_new = module.declare_func_in_func(imports.context_new, builder.func);
        let abi_version = builder.ins().iconst(types::I32, i64::from(ABI_VERSION));
        let call = builder
            .ins()
            .call(context_new, &[abi_version, context_output]);
        let status = builder.inst_results(call)[0];
        let context_failed = builder.ins().icmp_imm(IntCC::NotEqual, status, 0);
        let initialized = builder.create_block();
        let context_failure = builder.create_block();
        builder
            .ins()
            .brif(context_failed, context_failure, &[], initialized, &[]);
        builder.switch_to_block(context_failure);
        let failure_code = builder.ins().iconst(types::I32, 1);
        builder.ins().return_(&[failure_code]);
        builder.switch_to_block(initialized);
        let context_value = builder.ins().stack_load(pointer, context_slot, 0);
        let roots_pointer = builder.ins().stack_addr(pointer, roots_slot, 0);
        let null = builder.ins().iconst(pointer, 0);
        builder.ins().stack_store(null, frame_slot, 0);
        builder.ins().stack_store(roots_pointer, frame_slot, 8);
        let root_count = builder.ins().iconst(
            pointer,
            i64::try_from(root_capacity).map_err(|_| "too many GC roots")?,
        );
        builder.ins().stack_store(root_count, frame_slot, 16);
        let frame_pointer = builder.ins().stack_addr(pointer, frame_slot, 0);
        let failure = builder.create_block();
        let propagated_failure = builder.create_block();
        let push_ref = module.declare_func_in_func(imports.roots_push, builder.func);
        emit_status_call(
            &mut builder,
            push_ref,
            &[context_value, frame_pointer],
            failure,
        );
        let kernel_initialize =
            module.declare_func_in_func(imports.kernel_initialize, builder.func);
        emit_status_call(&mut builder, kernel_initialize, &[context_value], failure);
        let set_heap_limit =
            module.declare_func_in_func(imports.context_set_heap_limit, builder.func);
        let heap_limit = builder
            .ins()
            .iconst(pointer, heap_limit_bytes.unwrap_or(0) as i64);
        emit_status_call(
            &mut builder,
            set_heap_limit,
            &[context_value, heap_limit],
            failure,
        );

        let block_map = program
            .blocks
            .iter()
            .map(|_| builder.create_block())
            .collect::<Vec<_>>();
        builder.ins().jump(block_map[program.entry.0 as usize], &[]);
        let refs = FunctionRefs::new_module(module, builder.func, imports);
        let mut traced_exception_edges = Vec::new();
        for (block_index, block) in program.blocks.iter().enumerate() {
            builder.switch_to_block(block_map[block_index]);
            for (operation_index, operation) in block.operations.iter().enumerate() {
                let source_line = source_line(&module_program.line_starts, operation.span.start);
                let line_value = builder.ins().iconst(types::I32, i64::from(source_line));
                builder.ins().stack_store(line_value, failure_line_slot, 0);
                let preserves_traceback = matches!(
                    operation.kind,
                    OperationKind::Reraise | OperationKind::Propagate
                );
                let operation_failure = if let Some(target) = program
                    .exception_edges
                    .get(&(block_index as u32, operation_index as u32))
                {
                    if preserves_traceback {
                        block_map[target.0 as usize]
                    } else {
                        let traced = builder.create_block();
                        traced_exception_edges.push((
                            traced,
                            block_map[target.0 as usize],
                            source_line,
                        ));
                        traced
                    }
                } else if preserves_traceback
                    || matches!(operation.kind, OperationKind::CallModuleChunk { .. })
                {
                    propagated_failure
                } else {
                    failure
                };
                emit_operation(
                    &mut builder,
                    module,
                    imports,
                    data,
                    module_program,
                    native_functions,
                    function_index,
                    block_index as u32,
                    operation_index,
                    operation,
                    pointer,
                    values_slot,
                    roots_slot,
                    root_capacity,
                    root_plan.operation_roots(block_index, operation_index),
                    context_value,
                    None,
                    operation_failure,
                )?;
            }
            emit_terminator(
                &mut builder,
                &refs,
                program,
                &block_map,
                &block.terminator,
                pointer,
                values_slot,
                roots_slot,
                root_capacity,
                root_plan.terminator_roots(block_index),
                context_value,
                frame_pointer,
                ReturnMode::Main,
                failure,
            )?;
        }
        for (traced, target, line) in traced_exception_edges {
            builder.switch_to_block(traced);
            let (filename, filename_len) = metadata_pointer(
                &mut builder,
                module,
                data,
                &module_program.filename,
                pointer,
            )?;
            let (function_name, function_name_len) =
                metadata_pointer(&mut builder, module, data, &program.name, pointer)?;
            let line = builder.ins().iconst(types::I32, i64::from(line));
            let column = builder.ins().iconst(types::I32, 0);
            builder.ins().call(
                refs.traceback_append,
                &[
                    context_value,
                    filename,
                    filename_len,
                    function_name,
                    function_name_len,
                    line,
                    column,
                ],
            );
            builder.ins().jump(target, &[]);
        }

        builder.switch_to_block(failure);
        let (filename, filename_len) = metadata_pointer(
            &mut builder,
            module,
            data,
            &module_program.filename,
            pointer,
        )?;
        let (function_name, function_name_len) =
            metadata_pointer(&mut builder, module, data, &program.name, pointer)?;
        let line = builder.ins().stack_load(types::I32, failure_line_slot, 0);
        let column = builder.ins().iconst(types::I32, 0);
        builder.ins().call(
            refs.traceback_append,
            &[
                context_value,
                filename,
                filename_len,
                function_name,
                function_name_len,
                line,
                column,
            ],
        );
        builder.ins().jump(propagated_failure, &[]);
        builder.switch_to_block(propagated_failure);
        builder.ins().call(refs.render_error, &[context_value]);
        builder
            .ins()
            .call(refs.roots_pop, &[context_value, frame_pointer]);
        builder.ins().call(refs.context_free, &[context_value]);
        let failure_code = builder.ins().iconst(types::I32, 1);
        builder.ins().return_(&[failure_code]);
        builder.seal_all_blocks();
        builder.finalize();
    }
    module
        .define_function(function_id, &mut context)
        .map_err(|error| error.to_string())?;
    module.clear_context(&mut context);
    Ok(())
}

struct FunctionRefs {
    context_free: FuncRef,
    roots_pop: FuncRef,
    traceback_append: FuncRef,
    truthy: FuncRef,
    render_error: FuncRef,
}

impl FunctionRefs {
    fn new(
        module: &mut ObjectModule,
        function: &mut cranelift_codegen::ir::Function,
        imports: &Imports,
    ) -> Self {
        Self {
            context_free: module.declare_func_in_func(imports.context_free, function),
            roots_pop: module.declare_func_in_func(imports.roots_pop, function),
            traceback_append: module.declare_func_in_func(imports.traceback_append, function),
            truthy: module.declare_func_in_func(imports.truthy, function),
            render_error: module.declare_func_in_func(imports.render_error, function),
        }
    }

    fn new_module(
        module: &mut ObjectModule,
        function: &mut cranelift_codegen::ir::Function,
        imports: &Imports,
    ) -> Self {
        let mut refs = Self::new(module, function, imports);
        refs.traceback_append =
            module.declare_func_in_func(imports.traceback_append_module, function);
        refs
    }
}
#[allow(clippy::too_many_arguments)]
fn emit_operation(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    imports: &Imports,
    data: &ConstantData,
    module_program: &mir::Program,
    native_functions: &[Option<cranelift_module::FuncId>],
    function_index: u32,
    block_index: u32,
    operation_index: usize,
    operation: &mir::Operation,
    pointer: cranelift_codegen::ir::Type,
    values_slot: StackSlot,
    roots_slot: StackSlot,
    root_capacity: usize,
    roots: Option<&[mir::ValueId]>,
    context: Value,
    function_value: Option<Value>,
    failure: cranelift_codegen::ir::Block,
) -> Result<(), String> {
    if let Some(roots) = roots {
        publish_roots(builder, values_slot, roots_slot, root_capacity, roots)?;
    }
    match &operation.kind {
        OperationKind::Constant { dest, value } => match value {
            mir::Constant::None => store_immediate(builder, values_slot, *dest, 0, 0),
            mir::Constant::Bool(value) => {
                store_immediate(builder, values_slot, *dest, 1, u64::from(*value))
            }
            mir::Constant::Int(value) => {
                if let Ok(value) = value.parse::<i64>() {
                    store_immediate(builder, values_slot, *dest, 2, value as u64);
                } else {
                    let runtime_call =
                        module.declare_func_in_func(imports.int_from_decimal, builder.func);

                    emit_data_constructor(
                        builder,
                        module,
                        runtime_call,
                        data,
                        function_index,
                        block_index,
                        operation_index,
                        pointer,
                        values_slot,
                        *dest,
                        context,
                        failure,
                    )?;
                }
            }
            mir::Constant::Float(bits) => {
                let bits = builder.ins().iconst(types::I64, *bits as i64);
                let output = value_pointer(builder, pointer, values_slot, *dest);
                let runtime_call = module.declare_func_in_func(imports.float_new, builder.func);

                emit_status_call(builder, runtime_call, &[context, bits, output], failure);
            }
            mir::Constant::Complex { real, imag } => {
                let real = builder.ins().iconst(types::I64, *real as i64);
                let imag = builder.ins().iconst(types::I64, *imag as i64);
                let output = value_pointer(builder, pointer, values_slot, *dest);
                let runtime_call = module.declare_func_in_func(imports.complex_new, builder.func);

                emit_status_call(
                    builder,
                    runtime_call,
                    &[context, real, imag, output],
                    failure,
                );
            }
            mir::Constant::String(_) => {
                let runtime_call = module.declare_func_in_func(imports.string_new, builder.func);

                emit_data_constructor(
                    builder,
                    module,
                    runtime_call,
                    data,
                    function_index,
                    block_index,
                    operation_index,
                    pointer,
                    values_slot,
                    *dest,
                    context,
                    failure,
                )?;
            }
            mir::Constant::Bytes(_) => {
                let runtime_call = module.declare_func_in_func(imports.bytes_new, builder.func);

                emit_data_constructor(
                    builder,
                    module,
                    runtime_call,
                    data,
                    function_index,
                    block_index,
                    operation_index,
                    pointer,
                    values_slot,
                    *dest,
                    context,
                    failure,
                )?;
            }
        },
        OperationKind::SliceNew {
            dest,
            start,
            stop,
            step,
        } => {
            let null = builder.ins().iconst(pointer, 0);
            let start = start
                .map(|value| value_pointer(builder, pointer, values_slot, value))
                .unwrap_or(null);
            let stop = stop
                .map(|value| value_pointer(builder, pointer, values_slot, value))
                .unwrap_or(null);
            let step = step
                .map(|value| value_pointer(builder, pointer, values_slot, value))
                .unwrap_or(null);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.slice_new, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, start, stop, step, output],
                failure,
            );
        }
        OperationKind::Copy { dest, source } => copy_value(builder, values_slot, *source, *dest),
        OperationKind::Unary { dest, op, operand } => {
            let operand = value_pointer(builder, pointer, values_slot, *operand);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let opcode = builder.ins().iconst(types::I8, i64::from(*op as u8));
            let runtime_call = module.declare_func_in_func(imports.unary, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, opcode, operand, output],
                failure,
            );
        }
        OperationKind::Binary {
            dest,
            op,
            left,
            right,
        } => {
            let left = value_pointer(builder, pointer, values_slot, *left);
            let right = value_pointer(builder, pointer, values_slot, *right);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let opcode = builder.ins().iconst(types::I8, i64::from(*op as u8));
            let runtime_call = module.declare_func_in_func(imports.binary, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, opcode, left, right, output],
                failure,
            );
        }
        OperationKind::InPlace {
            dest,
            op,
            left,
            right,
        } => {
            let left = value_pointer(builder, pointer, values_slot, *left);
            let right = value_pointer(builder, pointer, values_slot, *right);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let opcode = builder.ins().iconst(types::I8, i64::from(*op as u8));
            let runtime_call = module.declare_func_in_func(imports.inplace, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, opcode, left, right, output],
                failure,
            );
        }
        OperationKind::Compare {
            dest,
            op,
            left,
            right,
        } => {
            let left = value_pointer(builder, pointer, values_slot, *left);
            let right = value_pointer(builder, pointer, values_slot, *right);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let opcode = builder.ins().iconst(types::I8, i64::from(*op as u8));
            let runtime_call = module.declare_func_in_func(imports.compare, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, opcode, left, right, output],
                failure,
            );
        }
        OperationKind::FormatValue {
            dest,
            value,
            conversion,
            spec,
        } => {
            let value = value_pointer(builder, pointer, values_slot, *value);
            let spec = value_pointer(builder, pointer, values_slot, *spec);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let conversion = builder
                .ins()
                .iconst(types::I8, i64::from(*conversion as u8));
            let runtime_call = module.declare_func_in_func(imports.format_value, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, conversion, value, spec, output],
                failure,
            );
        }
        OperationKind::ValueArray { dest, values }
        | OperationKind::Tuple { dest, values }
        | OperationKind::List { dest, values }
        | OperationKind::Set { dest, values } => {
            let size = u32::try_from(values.len().max(1) * VALUE_SIZE as usize)
                .map_err(|_| "value array is too large")?;
            let elements = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                size,
                3,
            ));
            for (index, value) in values.iter().enumerate() {
                copy_between_slots(
                    builder,
                    values_slot,
                    value_offset(*value),
                    elements,
                    i32::try_from(index * VALUE_SIZE as usize)
                        .map_err(|_| "value-array element offset is too large")?,
                );
            }
            let elements = builder.ins().stack_addr(pointer, elements, 0);
            let count = builder.ins().iconst(
                pointer,
                i64::try_from(values.len()).map_err(|_| "too many value-array elements")?,
            );
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let constructor = match &operation.kind {
                OperationKind::Tuple { .. } => {
                    module.declare_func_in_func(imports.tuple_new, builder.func)
                }
                OperationKind::List { .. } => {
                    module.declare_func_in_func(imports.list_new, builder.func)
                }
                OperationKind::Set { .. } => {
                    module.declare_func_in_func(imports.set_new, builder.func)
                }
                OperationKind::ValueArray { .. } => {
                    module.declare_func_in_func(imports.value_array_new, builder.func)
                }
                _ => unreachable!("collection constructor operation was selected by its match arm"),
            };
            emit_status_call(
                builder,
                constructor,
                &[context, elements, count, output],
                failure,
            );
        }
        OperationKind::Dictionary { dest, keys, values } => {
            if keys.len() != values.len() {
                return Err("dictionary operation has mismatched key/value lists".to_owned());
            }
            let size = u32::try_from(keys.len().max(1) * VALUE_SIZE as usize)
                .map_err(|_| "dictionary is too large")?;
            let key_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                size,
                3,
            ));
            let value_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                size,
                3,
            ));
            for (index, (key, value)) in keys.iter().zip(values).enumerate() {
                let offset = i32::try_from(index * VALUE_SIZE as usize)
                    .map_err(|_| "dictionary element offset is too large")?;
                copy_between_slots(builder, values_slot, value_offset(*key), key_slot, offset);
                copy_between_slots(
                    builder,
                    values_slot,
                    value_offset(*value),
                    value_slot,
                    offset,
                );
            }
            let key_ptr = builder.ins().stack_addr(pointer, key_slot, 0);
            let value_ptr = builder.ins().stack_addr(pointer, value_slot, 0);
            let count = builder.ins().iconst(
                pointer,
                i64::try_from(keys.len()).map_err(|_| "dictionary is too large")?,
            );
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.dict_new, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, key_ptr, value_ptr, count, output],
                failure,
            );
        }
        OperationKind::ListAppend { list, value } => {
            let list = value_pointer(builder, pointer, values_slot, *list);
            let value = value_pointer(builder, pointer, values_slot, *value);
            let runtime_call = module.declare_func_in_func(imports.list_append, builder.func);
            emit_status_call(builder, runtime_call, &[context, list, value], failure);
        }
        OperationKind::DictionaryMerge { dictionary, source } => {
            let dictionary = value_pointer(builder, pointer, values_slot, *dictionary);
            let source = value_pointer(builder, pointer, values_slot, *source);
            let runtime_call = module.declare_func_in_func(imports.dict_merge, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, dictionary, source],
                failure,
            );
        }
        OperationKind::DictionaryInsert {
            dictionary,
            key,
            value,
        } => {
            let dictionary = value_pointer(builder, pointer, values_slot, *dictionary);
            let key = value_pointer(builder, pointer, values_slot, *key);
            let value = value_pointer(builder, pointer, values_slot, *value);
            let runtime_call = module.declare_func_in_func(imports.dict_insert, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, dictionary, key, value],
                failure,
            );
        }
        OperationKind::SetInsert { set, value } => {
            let set = value_pointer(builder, pointer, values_slot, *set);
            let value = value_pointer(builder, pointer, values_slot, *value);
            let runtime_call = module.declare_func_in_func(imports.set_insert, builder.func);
            emit_status_call(builder, runtime_call, &[context, set, value], failure);
        }
        OperationKind::Contains {
            dest,
            collection,
            needle,
            negate,
        } => {
            let collection = value_pointer(builder, pointer, values_slot, *collection);
            let needle = value_pointer(builder, pointer, values_slot, *needle);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.contains, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, collection, needle, output],
                failure,
            );
            if *negate {
                let payload = builder.ins().load(
                    types::I64,
                    cranelift_codegen::ir::MemFlags::trusted(),
                    output,
                    8,
                );
                let inverted = builder.ins().bxor_imm(payload, 1);
                builder.ins().store(
                    cranelift_codegen::ir::MemFlags::trusted(),
                    inverted,
                    output,
                    8,
                );
            }
        }
        OperationKind::Unpack {
            dest,
            value,
            before_count,
            after_count,
            starred,
        } => {
            let value = value_pointer(builder, pointer, values_slot, *value);
            let before = builder.ins().iconst(pointer, i64::from(*before_count));
            let after = builder.ins().iconst(pointer, i64::from(*after_count));
            let starred = builder.ins().iconst(types::I8, i64::from(*starred));
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.unpack, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, value, before, after, starred, output],
                failure,
            );
        }
        OperationKind::ValueArrayGet { dest, array, index } => {
            let array = value_pointer(builder, pointer, values_slot, *array);
            let index = builder.ins().iconst(pointer, i64::from(*index));
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.value_array_get, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, array, index, output],
                failure,
            );
        }
        OperationKind::PatternSequence {
            values,
            matched,
            subject,
            before_count,
            after_count,
            starred,
        } => {
            let flag = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                1,
                0,
            ));
            let subject = value_pointer(builder, pointer, values_slot, *subject);
            let before = builder.ins().iconst(pointer, i64::from(*before_count));
            let after = builder.ins().iconst(pointer, i64::from(*after_count));
            let starred = builder.ins().iconst(types::I8, i64::from(*starred));
            let output = value_pointer(builder, pointer, values_slot, *values);
            let flag_pointer = builder.ins().stack_addr(pointer, flag, 0);
            let runtime_call = module.declare_func_in_func(imports.pattern_sequence, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[
                    context,
                    subject,
                    before,
                    after,
                    starred,
                    output,
                    flag_pointer,
                ],
                failure,
            );
            let tag = builder.ins().iconst(types::I64, 1);
            let loaded = builder.ins().stack_load(types::I8, flag, 0);
            let payload = builder.ins().uextend(types::I64, loaded);
            builder
                .ins()
                .stack_store(tag, values_slot, value_offset(*matched));
            builder
                .ins()
                .stack_store(payload, values_slot, value_offset(*matched) + 8);
        }
        OperationKind::PatternMappingCheck {
            matched,
            subject,
            minimum_count,
        } => {
            let subject = value_pointer(builder, pointer, values_slot, *subject);
            let minimum_count = builder.ins().iconst(pointer, i64::from(*minimum_count));
            let output = value_pointer(builder, pointer, values_slot, *matched);
            let runtime_call =
                module.declare_func_in_func(imports.pattern_mapping_check, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, subject, minimum_count, output],
                failure,
            );
        }
        OperationKind::PatternMapping {
            values,
            matched,
            subject,
            keys,
            rest,
        } => {
            let key_bytes = u32::try_from(keys.len().max(1) * VALUE_SIZE as usize)
                .map_err(|_| "mapping pattern key array is too large")?;
            let key_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                key_bytes,
                3,
            ));
            for (index, key) in keys.iter().enumerate() {
                copy_between_slots(
                    builder,
                    values_slot,
                    value_offset(*key),
                    key_slot,
                    i32::try_from(index * VALUE_SIZE as usize)
                        .map_err(|_| "mapping pattern key offset is too large")?,
                );
            }
            let flag = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                1,
                0,
            ));
            let subject = value_pointer(builder, pointer, values_slot, *subject);
            let key_pointer = builder.ins().stack_addr(pointer, key_slot, 0);
            let key_count = builder.ins().iconst(
                pointer,
                i64::try_from(keys.len()).map_err(|_| "too many mapping pattern keys")?,
            );
            let rest = builder.ins().iconst(types::I8, i64::from(*rest));
            let output = value_pointer(builder, pointer, values_slot, *values);
            let flag_pointer = builder.ins().stack_addr(pointer, flag, 0);
            let runtime_call = module.declare_func_in_func(imports.pattern_mapping, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[
                    context,
                    subject,
                    key_pointer,
                    key_count,
                    rest,
                    output,
                    flag_pointer,
                ],
                failure,
            );
            let tag = builder.ins().iconst(types::I64, 1);
            let loaded = builder.ins().stack_load(types::I8, flag, 0);
            let payload = builder.ins().uextend(types::I64, loaded);
            builder
                .ins()
                .stack_store(tag, values_slot, value_offset(*matched));
            builder
                .ins()
                .stack_store(payload, values_slot, value_offset(*matched) + 8);
        }
        OperationKind::PatternClass {
            values,
            matched,
            subject,
            class,
            positional_count,
            keyword_names,
        } => {
            let flag = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                1,
                0,
            ));
            let subject = value_pointer(builder, pointer, values_slot, *subject);
            let class = value_pointer(builder, pointer, values_slot, *class);
            let positional_count = builder.ins().iconst(pointer, i64::from(*positional_count));
            let keyword_blob = keyword_names.join("\0");
            let (keyword_pointer, keyword_len) = if keyword_blob.is_empty() {
                (
                    builder.ins().iconst(pointer, 0),
                    builder.ins().iconst(pointer, 0),
                )
            } else {
                metadata_pointer(builder, module, data, &keyword_blob, pointer)?
            };
            let output = value_pointer(builder, pointer, values_slot, *values);
            let flag_pointer = builder.ins().stack_addr(pointer, flag, 0);
            let runtime_call = module.declare_func_in_func(imports.pattern_class, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[
                    context,
                    subject,
                    class,
                    positional_count,
                    keyword_pointer,
                    keyword_len,
                    output,
                    flag_pointer,
                ],
                failure,
            );
            let tag = builder.ins().iconst(types::I64, 1);
            let loaded = builder.ins().stack_load(types::I8, flag, 0);
            let payload = builder.ins().uextend(types::I64, loaded);
            builder
                .ins()
                .stack_store(tag, values_slot, value_offset(*matched));
            builder
                .ins()
                .stack_store(payload, values_slot, value_offset(*matched) + 8);
        }
        OperationKind::ItemGet {
            dest,
            collection,
            index,
        } => {
            let collection = value_pointer(builder, pointer, values_slot, *collection);
            let index = value_pointer(builder, pointer, values_slot, *index);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.item_get, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, collection, index, output],
                failure,
            );
        }
        OperationKind::Length { dest, value } => {
            let value = value_pointer(builder, pointer, values_slot, *value);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.length, builder.func);

            emit_status_call(builder, runtime_call, &[context, value, output], failure);
        }
        OperationKind::Range {
            dest,
            start,
            stop,
            step,
        } => {
            let start = value_pointer(builder, pointer, values_slot, *start);
            let stop = value_pointer(builder, pointer, values_slot, *stop);
            let step = value_pointer(builder, pointer, values_slot, *step);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.range_new, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, start, stop, step, output],
                failure,
            );
        }
        OperationKind::IteratorNew { dest, value } => {
            let value = value_pointer(builder, pointer, values_slot, *value);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.iterator_new, builder.func);

            emit_status_call(builder, runtime_call, &[context, value, output], failure);
        }
        OperationKind::IteratorNext {
            item,
            has_value,
            iterator,
        } => {
            let flag = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                1,
                0,
            ));
            let iterator = value_pointer(builder, pointer, values_slot, *iterator);
            let output = value_pointer(builder, pointer, values_slot, *item);
            let flag_pointer = builder.ins().stack_addr(pointer, flag, 0);
            let runtime_call = module.declare_func_in_func(imports.iterator_next, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, iterator, output, flag_pointer],
                failure,
            );
            let tag = builder.ins().iconst(types::I64, 1);
            let loaded = builder.ins().stack_load(types::I8, flag, 0);
            let payload = builder.ins().uextend(types::I64, loaded);
            builder
                .ins()
                .stack_store(tag, values_slot, value_offset(*has_value));
            builder
                .ins()
                .stack_store(payload, values_slot, value_offset(*has_value) + 8);
        }
        OperationKind::YieldFromNext {
            yielded,
            result,
            complete,
            iterator,
        } => {
            let outcome_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                1,
                0,
            ));
            let temporary = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                VALUE_SIZE,
                3,
            ));
            let iterator = value_pointer(builder, pointer, values_slot, *iterator);
            let temporary_pointer = builder.ins().stack_addr(pointer, temporary, 0);
            let outcome_pointer = builder.ins().stack_addr(pointer, outcome_slot, 0);
            let runtime_call =
                module.declare_func_in_func(imports.generator_delegate_start, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, iterator, temporary_pointer, outcome_pointer],
                failure,
            );
            let first = builder.ins().stack_load(types::I64, temporary, 0);
            let second = builder.ins().stack_load(types::I64, temporary, 8);
            for destination in [*yielded, *result] {
                let offset = value_offset(destination);
                builder.ins().stack_store(first, values_slot, offset);
                builder.ins().stack_store(second, values_slot, offset + 8);
            }
            let outcome = builder.ins().stack_load(types::I8, outcome_slot, 0);
            let completed = builder.ins().icmp_imm(IntCC::Equal, outcome, 1);
            let tag = builder.ins().iconst(types::I64, 1);
            let payload = builder.ins().uextend(types::I64, completed);
            builder
                .ins()
                .stack_store(tag, values_slot, value_offset(*complete));
            builder
                .ins()
                .stack_store(payload, values_slot, value_offset(*complete) + 8);
        }
        OperationKind::ItemSet {
            collection,
            index,
            value,
        } => {
            let collection = value_pointer(builder, pointer, values_slot, *collection);
            let index = value_pointer(builder, pointer, values_slot, *index);
            let value = value_pointer(builder, pointer, values_slot, *value);
            let runtime_call = module.declare_func_in_func(imports.item_set, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, collection, index, value],
                failure,
            );
        }
        OperationKind::ItemDelete { collection, index } => {
            let collection = value_pointer(builder, pointer, values_slot, *collection);
            let index = value_pointer(builder, pointer, values_slot, *index);
            let runtime_call = module.declare_func_in_func(imports.item_delete, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, collection, index],
                failure,
            );
        }
        OperationKind::MakeFunction {
            dest,
            function,
            defaults,
            closure,
            local_names,
            cell_names,
            free_names,
        } => {
            let target = module_program
                .functions
                .get(function.0 as usize)
                .ok_or_else(|| "function creation references a missing function".to_owned())?;
            let function_id = native_functions
                .get(function.0 as usize)
                .and_then(|function| *function)
                .ok_or_else(|| "module initializer cannot be made into a function".to_owned())?;
            let function_ref = module.declare_func_in_func(function_id, builder.func);
            let code = builder.ins().func_addr(pointer, function_ref);
            let (name, name_len) = metadata_pointer(builder, module, data, &target.name, pointer)?;
            let (qualified_name, qualified_name_len) =
                metadata_pointer(builder, module, data, &target.qualified_name, pointer)?;

            let parameter_bytes = u32::try_from(target.parameters.len().max(1) * 40)
                .map_err(|_| "function parameter metadata is too large")?;
            let parameter_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                parameter_bytes,
                3,
            ));
            for (index, parameter) in target.parameters.iter().enumerate() {
                let offset = i32::try_from(index * 40)
                    .map_err(|_| "function parameter offset is too large")?;
                let (parameter_name, parameter_name_len) =
                    metadata_pointer(builder, module, data, &parameter.name, pointer)?;
                builder
                    .ins()
                    .stack_store(parameter_name, parameter_slot, offset);
                builder
                    .ins()
                    .stack_store(parameter_name_len, parameter_slot, offset + 8);
                let flags = u16::from(parameter.kind as u8)
                    | if parameter.has_default { 1 << 8 } else { 0 };
                let flags = builder.ins().iconst(types::I64, i64::from(flags));
                builder
                    .ins()
                    .stack_store(flags, parameter_slot, offset + 16);
                if let Some((_, default)) = defaults
                    .iter()
                    .find(|(parameter_index, _)| *parameter_index as usize == index)
                {
                    copy_between_slots(
                        builder,
                        values_slot,
                        value_offset(*default),
                        parameter_slot,
                        offset + 24,
                    );
                } else {
                    let zero = builder.ins().iconst(types::I64, 0);
                    builder.ins().stack_store(zero, parameter_slot, offset + 24);
                    builder.ins().stack_store(zero, parameter_slot, offset + 32);
                }
            }
            let parameter_pointer = builder.ins().stack_addr(pointer, parameter_slot, 0);
            let parameter_count = builder.ins().iconst(
                pointer,
                i64::try_from(target.parameters.len()).map_err(|_| "too many parameters")?,
            );

            let closure_bytes = u32::try_from(closure.len().max(1) * VALUE_SIZE as usize)
                .map_err(|_| "function closure is too large")?;
            let closure_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                closure_bytes,
                3,
            ));
            for (index, value) in closure.iter().enumerate() {
                copy_between_slots(
                    builder,
                    values_slot,
                    value_offset(*value),
                    closure_slot,
                    i32::try_from(index * VALUE_SIZE as usize)
                        .map_err(|_| "closure value offset is too large")?,
                );
            }
            let closure_pointer = builder.ins().stack_addr(pointer, closure_slot, 0);
            let closure_count = builder.ins().iconst(
                pointer,
                i64::try_from(closure.len()).map_err(|_| "too many closure cells")?,
            );

            let (filename, filename_len) =
                metadata_pointer(builder, module, data, &module_program.filename, pointer)?;
            let (local_names, local_name_len) =
                name_specs_pointer(builder, module, data, local_names, pointer)?;
            let (cell_names, cell_name_len) =
                name_specs_pointer(builder, module, data, cell_names, pointer)?;
            let (free_names, free_name_len) =
                name_specs_pointer(builder, module, data, free_names, pointer)?;
            let code_metadata_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                72,
                3,
            ));
            builder.ins().stack_store(filename, code_metadata_slot, 0);
            builder
                .ins()
                .stack_store(filename_len, code_metadata_slot, 8);
            let first_line = source_line(&module_program.line_starts, operation.span.start);
            let first_line = builder.ins().iconst(types::I32, i64::from(first_line));
            builder
                .ins()
                .stack_store(first_line, code_metadata_slot, 16);
            let reserved = builder.ins().iconst(types::I32, 0);
            builder.ins().stack_store(reserved, code_metadata_slot, 20);
            builder
                .ins()
                .stack_store(local_names, code_metadata_slot, 24);
            builder
                .ins()
                .stack_store(local_name_len, code_metadata_slot, 32);
            builder
                .ins()
                .stack_store(cell_names, code_metadata_slot, 40);
            builder
                .ins()
                .stack_store(cell_name_len, code_metadata_slot, 48);
            builder
                .ins()
                .stack_store(free_names, code_metadata_slot, 56);
            builder
                .ins()
                .stack_store(free_name_len, code_metadata_slot, 64);
            let code_metadata = builder.ins().stack_addr(pointer, code_metadata_slot, 0);

            let output = value_pointer(builder, pointer, values_slot, *dest);
            if target.kind == mir::FunctionKind::Generator {
                let persistent = mir::generator_persistent_values(target)?;
                let parameter_values = target
                    .parameters
                    .iter()
                    .map(|parameter| parameter.value)
                    .collect::<BTreeSet<_>>();
                let persistent_slot_count = target.parameters.len()
                    + persistent
                        .iter()
                        .filter(|value| !parameter_values.contains(value))
                        .count();
                let persistent_slot_count = builder.ins().iconst(
                    pointer,
                    i64::try_from(persistent_slot_count)
                        .map_err(|_| "too many generator persistent slots")?,
                );
                let runtime_call =
                    module.declare_func_in_func(imports.generator_function_new, builder.func);
                emit_status_call(
                    builder,
                    runtime_call,
                    &[
                        context,
                        code,
                        name,
                        name_len,
                        qualified_name,
                        qualified_name_len,
                        parameter_pointer,
                        parameter_count,
                        closure_pointer,
                        closure_count,
                        code_metadata,
                        persistent_slot_count,
                        output,
                    ],
                    failure,
                );
            } else {
                let runtime_call = module.declare_func_in_func(imports.function_new, builder.func);
                emit_status_call(
                    builder,
                    runtime_call,
                    &[
                        context,
                        code,
                        name,
                        name_len,
                        qualified_name,
                        qualified_name_len,
                        parameter_pointer,
                        parameter_count,
                        closure_pointer,
                        closure_count,
                        code_metadata,
                        output,
                    ],
                    failure,
                );
            }
        }
        OperationKind::TypeParameterNew { dest, name, kind } => {
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let kind = builder.ins().iconst(types::I8, i64::from(*kind as u8));
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call =
                module.declare_func_in_func(imports.type_parameter_new, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, kind, name, name_len, output],
                failure,
            );
        }
        OperationKind::TypeAliasNew {
            dest,
            name,
            type_params,
            value,
        } => {
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let type_params = value_pointer(builder, pointer, values_slot, *type_params);
            let value = value_pointer(builder, pointer, values_slot, *value);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.type_alias_new, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, name, name_len, type_params, value, output],
                failure,
            );
        }
        OperationKind::ClassNamespaceNew { dest } => {
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.namespace_new, builder.func);

            emit_status_call(builder, runtime_call, &[context, output], failure);
        }
        OperationKind::AnnotationsEnsure { namespace } => {
            let namespace = namespace
                .map(|value| value_pointer(builder, pointer, values_slot, value))
                .unwrap_or_else(|| builder.ins().iconst(pointer, 0));
            let runtime_call =
                module.declare_func_in_func(imports.annotations_ensure, builder.func);
            emit_status_call(builder, runtime_call, &[context, namespace], failure);
        }
        OperationKind::ClassNamespaceSet {
            namespace,
            name,
            value,
        } => {
            let namespace = value_pointer(builder, pointer, values_slot, *namespace);
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let value = value_pointer(builder, pointer, values_slot, *value);
            let runtime_call = module.declare_func_in_func(imports.namespace_set, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, namespace, name, name_len, value],
                failure,
            );
        }
        OperationKind::ClassNamespaceGet {
            dest,
            namespace,
            name,
        } => {
            let namespace = value_pointer(builder, pointer, values_slot, *namespace);
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.namespace_get, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, namespace, name, name_len, output],
                failure,
            );
        }
        OperationKind::ClassNameGet {
            dest,
            namespace,
            name,
        } => {
            let namespace = value_pointer(builder, pointer, values_slot, *namespace);
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.class_name_get, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, namespace, name, name_len, output],
                failure,
            );
        }
        OperationKind::ClassFreeGet {
            dest,
            namespace,
            cell,
            name,
        } => {
            let namespace = value_pointer(builder, pointer, values_slot, *namespace);
            let cell = value_pointer(builder, pointer, values_slot, *cell);
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.class_free_get, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, namespace, cell, name, name_len, output],
                failure,
            );
        }
        OperationKind::ClassNamespaceDelete { namespace, name } => {
            let namespace = value_pointer(builder, pointer, values_slot, *namespace);
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let runtime_call = module.declare_func_in_func(imports.namespace_delete, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, namespace, name, name_len],
                failure,
            );
        }
        OperationKind::ClassNew {
            dest,
            name,
            bases,
            namespace,
        } => {
            let namespace = value_pointer(builder, pointer, values_slot, *namespace);
            let size = u32::try_from(bases.len().max(1) * VALUE_SIZE as usize)
                .map_err(|_| "class base array is too large")?;
            let base_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                size,
                3,
            ));
            for (index, base) in bases.iter().enumerate() {
                copy_between_slots(
                    builder,
                    values_slot,
                    value_offset(*base),
                    base_slot,
                    i32::try_from(index * VALUE_SIZE as usize)
                        .map_err(|_| "class base offset is too large")?,
                );
            }
            let base_pointer = builder.ins().stack_addr(pointer, base_slot, 0);
            let base_count = builder.ins().iconst(
                pointer,
                i64::try_from(bases.len()).map_err(|_| "too many class bases")?,
            );
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.class_new, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[
                    context,
                    name,
                    name_len,
                    base_pointer,
                    base_count,
                    namespace,
                    output,
                ],
                failure,
            );
        }
        OperationKind::AttributeGet {
            dest,
            receiver,
            name,
        } => {
            let receiver = value_pointer(builder, pointer, values_slot, *receiver);
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.attr_get, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, receiver, name, name_len, output],
                failure,
            );
        }
        OperationKind::AttributeSet {
            receiver,
            name,
            value,
        } => {
            let receiver = value_pointer(builder, pointer, values_slot, *receiver);
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let value = value_pointer(builder, pointer, values_slot, *value);
            let runtime_call = module.declare_func_in_func(imports.attr_set, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, receiver, name, name_len, value],
                failure,
            );
        }
        OperationKind::AttributeDelete { receiver, name } => {
            let receiver = value_pointer(builder, pointer, values_slot, *receiver);
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let runtime_call = module.declare_func_in_func(imports.attr_delete, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, receiver, name, name_len],
                failure,
            );
        }
        OperationKind::Call {
            dest,
            callable,
            positional,
            keywords,
        } => {
            let positional_bytes = u32::try_from(positional.len().max(1) * VALUE_SIZE as usize)
                .map_err(|_| "call argument storage is too large")?;
            let positional_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                positional_bytes,
                3,
            ));
            for (index, value) in positional.iter().enumerate() {
                copy_between_slots(
                    builder,
                    values_slot,
                    value_offset(*value),
                    positional_slot,
                    i32::try_from(index * VALUE_SIZE as usize)
                        .map_err(|_| "call argument offset is too large")?,
                );
            }
            let positional_pointer = builder.ins().stack_addr(pointer, positional_slot, 0);
            let positional_count = builder.ins().iconst(
                pointer,
                i64::try_from(positional.len()).map_err(|_| "too many call arguments")?,
            );
            let keyword_bytes = u32::try_from(keywords.len().max(1) * 32)
                .map_err(|_| "keyword argument storage is too large")?;
            let keyword_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                keyword_bytes,
                3,
            ));
            for (index, (name, value)) in keywords.iter().enumerate() {
                let offset = i32::try_from(index * 32)
                    .map_err(|_| "keyword argument offset is too large")?;
                let (name_pointer, name_len) =
                    metadata_pointer(builder, module, data, name, pointer)?;
                builder
                    .ins()
                    .stack_store(name_pointer, keyword_slot, offset);
                builder
                    .ins()
                    .stack_store(name_len, keyword_slot, offset + 8);
                copy_between_slots(
                    builder,
                    values_slot,
                    value_offset(*value),
                    keyword_slot,
                    offset + 16,
                );
            }
            let keyword_pointer = builder.ins().stack_addr(pointer, keyword_slot, 0);
            let keyword_count = builder.ins().iconst(
                pointer,
                i64::try_from(keywords.len()).map_err(|_| "too many keyword arguments")?,
            );
            let descriptor = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                32,
                3,
            ));
            builder.ins().stack_store(positional_pointer, descriptor, 0);
            builder.ins().stack_store(positional_count, descriptor, 8);
            builder.ins().stack_store(keyword_pointer, descriptor, 16);
            builder.ins().stack_store(keyword_count, descriptor, 24);
            let descriptor = builder.ins().stack_addr(pointer, descriptor, 0);
            let callable = value_pointer(builder, pointer, values_slot, *callable);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.call, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, callable, descriptor, output],
                failure,
            );
        }
        OperationKind::CallArgumentsNew { dest, callable } => {
            let callable = value_pointer(builder, pointer, values_slot, *callable);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call =
                module.declare_func_in_func(imports.call_arguments_new, builder.func);
            emit_status_call(builder, runtime_call, &[context, callable, output], failure);
        }
        OperationKind::CallArgumentAdd {
            arguments,
            kind,
            name,
            value,
        } => {
            let arguments = value_pointer(builder, pointer, values_slot, *arguments);
            let value = value_pointer(builder, pointer, values_slot, *value);
            let kind = builder.ins().iconst(types::I8, i64::from(*kind as u8));
            let null = builder.ins().iconst(pointer, 0);
            let (name_pointer, name_len) = if let Some(name) = name {
                metadata_pointer(builder, module, data, name, pointer)?
            } else {
                (null, null)
            };
            let runtime_call = module.declare_func_in_func(imports.call_argument_add, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, arguments, kind, name_pointer, name_len, value],
                failure,
            );
        }
        OperationKind::CallPrepared {
            dest,
            callable,
            arguments,
        } => {
            let callable = value_pointer(builder, pointer, values_slot, *callable);
            let arguments = value_pointer(builder, pointer, values_slot, *arguments);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.call_prepared, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, callable, arguments, output],
                failure,
            );
        }
        OperationKind::CallModuleChunk { function } => {
            let function_id = native_functions
                .get(function.0 as usize)
                .and_then(|function| *function)
                .ok_or_else(|| format!("module chunk {} has no native function", function.0))?;
            let function_ref = module.declare_func_in_func(function_id, builder.func);
            let output_slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                VALUE_SIZE,
                3,
            ));
            let output = builder.ins().stack_addr(pointer, output_slot, 0);
            let null = builder.ins().iconst(pointer, 0);
            let count = builder.ins().iconst(pointer, 0);
            emit_status_call(
                builder,
                function_ref,
                &[context, null, null, count, output],
                failure,
            );
        }
        OperationKind::CellNew { dest, initial } => {
            let initial = if let Some(value) = initial {
                value_pointer(builder, pointer, values_slot, *value)
            } else {
                builder.ins().iconst(pointer, 0)
            };
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.cell_new, builder.func);

            emit_status_call(builder, runtime_call, &[context, initial, output], failure);
        }
        OperationKind::ReflectionScopeConfigure {
            namespace,
            comprehension,
        } => {
            let namespace = namespace
                .map(|value| value_pointer(builder, pointer, values_slot, value))
                .unwrap_or_else(|| builder.ins().iconst(pointer, 0));
            let comprehension = builder.ins().iconst(types::I8, i64::from(*comprehension));
            let runtime_call =
                module.declare_func_in_func(imports.reflection_scope_configure, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, namespace, comprehension],
                failure,
            );
        }
        OperationKind::ReflectionLocalRegister { name, cell } => {
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let cell = value_pointer(builder, pointer, values_slot, *cell);
            let runtime_call =
                module.declare_func_in_func(imports.reflection_local_register, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, name, name_len, cell],
                failure,
            );
        }
        OperationKind::CellGet {
            dest,
            cell,
            name,
            free,
        } => {
            let cell = value_pointer(builder, pointer, values_slot, *cell);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            if let Some(name) = name {
                let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
                let free = builder.ins().iconst(types::I8, i64::from(*free));
                let runtime_call =
                    module.declare_func_in_func(imports.cell_get_named, builder.func);

                emit_status_call(
                    builder,
                    runtime_call,
                    &[context, cell, name, name_len, free, output],
                    failure,
                );
            } else {
                let runtime_call = module.declare_func_in_func(imports.cell_get, builder.func);

                emit_status_call(builder, runtime_call, &[context, cell, output], failure);
            }
        }
        OperationKind::ClosureGet { dest, index } => {
            let function = function_value
                .ok_or_else(|| "module initializer cannot read a function closure".to_owned())?;
            let index = builder.ins().iconst(pointer, i64::from(*index));
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.closure_get, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, function, index, output],
                failure,
            );
        }
        OperationKind::CellSet { cell, value } => {
            let cell = value_pointer(builder, pointer, values_slot, *cell);
            let value = value_pointer(builder, pointer, values_slot, *value);
            let runtime_call = module.declare_func_in_func(imports.cell_set, builder.func);

            emit_status_call(builder, runtime_call, &[context, cell, value], failure);
        }
        OperationKind::CellClear { cell } => {
            let cell = value_pointer(builder, pointer, values_slot, *cell);
            let runtime_call = module.declare_func_in_func(imports.cell_clear, builder.func);

            emit_status_call(builder, runtime_call, &[context, cell], failure);
        }
        OperationKind::ImportName { dest, name } => {
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.import_name, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, name, name_len, output],
                failure,
            );
        }
        OperationKind::GlobalGet { dest, name } => {
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.global_get, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, name, name_len, output],
                failure,
            );
        }
        OperationKind::GlobalSet { name, value } => {
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let value = value_pointer(builder, pointer, values_slot, *value);
            let runtime_call = module.declare_func_in_func(imports.global_set, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, name, name_len, value],
                failure,
            );
        }
        OperationKind::GlobalDelete { name } => {
            let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
            let runtime_call = module.declare_func_in_func(imports.global_delete, builder.func);

            emit_status_call(builder, runtime_call, &[context, name, name_len], failure);
        }
        OperationKind::ExceptionActive { dest } => {
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.exception_active, builder.func);

            emit_status_call(builder, runtime_call, &[context, output], failure);
        }
        OperationKind::ExceptionMatches {
            dest,
            exception,
            expected_type,
        } => {
            let exception = value_pointer(builder, pointer, values_slot, *exception);
            let expected_type = value_pointer(builder, pointer, values_slot, *expected_type);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.exception_matches, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, exception, expected_type, output],
                failure,
            );
        }
        OperationKind::ExceptionSplit {
            dest,
            exception,
            expected_type,
        } => {
            let exception = value_pointer(builder, pointer, values_slot, *exception);
            let expected_type = value_pointer(builder, pointer, values_slot, *expected_type);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.exception_split, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, exception, expected_type, output],
                failure,
            );
        }
        OperationKind::ExceptionSetActive { exception } => {
            let exception = value_pointer(builder, pointer, values_slot, *exception);
            let runtime_call =
                module.declare_func_in_func(imports.exception_set_active, builder.func);

            emit_status_call(builder, runtime_call, &[context, exception], failure);
        }
        OperationKind::ExceptionMerge { remainder } => {
            let remainder = value_pointer(builder, pointer, values_slot, *remainder);
            let runtime_call = module.declare_func_in_func(imports.exception_merge, builder.func);

            emit_status_call(builder, runtime_call, &[context, remainder], failure);
        }
        OperationKind::ExceptionCombine { dest, left, right } => {
            let left = value_pointer(builder, pointer, values_slot, *left);
            let right = value_pointer(builder, pointer, values_slot, *right);
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.exception_combine, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, left, right, output],
                failure,
            );
        }
        OperationKind::ExceptionClearActive => {
            let runtime_call =
                module.declare_func_in_func(imports.exception_clear_active, builder.func);

            emit_status_call(builder, runtime_call, &[context], failure);
        }
        OperationKind::HandlerEnter { dest } => {
            let output = value_pointer(builder, pointer, values_slot, *dest);
            let runtime_call = module.declare_func_in_func(imports.handler_enter, builder.func);

            emit_status_call(builder, runtime_call, &[context, output], failure);
        }
        OperationKind::HandlerLeave => {
            let runtime_call = module.declare_func_in_func(imports.handler_leave, builder.func);

            emit_status_call(builder, runtime_call, &[context], failure);
        }
        OperationKind::Raise {
            exception,
            cause,
            suppress_context,
        } => {
            let exception = value_pointer(builder, pointer, values_slot, *exception);
            let cause = if let Some(cause) = cause {
                value_pointer(builder, pointer, values_slot, *cause)
            } else {
                builder.ins().iconst(pointer, 0)
            };
            let suppress = builder
                .ins()
                .iconst(types::I8, i64::from(*suppress_context));
            let runtime_call = module.declare_func_in_func(imports.raise, builder.func);
            emit_status_call(
                builder,
                runtime_call,
                &[context, exception, cause, suppress],
                failure,
            );
        }
        OperationKind::Reraise => {
            let runtime_call = module.declare_func_in_func(imports.reraise, builder.func);

            emit_status_call(builder, runtime_call, &[context], failure);
        }
        OperationKind::Propagate => {
            let runtime_call = module.declare_func_in_func(imports.propagate, builder.func);

            emit_status_call(builder, runtime_call, &[context], failure);
        }
        OperationKind::Print { values } => {
            let size = u32::try_from(values.len().max(1) * VALUE_SIZE as usize)
                .map_err(|_| "print argument frame is too large")?;
            let arguments = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                size,
                3,
            ));
            for (index, value) in values.iter().enumerate() {
                copy_between_slots(
                    builder,
                    values_slot,
                    value_offset(*value),
                    arguments,
                    i32::try_from(index * VALUE_SIZE as usize)
                        .map_err(|_| "print argument offset is too large")?,
                );
            }
            let pointer_value = builder.ins().stack_addr(pointer, arguments, 0);
            let count = builder.ins().iconst(
                pointer,
                i64::try_from(values.len()).map_err(|_| "too many print arguments")?,
            );
            let runtime_call = module.declare_func_in_func(imports.print, builder.func);

            emit_status_call(
                builder,
                runtime_call,
                &[context, pointer_value, count],
                failure,
            );
        }
        OperationKind::PrintLiteral { .. } => {
            let (data_id, len) = data
                .values
                .get(&(function_index, block_index, operation_index))
                .ok_or_else(|| "literal print data was not defined".to_owned())?;
            let global = module.declare_data_in_func(*data_id, builder.func);
            let bytes = builder.ins().symbol_value(pointer, global);
            let len = builder.ins().iconst(
                pointer,
                i64::try_from(*len).map_err(|_| "literal print data is too large")?,
            );
            let runtime_call = module.declare_func_in_func(imports.print_literal, builder.func);
            emit_status_call(builder, runtime_call, &[context, bytes, len], failure);
        }
        OperationKind::Collect => {
            let runtime_call = module.declare_func_in_func(imports.collect, builder.func);
            emit_status_call(builder, runtime_call, &[context], failure);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_generator_terminator(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    imports: &Imports,
    refs: &FunctionRefs,
    program: &mir::Function,
    blocks: &[cranelift_codegen::ir::Block],
    block_index: usize,
    line_starts: &[u32],
    terminator: &Terminator,
    pointer: cranelift_codegen::ir::Type,
    values_slot: StackSlot,
    roots_slot: StackSlot,
    root_capacity: usize,
    roots: Option<&[mir::ValueId]>,
    slots: &BTreeMap<mir::ValueId, usize>,
    context: Value,
    generator: Value,
    output: Value,
    outcome: Value,
    frame: Value,
    failure: cranelift_codegen::ir::Block,
) -> Result<(), String> {
    match terminator {
        Terminator::Jump { target, arguments } => {
            copy_edge_arguments(
                builder,
                values_slot,
                arguments,
                &program.blocks[target.0 as usize].parameters,
            )?;
            builder.ins().jump(blocks[target.0 as usize], &[]);
        }
        Terminator::Branch {
            condition,
            then_target,
            then_arguments,
            else_target,
            else_arguments,
        } => {
            let roots = roots.ok_or_else(|| {
                "internal error: branch is missing its GC safepoint roots".to_owned()
            })?;
            publish_roots(builder, values_slot, roots_slot, root_capacity, roots)?;
            let truth = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                VALUE_SIZE,
                3,
            ));
            let input = value_pointer(builder, pointer, values_slot, *condition);
            let truth_output = builder.ins().stack_addr(pointer, truth, 0);
            emit_status_call(
                builder,
                refs.truthy,
                &[context, input, truth_output],
                failure,
            );
            let payload = builder.ins().stack_load(types::I64, truth, 8);
            let condition = builder.ins().icmp_imm(IntCC::NotEqual, payload, 0);
            let then_edge = builder.create_block();
            let else_edge = builder.create_block();
            builder
                .ins()
                .brif(condition, then_edge, &[], else_edge, &[]);
            builder.switch_to_block(then_edge);
            copy_edge_arguments(
                builder,
                values_slot,
                then_arguments,
                &program.blocks[then_target.0 as usize].parameters,
            )?;
            builder.ins().jump(blocks[then_target.0 as usize], &[]);
            builder.switch_to_block(else_edge);
            copy_edge_arguments(
                builder,
                values_slot,
                else_arguments,
                &program.blocks[else_target.0 as usize].parameters,
            )?;
            builder.ins().jump(blocks[else_target.0 as usize], &[]);
        }
        Terminator::Yield {
            value, delegate, ..
        } => {
            let roots = roots.ok_or_else(|| {
                "internal error: yield is missing its persistent root set".to_owned()
            })?;
            publish_roots(builder, values_slot, roots_slot, root_capacity, roots)?;
            for persistent in roots {
                let Some(slot) = slots.get(persistent).copied() else {
                    continue;
                };
                let slot_index = builder.ins().iconst(
                    pointer,
                    i64::try_from(slot).map_err(|_| "generator slot index is too large")?,
                );
                let value_pointer = value_pointer(builder, pointer, values_slot, *persistent);
                let slot_set =
                    module.declare_func_in_func(imports.generator_slot_set, builder.func);
                emit_status_call(
                    builder,
                    slot_set,
                    &[context, generator, slot_index, value_pointer],
                    failure,
                );
            }
            if let Some(delegate) = delegate {
                let delegate = value_pointer(builder, pointer, values_slot, *delegate);
                let delegate_set =
                    module.declare_func_in_func(imports.generator_delegate_set, builder.func);
                emit_status_call(
                    builder,
                    delegate_set,
                    &[context, generator, delegate],
                    failure,
                );
            }
            let source_offset = program.blocks[block_index]
                .operations
                .last()
                .map_or(0, |operation| operation.span.start);
            let line = source_line(line_starts, source_offset);
            let line = builder.ins().iconst(types::I32, i64::from(line));
            let frame_line_set =
                module.declare_func_in_func(imports.generator_frame_line_set, builder.func);
            emit_status_call(
                builder,
                frame_line_set,
                &[context, generator, line],
                failure,
            );
            let state_id =
                u32::try_from(block_index + 1).map_err(|_| "too many generator states")?;
            let state = builder.ins().iconst(types::I32, i64::from(state_id));
            let state_set = module.declare_func_in_func(imports.generator_state_set, builder.func);
            emit_status_call(builder, state_set, &[context, generator, state], failure);
            let source = value_offset(*value);
            let first = builder.ins().stack_load(types::I64, values_slot, source);
            let second = builder
                .ins()
                .stack_load(types::I64, values_slot, source + 8);
            builder
                .ins()
                .store(cranelift_codegen::ir::MemFlags::trusted(), first, output, 0);
            builder.ins().store(
                cranelift_codegen::ir::MemFlags::trusted(),
                second,
                output,
                8,
            );
            let yielded = builder.ins().iconst(types::I8, 0);
            builder.ins().store(
                cranelift_codegen::ir::MemFlags::trusted(),
                yielded,
                outcome,
                0,
            );
            builder.ins().call(refs.roots_pop, &[context, frame]);
            let ok = builder.ins().iconst(types::I32, 0);
            builder.ins().return_(&[ok]);
        }
        Terminator::ReturnValue { value } => {
            if let Some(value) = value {
                let source = value_offset(*value);
                let first = builder.ins().stack_load(types::I64, values_slot, source);
                let second = builder
                    .ins()
                    .stack_load(types::I64, values_slot, source + 8);
                builder
                    .ins()
                    .store(cranelift_codegen::ir::MemFlags::trusted(), first, output, 0);
                builder.ins().store(
                    cranelift_codegen::ir::MemFlags::trusted(),
                    second,
                    output,
                    8,
                );
            } else {
                let zero = builder.ins().iconst(types::I64, 0);
                builder
                    .ins()
                    .store(cranelift_codegen::ir::MemFlags::trusted(), zero, output, 0);
                builder
                    .ins()
                    .store(cranelift_codegen::ir::MemFlags::trusted(), zero, output, 8);
            }
            let returned = builder.ins().iconst(types::I8, 1);
            builder.ins().store(
                cranelift_codegen::ir::MemFlags::trusted(),
                returned,
                outcome,
                0,
            );
            builder.ins().call(refs.roots_pop, &[context, frame]);
            let ok = builder.ins().iconst(types::I32, 0);
            builder.ins().return_(&[ok]);
        }
        Terminator::Return { .. } => {
            return Err("exit-code return cannot terminate a generator".to_owned());
        }
        Terminator::Unreachable => return Err("cannot emit unterminated MIR block".to_owned()),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_terminator(
    builder: &mut FunctionBuilder<'_>,
    refs: &FunctionRefs,
    program: &mir::Function,
    blocks: &[cranelift_codegen::ir::Block],
    terminator: &Terminator,
    pointer: cranelift_codegen::ir::Type,
    values_slot: StackSlot,
    roots_slot: StackSlot,
    root_capacity: usize,
    roots: Option<&[mir::ValueId]>,
    context: Value,
    frame: Value,
    return_mode: ReturnMode,
    failure: cranelift_codegen::ir::Block,
) -> Result<(), String> {
    match terminator {
        Terminator::Jump { target, arguments } => {
            copy_edge_arguments(
                builder,
                values_slot,
                arguments,
                &program.blocks[target.0 as usize].parameters,
            )?;
            builder.ins().jump(blocks[target.0 as usize], &[]);
        }
        Terminator::Branch {
            condition,
            then_target,
            then_arguments,
            else_target,
            else_arguments,
        } => {
            let roots = roots.ok_or_else(|| {
                "internal error: branch is missing its GC safepoint roots".to_owned()
            })?;
            publish_roots(builder, values_slot, roots_slot, root_capacity, roots)?;
            let truth = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                VALUE_SIZE,
                3,
            ));
            let input = value_pointer(builder, pointer, values_slot, *condition);
            let output = builder.ins().stack_addr(pointer, truth, 0);
            emit_status_call(builder, refs.truthy, &[context, input, output], failure);
            let payload = builder.ins().stack_load(types::I64, truth, 8);
            let condition = builder.ins().icmp_imm(IntCC::NotEqual, payload, 0);
            let then_edge = builder.create_block();
            let else_edge = builder.create_block();
            builder
                .ins()
                .brif(condition, then_edge, &[], else_edge, &[]);
            builder.switch_to_block(then_edge);
            copy_edge_arguments(
                builder,
                values_slot,
                then_arguments,
                &program.blocks[then_target.0 as usize].parameters,
            )?;
            builder.ins().jump(blocks[then_target.0 as usize], &[]);
            builder.switch_to_block(else_edge);
            copy_edge_arguments(
                builder,
                values_slot,
                else_arguments,
                &program.blocks[else_target.0 as usize].parameters,
            )?;
            builder.ins().jump(blocks[else_target.0 as usize], &[]);
        }
        Terminator::Return { code } => {
            if !matches!(return_mode, ReturnMode::Main) {
                return Err("exit-code return cannot terminate a Python function".to_owned());
            }
            builder.ins().call(refs.roots_pop, &[context, frame]);
            builder.ins().call(refs.context_free, &[context]);
            let return_code = builder.ins().iconst(types::I32, i64::from(*code));
            builder.ins().return_(&[return_code]);
        }
        Terminator::ReturnValue { value } => {
            let ReturnMode::Native { output } = return_mode else {
                return Err("value return cannot terminate the module initializer".to_owned());
            };
            if let Some(value) = value {
                let first = builder
                    .ins()
                    .stack_load(types::I64, values_slot, value_offset(*value));
                let second =
                    builder
                        .ins()
                        .stack_load(types::I64, values_slot, value_offset(*value) + 8);
                builder
                    .ins()
                    .store(cranelift_codegen::ir::MemFlags::trusted(), first, output, 0);
                builder.ins().store(
                    cranelift_codegen::ir::MemFlags::trusted(),
                    second,
                    output,
                    8,
                );
            } else {
                let zero = builder.ins().iconst(types::I64, 0);
                builder
                    .ins()
                    .store(cranelift_codegen::ir::MemFlags::trusted(), zero, output, 0);
                builder
                    .ins()
                    .store(cranelift_codegen::ir::MemFlags::trusted(), zero, output, 8);
            }
            builder.ins().call(refs.roots_pop, &[context, frame]);
            let ok = builder.ins().iconst(types::I32, 0);
            builder.ins().return_(&[ok]);
        }
        Terminator::Yield { .. } => {
            return Err("yield terminator reached the non-generator code path".to_owned());
        }
        Terminator::Unreachable => return Err("cannot emit unterminated MIR block".to_owned()),
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ReturnMode {
    Main,
    Native { output: Value },
}

fn publish_roots(
    builder: &mut FunctionBuilder<'_>,
    values_slot: StackSlot,
    roots_slot: StackSlot,
    capacity: usize,
    roots: &[mir::ValueId],
) -> Result<(), String> {
    if roots.len() > capacity {
        return Err("internal error: GC root set exceeds its frame capacity".to_owned());
    }
    for (index, value) in roots.iter().enumerate() {
        copy_between_slots(
            builder,
            values_slot,
            value_offset(*value),
            roots_slot,
            i32::try_from(index.saturating_mul(VALUE_SIZE as usize))
                .map_err(|_| "GC root offset is too large")?,
        );
    }
    for index in roots.len()..capacity {
        let offset = i32::try_from(index.saturating_mul(VALUE_SIZE as usize))
            .map_err(|_| "GC root offset is too large")?;
        let zero = builder.ins().iconst(types::I64, 0);
        builder.ins().stack_store(zero, roots_slot, offset);
        builder.ins().stack_store(zero, roots_slot, offset + 8);
    }
    Ok(())
}

fn copy_edge_arguments(
    builder: &mut FunctionBuilder<'_>,
    slot: StackSlot,
    arguments: &[mir::ValueId],
    parameters: &[mir::ValueId],
) -> Result<(), String> {
    if arguments.len() != parameters.len() {
        return Err("internal error: MIR edge arity changed after verification".to_owned());
    }
    for (argument, parameter) in arguments.iter().zip(parameters) {
        copy_value(builder, slot, *argument, *parameter);
    }
    Ok(())
}

fn metadata_pointer(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    data: &ConstantData,
    value: &str,
    pointer: cranelift_codegen::ir::Type,
) -> Result<(Value, Value), String> {
    let (data_id, len) = data
        .strings
        .get(value)
        .copied()
        .ok_or_else(|| format!("metadata string {value:?} is missing"))?;
    let global = module.declare_data_in_func(data_id, builder.func);
    let address = builder.ins().symbol_value(pointer, global);
    let len = builder.ins().iconst(
        pointer,
        i64::try_from(len).map_err(|_| "metadata string is too large")?,
    );
    Ok((address, len))
}

fn name_specs_pointer(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    data: &ConstantData,
    names: &[String],
    pointer: cranelift_codegen::ir::Type,
) -> Result<(Value, Value), String> {
    let bytes = u32::try_from(names.len().max(1) * 16)
        .map_err(|_| "code metadata name array is too large")?;
    let slot =
        builder.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, bytes, 3));
    for (index, name) in names.iter().enumerate() {
        let offset =
            i32::try_from(index * 16).map_err(|_| "code metadata name offset is too large")?;
        let (name, name_len) = metadata_pointer(builder, module, data, name, pointer)?;
        builder.ins().stack_store(name, slot, offset);
        builder.ins().stack_store(name_len, slot, offset + 8);
    }
    let address = builder.ins().stack_addr(pointer, slot, 0);
    let len = builder.ins().iconst(
        pointer,
        i64::try_from(names.len()).map_err(|_| "too many code metadata names")?,
    );
    Ok((address, len))
}

fn source_line(line_starts: &[u32], offset: u32) -> u32 {
    let index = line_starts.partition_point(|start| *start <= offset);
    u32::try_from(index.max(1)).unwrap_or(u32::MAX)
}

#[allow(clippy::too_many_arguments)]
fn emit_data_constructor(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    function: FuncRef,
    data: &ConstantData,
    function_index: u32,
    block: u32,
    operation: usize,
    pointer: cranelift_codegen::ir::Type,
    values_slot: StackSlot,
    destination: mir::ValueId,
    context: Value,
    failure: cranelift_codegen::ir::Block,
) -> Result<(), String> {
    let (data_id, len) = data
        .values
        .get(&(function_index, block, operation))
        .ok_or_else(|| "constant data was not defined".to_owned())?;
    let global = module.declare_data_in_func(*data_id, builder.func);
    let bytes = builder.ins().symbol_value(pointer, global);
    let len = builder.ins().iconst(
        pointer,
        i64::try_from(*len).map_err(|_| "constant is too large")?,
    );
    let output = value_pointer(builder, pointer, values_slot, destination);
    emit_status_call(builder, function, &[context, bytes, len, output], failure);
    Ok(())
}

fn emit_status_call(
    builder: &mut FunctionBuilder<'_>,
    function: FuncRef,
    arguments: &[Value],
    failure: cranelift_codegen::ir::Block,
) {
    let call = builder.ins().call(function, arguments);
    let status = builder.inst_results(call)[0];
    let failed = builder.ins().icmp_imm(IntCC::NotEqual, status, 0);
    let continuation = builder.create_block();
    builder.ins().brif(failed, failure, &[], continuation, &[]);
    builder.switch_to_block(continuation);
}

fn store_immediate(
    builder: &mut FunctionBuilder<'_>,
    slot: StackSlot,
    destination: mir::ValueId,
    tag: i64,
    payload: u64,
) {
    let offset = value_offset(destination);
    let header = builder.ins().iconst(types::I64, tag);
    let payload = builder.ins().iconst(types::I64, payload as i64);
    builder.ins().stack_store(header, slot, offset);
    builder.ins().stack_store(payload, slot, offset + 8);
}

fn copy_value(
    builder: &mut FunctionBuilder<'_>,
    slot: StackSlot,
    source: mir::ValueId,
    destination: mir::ValueId,
) {
    copy_between_slots(
        builder,
        slot,
        value_offset(source),
        slot,
        value_offset(destination),
    );
}

fn copy_between_slots(
    builder: &mut FunctionBuilder<'_>,
    source: StackSlot,
    source_offset: i32,
    destination: StackSlot,
    destination_offset: i32,
) {
    let header = builder.ins().stack_load(types::I64, source, source_offset);
    let payload = builder
        .ins()
        .stack_load(types::I64, source, source_offset + 8);
    builder
        .ins()
        .stack_store(header, destination, destination_offset);
    builder
        .ins()
        .stack_store(payload, destination, destination_offset + 8);
}

fn value_pointer(
    builder: &mut FunctionBuilder<'_>,
    pointer: cranelift_codegen::ir::Type,
    slot: StackSlot,
    value: mir::ValueId,
) -> Value {
    builder.ins().stack_addr(pointer, slot, value_offset(value))
}

fn value_offset(value: mir::ValueId) -> i32 {
    i32::try_from(value.0 * VALUE_SIZE).expect("verified MIR value frame fits signed offsets")
}
