use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;

/// Wraps LLVM context, module, and builder for code generation.
pub struct CodegenContext<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
}

impl<'ctx> CodegenContext<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        Self {
            context,
            module,
            builder,
        }
    }

    /// Write the LLVM IR to a string for debugging.
    pub fn dump_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }
}
