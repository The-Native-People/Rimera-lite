// Explicit ABI references keep the JIT independent of host symbol export/stripping.
pub(crate) fn symbols() -> Vec<(&'static str, *const u8)> {
    vec![
        (
            "rimera_display",
            rimera_runtime::rimera_display as *const u8,
        ),
        (
            "rimera_context_new",
            rimera_runtime::rimera_context_new as *const () as *const u8,
        ),
        (
            "rimera_context_free",
            rimera_runtime::rimera_context_free as *const () as *const u8,
        ),
        (
            "rimera_context_set_heap_limit",
            rimera_runtime::rimera_context_set_heap_limit as *const () as *const u8,
        ),
        (
            "rimera_dynamic_compilation_enable",
            rimera_runtime::rimera_dynamic_compilation_enable as *const () as *const u8,
        ),
        (
            "rimera_kernel_initialize",
            rimera_runtime::rimera_kernel_initialize as *const () as *const u8,
        ),
        (
            "rimera_type_of",
            rimera_runtime::rimera_type_of as *const () as *const u8,
        ),
        (
            "rimera_function_new",
            rimera_runtime::rimera_function_new as *const () as *const u8,
        ),
        (
            "rimera_type_parameter_new",
            rimera_runtime::rimera_type_parameter_new as *const () as *const u8,
        ),
        (
            "rimera_type_alias_new",
            rimera_runtime::rimera_type_alias_new as *const () as *const u8,
        ),
        (
            "rimera_generator_function_new",
            rimera_runtime::rimera_generator_function_new as *const () as *const u8,
        ),
        (
            "rimera_coroutine_function_new",
            rimera_runtime::rimera_coroutine_function_new as *const () as *const u8,
        ),
        (
            "rimera_function_ready_coroutine_set",
            rimera_runtime::rimera_function_ready_coroutine_set as *const () as *const u8,
        ),
        (
            "rimera_async_generator_function_new",
            rimera_runtime::rimera_async_generator_function_new as *const () as *const u8,
        ),
        (
            "rimera_generator_new",
            rimera_runtime::rimera_generator_new as *const () as *const u8,
        ),
        (
            "rimera_generator_resume",
            rimera_runtime::rimera_generator_resume as *const () as *const u8,
        ),
        (
            "rimera_generator_delegate_start",
            rimera_runtime::rimera_generator_delegate_start as *const () as *const u8,
        ),
        (
            "rimera_generator_delegate_set",
            rimera_runtime::rimera_generator_delegate_set as *const () as *const u8,
        ),
        (
            "rimera_generator_delegate_resume",
            rimera_runtime::rimera_generator_delegate_resume as *const () as *const u8,
        ),
        (
            "rimera_generator_function_get",
            rimera_runtime::rimera_generator_function_get as *const () as *const u8,
        ),
        (
            "rimera_generator_state_get",
            rimera_runtime::rimera_generator_state_get as *const () as *const u8,
        ),
        (
            "rimera_generator_state_set",
            rimera_runtime::rimera_generator_state_set as *const () as *const u8,
        ),
        (
            "rimera_generator_frame_line_set",
            rimera_runtime::rimera_generator_frame_line_set as *const () as *const u8,
        ),
        (
            "rimera_generator_slot_get",
            rimera_runtime::rimera_generator_slot_get as *const () as *const u8,
        ),
        (
            "rimera_generator_slot_set",
            rimera_runtime::rimera_generator_slot_set as *const () as *const u8,
        ),
        (
            "rimera_cell_new",
            rimera_runtime::rimera_cell_new as *const () as *const u8,
        ),
        (
            "rimera_reflection_scope_configure",
            rimera_runtime::rimera_reflection_scope_configure as *const () as *const u8,
        ),
        (
            "rimera_reflection_native_locals_register",
            rimera_runtime::rimera_reflection_native_locals_register as *const () as *const u8,
        ),
        (
            "rimera_reflection_local_register",
            rimera_runtime::rimera_reflection_local_register as *const () as *const u8,
        ),
        (
            "rimera_unbound_local",
            rimera_runtime::rimera_unbound_local as *const () as *const u8,
        ),
        (
            "rimera_cell_get",
            rimera_runtime::rimera_cell_get as *const () as *const u8,
        ),
        (
            "rimera_cell_get_named",
            rimera_runtime::rimera_cell_get_named as *const () as *const u8,
        ),
        (
            "rimera_cell_set",
            rimera_runtime::rimera_cell_set as *const () as *const u8,
        ),
        (
            "rimera_cell_clear",
            rimera_runtime::rimera_cell_clear as *const () as *const u8,
        ),
        (
            "rimera_function_closure_get",
            rimera_runtime::rimera_function_closure_get as *const () as *const u8,
        ),
        (
            "rimera_global_set",
            rimera_runtime::rimera_global_set as *const () as *const u8,
        ),
        (
            "rimera_import_name",
            rimera_runtime::rimera_import_name as *const () as *const u8,
        ),
        (
            "rimera_import_dispatch",
            rimera_runtime::rimera_import_dispatch as *const () as *const u8,
        ),
        (
            "rimera_register_source_module",
            rimera_runtime::rimera_register_source_module as *const () as *const u8,
        ),
        (
            "rimera_register_namespace_module",
            rimera_runtime::rimera_register_namespace_module as *const () as *const u8,
        ),
        (
            "rimera_register_module_resource",
            rimera_runtime::rimera_register_module_resource as *const () as *const u8,
        ),
        (
            "rimera_import_source",
            rimera_runtime::rimera_import_source as *const () as *const u8,
        ),
        (
            "rimera_import_namespace",
            rimera_runtime::rimera_import_namespace as *const () as *const u8,
        ),
        (
            "rimera_import_from",
            rimera_runtime::rimera_import_from as *const () as *const u8,
        ),
        (
            "rimera_import_star",
            rimera_runtime::rimera_import_star as *const () as *const u8,
        ),
        (
            "rimera_global_get",
            rimera_runtime::rimera_global_get as *const () as *const u8,
        ),
        (
            "rimera_global_delete",
            rimera_runtime::rimera_global_delete as *const () as *const u8,
        ),
        (
            "rimera_namespace_new",
            rimera_runtime::rimera_namespace_new as *const () as *const u8,
        ),
        (
            "rimera_annotations_ensure",
            rimera_runtime::rimera_annotations_ensure as *const () as *const u8,
        ),
        (
            "rimera_namespace_set",
            rimera_runtime::rimera_namespace_set as *const () as *const u8,
        ),
        (
            "rimera_namespace_get",
            rimera_runtime::rimera_namespace_get as *const () as *const u8,
        ),
        (
            "rimera_class_name_get",
            rimera_runtime::rimera_class_name_get as *const () as *const u8,
        ),
        (
            "rimera_class_free_get",
            rimera_runtime::rimera_class_free_get as *const () as *const u8,
        ),
        (
            "rimera_namespace_delete",
            rimera_runtime::rimera_namespace_delete as *const () as *const u8,
        ),
        (
            "rimera_attr_get",
            rimera_runtime::rimera_attr_get as *const () as *const u8,
        ),
        (
            "rimera_special_method_get",
            rimera_runtime::rimera_special_method_get as *const () as *const u8,
        ),
        (
            "rimera_super_new",
            rimera_runtime::rimera_super_new as *const () as *const u8,
        ),
        (
            "rimera_attr_set",
            rimera_runtime::rimera_attr_set as *const () as *const u8,
        ),
        (
            "rimera_attr_delete",
            rimera_runtime::rimera_attr_delete as *const () as *const u8,
        ),
        (
            "rimera_dictionary_merge",
            rimera_runtime::rimera_dictionary_merge as *const () as *const u8,
        ),
        (
            "rimera_call_arguments_new",
            rimera_runtime::rimera_call_arguments_new as *const () as *const u8,
        ),
        (
            "rimera_call_argument_add",
            rimera_runtime::rimera_call_argument_add as *const () as *const u8,
        ),
        (
            "rimera_call_prepared",
            rimera_runtime::rimera_call_prepared as *const () as *const u8,
        ),
        (
            "rimera_call_positional_rooted",
            rimera_runtime::rimera_call_positional_rooted as *const () as *const u8,
        ),
        (
            "rimera_call_ready_coroutine_rooted",
            rimera_runtime::rimera_call_ready_coroutine_rooted as *const () as *const u8,
        ),
        (
            "rimera_call",
            rimera_runtime::rimera_call as *const () as *const u8,
        ),
        (
            "rimera_exception_new",
            rimera_runtime::rimera_exception_new as *const () as *const u8,
        ),
        (
            "rimera_raise",
            rimera_runtime::rimera_raise as *const () as *const u8,
        ),
        (
            "rimera_exception_active",
            rimera_runtime::rimera_exception_active as *const () as *const u8,
        ),
        (
            "rimera_reraise",
            rimera_runtime::rimera_reraise as *const () as *const u8,
        ),
        (
            "rimera_exception_propagate",
            rimera_runtime::rimera_exception_propagate as *const () as *const u8,
        ),
        (
            "rimera_handler_enter",
            rimera_runtime::rimera_handler_enter as *const () as *const u8,
        ),
        (
            "rimera_handler_leave",
            rimera_runtime::rimera_handler_leave as *const () as *const u8,
        ),
        (
            "rimera_exception_matches",
            rimera_runtime::rimera_exception_matches as *const () as *const u8,
        ),
        (
            "rimera_exception_split",
            rimera_runtime::rimera_exception_split as *const () as *const u8,
        ),
        (
            "rimera_exception_set_active",
            rimera_runtime::rimera_exception_set_active as *const () as *const u8,
        ),
        (
            "rimera_exception_merge_active",
            rimera_runtime::rimera_exception_merge_active as *const () as *const u8,
        ),
        (
            "rimera_exception_combine",
            rimera_runtime::rimera_exception_combine as *const () as *const u8,
        ),
        (
            "rimera_exception_clear_active",
            rimera_runtime::rimera_exception_clear_active as *const () as *const u8,
        ),
        (
            "rimera_traceback_append_module",
            rimera_runtime::rimera_traceback_append_module as *const () as *const u8,
        ),
        (
            "rimera_traceback_append",
            rimera_runtime::rimera_traceback_append as *const () as *const u8,
        ),
        (
            "rimera_roots_push",
            rimera_runtime::rimera_roots_push as *const () as *const u8,
        ),
        (
            "rimera_roots_pop",
            rimera_runtime::rimera_roots_pop as *const () as *const u8,
        ),
        (
            "rimera_int_from_decimal",
            rimera_runtime::rimera_int_from_decimal as *const () as *const u8,
        ),
        (
            "rimera_string_new",
            rimera_runtime::rimera_string_new as *const () as *const u8,
        ),
        (
            "rimera_float_new",
            rimera_runtime::rimera_float_new as *const () as *const u8,
        ),
        (
            "rimera_complex_new",
            rimera_runtime::rimera_complex_new as *const () as *const u8,
        ),
        (
            "rimera_bytes_new",
            rimera_runtime::rimera_bytes_new as *const () as *const u8,
        ),
        (
            "rimera_bytearray_new",
            rimera_runtime::rimera_bytearray_new as *const () as *const u8,
        ),
        (
            "rimera_slice_new",
            rimera_runtime::rimera_slice_new as *const () as *const u8,
        ),
        (
            "rimera_dictionary_view_new",
            rimera_runtime::rimera_dictionary_view_new as *const () as *const u8,
        ),
        (
            "rimera_memoryview_release",
            rimera_runtime::rimera_memoryview_release as *const () as *const u8,
        ),
        (
            "rimera_value_array_new",
            rimera_runtime::rimera_value_array_new as *const () as *const u8,
        ),
        (
            "rimera_tuple_new",
            rimera_runtime::rimera_tuple_new as *const () as *const u8,
        ),
        (
            "rimera_list_new",
            rimera_runtime::rimera_list_new as *const () as *const u8,
        ),
        (
            "rimera_list_append",
            rimera_runtime::rimera_list_append as *const () as *const u8,
        ),
        (
            "rimera_dict_new",
            rimera_runtime::rimera_dict_new as *const () as *const u8,
        ),
        (
            "rimera_dictionary_insert",
            rimera_runtime::rimera_dictionary_insert as *const () as *const u8,
        ),
        (
            "rimera_class_new",
            rimera_runtime::rimera_class_new as *const () as *const u8,
        ),
        (
            "rimera_set_new",
            rimera_runtime::rimera_set_new as *const () as *const u8,
        ),
        (
            "rimera_set_insert",
            rimera_runtime::rimera_set_insert as *const () as *const u8,
        ),
        (
            "rimera_unpack",
            rimera_runtime::rimera_unpack as *const () as *const u8,
        ),
        (
            "rimera_unpack_ex",
            rimera_runtime::rimera_unpack_ex as *const () as *const u8,
        ),
        (
            "rimera_pattern_sequence",
            rimera_runtime::rimera_pattern_sequence as *const () as *const u8,
        ),
        (
            "rimera_pattern_mapping_check",
            rimera_runtime::rimera_pattern_mapping_check as *const () as *const u8,
        ),
        (
            "rimera_pattern_mapping",
            rimera_runtime::rimera_pattern_mapping as *const () as *const u8,
        ),
        (
            "rimera_pattern_class",
            rimera_runtime::rimera_pattern_class as *const () as *const u8,
        ),
        (
            "rimera_range_new",
            rimera_runtime::rimera_range_new as *const () as *const u8,
        ),
        (
            "rimera_await_iterator",
            rimera_runtime::rimera_await_iterator as *const () as *const u8,
        ),
        (
            "rimera_async_iterator_new",
            rimera_runtime::rimera_async_iterator_new as *const () as *const u8,
        ),
        (
            "rimera_async_iterator_next",
            rimera_runtime::rimera_async_iterator_next as *const () as *const u8,
        ),
        (
            "rimera_iterator_new",
            rimera_runtime::rimera_iterator_new as *const () as *const u8,
        ),
        (
            "rimera_coroutine_close_elide_probe",
            rimera_runtime::rimera_coroutine_close_elide_probe as *const () as *const u8,
        ),
        (
            "rimera_range_collapse_probe",
            rimera_runtime::rimera_range_collapse_probe as *const () as *const u8,
        ),
        (
            "rimera_iterator_next",
            rimera_runtime::rimera_iterator_next as *const () as *const u8,
        ),
        (
            "rimera_length",
            rimera_runtime::rimera_length as *const () as *const u8,
        ),
        (
            "rimera_item_get",
            rimera_runtime::rimera_item_get as *const () as *const u8,
        ),
        (
            "rimera_item_set",
            rimera_runtime::rimera_item_set as *const () as *const u8,
        ),
        (
            "rimera_item_delete",
            rimera_runtime::rimera_item_delete as *const () as *const u8,
        ),
        (
            "rimera_contains",
            rimera_runtime::rimera_contains as *const () as *const u8,
        ),
        (
            "rimera_value_array_get",
            rimera_runtime::rimera_value_array_get as *const () as *const u8,
        ),
        (
            "rimera_unary",
            rimera_runtime::rimera_unary as *const () as *const u8,
        ),
        (
            "rimera_binary",
            rimera_runtime::rimera_binary as *const () as *const u8,
        ),
        (
            "rimera_inplace",
            rimera_runtime::rimera_inplace as *const () as *const u8,
        ),
        (
            "rimera_compare",
            rimera_runtime::rimera_compare as *const () as *const u8,
        ),
        (
            "rimera_format_value",
            rimera_runtime::rimera_format_value as *const () as *const u8,
        ),
        (
            "rimera_truthy",
            rimera_runtime::rimera_truthy as *const () as *const u8,
        ),
        (
            "rimera_print",
            rimera_runtime::rimera_print as *const () as *const u8,
        ),
        (
            "rimera_print_literal",
            rimera_runtime::rimera_print_literal as *const () as *const u8,
        ),
        (
            "rimera_collect",
            rimera_runtime::rimera_collect as *const () as *const u8,
        ),
        (
            "rimera_render_error",
            rimera_runtime::rimera_render_error as *const () as *const u8,
        ),
    ]
}
