use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context as LLVMContext;
use inkwell::execution_engine::{ExecutionEngine, JitFunction};
use inkwell::module::Module;
use ir::const_stage::Context;
use ir::ir::FunctionIrKey;

// mod layouts;

use std::collections::HashMap;
use std::error::Error;

//use crate::layouts::Layouts;

pub struct LLVMLoweringContext<'a, 'ctx> {
    ctx: &'a Context,
    //layouts: Layouts,
    llvm: &'ctx LLVMContext,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
}

impl<'a, 'ctx> LLVMLoweringContext<'a, 'ctx> {
    pub fn new(ctx: &'a Context, llvm: &'ctx LLVMContext) -> Self {
        Self {
            ctx,
            module: llvm.create_module("A"),
            builder: llvm.create_builder(),
            llvm,
        }
    }

    pub fn lower(&mut self) {
        self.declaration_pass();
        for ir in self.ctx.ir_cache.iter_keys() {
            self.lower_function(ir);
        }
    }

    fn declaration_pass(&self) {}
    fn lower_function(&self, ir: FunctionIrKey) {}
}
