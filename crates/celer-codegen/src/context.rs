use std::collections::HashMap;

use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::PointerValue;

/// Tracks the LLVM basic blocks for break/continue inside loops.
pub struct LoopContext<'ctx> {
    pub cond_bb: BasicBlock<'ctx>,
    pub exit_bb: BasicBlock<'ctx>,
}

/// Wraps LLVM context, module, and builder for code generation.
/// Tracks local variables, a string constant pool, and loop stack.
pub struct CodegenContext<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    locals: HashMap<String, PointerValue<'ctx>>,
    string_counter: u32,
    loop_stack: Vec<LoopContext<'ctx>>,
}

impl<'ctx> CodegenContext<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        Self {
            context,
            module,
            builder,
            locals: HashMap::new(),
            string_counter: 0,
            loop_stack: Vec::new(),
        }
    }

    /// Write the LLVM IR to a string for debugging.
    pub fn dump_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }

    /// Store a local variable pointer by name.
    pub fn set_local(&mut self, name: &str, ptr: PointerValue<'ctx>) {
        self.locals.insert(name.to_string(), ptr);
    }

    /// Look up a local variable pointer by name.
    pub fn get_local(&self, name: &str) -> Option<PointerValue<'ctx>> {
        self.locals.get(name).copied()
    }

    /// Clear local variable bindings (between functions).
    pub fn clear_locals(&mut self) {
        self.locals.clear();
    }

    pub fn push_loop(&mut self, cond_bb: BasicBlock<'ctx>, exit_bb: BasicBlock<'ctx>) {
        self.loop_stack.push(LoopContext { cond_bb, exit_bb });
    }

    pub fn pop_loop(&mut self) {
        self.loop_stack.pop();
    }

    pub fn current_loop(&self) -> Option<&LoopContext<'ctx>> {
        self.loop_stack.last()
    }

    /// Add a global string constant and return a pointer to it.
    /// Uses raw byte array globals so the string is NOT null-terminated,
    /// preserving exact byte length for JSON output.
    pub fn add_string_constant(&mut self, value: &str, name: &str) -> PointerValue<'ctx> {
        let unique_name = format!("{}_{}", name, self.string_counter);
        self.string_counter += 1;

        let bytes = value.as_bytes();
        let i8_ty = self.context.i8_type();
        let byte_vals: Vec<inkwell::values::IntValue<'ctx>> = bytes
            .iter()
            .map(|&b| i8_ty.const_int(b as u64, false))
            .collect();
        let const_array = i8_ty.const_array(&byte_vals);

        let global =
            self.module
                .add_global(i8_ty.array_type(bytes.len() as u32), None, &unique_name);
        global.set_initializer(&const_array);
        global.set_constant(true);
        global.set_unnamed_addr(true);

        global.as_pointer_value()
    }
}
