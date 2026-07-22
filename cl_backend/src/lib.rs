use std::ops::Deref;

use cranelift::{
    codegen::{
        ir::{
            AbiParam, Function, Signature, Type, UserExternalName, UserFuncName,
            types::{F32, F64, I8, I16, I32, I64, I128},
        },
        isa::TargetFrontendConfig,
    },
    frontend::{FunctionBuilder, FunctionBuilderContext},
    prelude::*,
};
use ir::{
    const_stage::{
        Context,
        types::{AnyTypeKey, Types},
    },
    ir::FunctionIrKey,
};

pub struct CLLoweringCtx<'a> {
    ctx: &'a mut Context,
    frontend_config: TargetFrontendConfig,
    fb_ctx: FunctionBuilderContext,
}

impl<'a> CLLoweringCtx<'a> {
    pub fn compile(ctx: &'a mut Context) {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut flag_bldr = settings::builder();
        let flags = settings::Flags::new(flag_bldr);

        let isa_bldr = isa::lookup(target_lexicon::Triple::host()).unwrap();
        let isa = isa_bldr.finish(flags).unwrap();
        let frontend_config = isa.frontend_config();

        let fb_ctx = FunctionBuilderContext::new();

        let Self {
            ctx,
            frontend_config,
            fb_ctx,
        };
    }

    fn lower_function(&mut self, fun_key: FunctionIrKey) {
        let mut sig = Signature::new(cranelift::codegen::isa::CallConv::SystemV);
        let fun = self.ctx.ir_cache.get_unchecked(&fun_key);
        for (_, var) in &fun.parameters {
            let ty = fun.variables.get_unchecked(var).ty;
            sig.params
                .push(AbiParam::new(convert_type(&self.ctx.types, ty)));
        }
        if let Some(ty) = &fun.returns {
            sig.returns
                .push(AbiParam::new(convert_type(&self.ctx.types, *ty)));
        }

        let mut func = Function::with_name_signature(
            UserFuncName::User(UserExternalName::new(0, fun_key.id() as _)),
            sig,
        );
        {
            let mut builder = FunctionBuilder::new(&mut func, &mut self.fb_ctx);
            let blocks: Vec<_> = fun
                .blocks
                .arena()
                .iter()
                .map(|_| builder.create_block())
                .collect();
            let variables: Vec<_> = fun
                .variables
                .iter()
                .map(|var| builder.declare_var(convert_type(&self.ctx.types, var.ty)))
                .collect();

            let block_0 = *blocks.first().unwrap();
            builder.append_block_params_for_function_params(block_0);

            builder.switch_to_block(block_0);
            builder.seal_block(block_0);

            for (param, variable) in builder
                .block_params(block_0)
                .to_vec()
                .iter()
                .zip(&variables)
            {
                builder.def_var(*variable, *param);
            }

            for block in fun.blocks.arena().iter().map(|s| &s.value) {
                for instr in block.instructions() {
                    match instr.deref() {
                        ir::ir::Instruction::LoadConst { src, dst } => todo!(),
                        ir::ir::Instruction::BinOp { op, l, r, dst } => todo!(),
                        ir::ir::Instruction::UnaryOp { op, src, dst } => todo!(),
                        ir::ir::Instruction::StoreVar { dst, src } => todo!(),
                        ir::ir::Instruction::LoadVar { src, dst } => todo!(),
                        ir::ir::Instruction::Call {
                            fun,
                            arguments,
                            result,
                        } => todo!(),
                        ir::ir::Instruction::AddressOfObj { obj, dst } => todo!(),
                        ir::ir::Instruction::AddressOfFun { fun, dst } => todo!(),
                        ir::ir::Instruction::AddressOfVar { var, dst } => todo!(),
                        ir::ir::Instruction::AddressOfVal { val, dst } => todo!(),
                        ir::ir::Instruction::Deref { src, dst } => todo!(),
                        ir::ir::Instruction::Exit(key) => todo!(),
                    }
                }
            }

            builder.finalize(self.frontend_config);
        }
    }
}

fn convert_type(types: &Types, ty: AnyTypeKey) -> Type {
    match ty {
        AnyTypeKey::Primitive(primitive_type) => match primitive_type {
            ir::const_stage::types::PrimitiveType::I8 => I8,
            ir::const_stage::types::PrimitiveType::I16 => I16,
            ir::const_stage::types::PrimitiveType::I32 => I32,
            ir::const_stage::types::PrimitiveType::I64 => I64,
            ir::const_stage::types::PrimitiveType::I128 => I128,
            ir::const_stage::types::PrimitiveType::U8 => I8,
            ir::const_stage::types::PrimitiveType::U16 => I16,
            ir::const_stage::types::PrimitiveType::U32 => I32,
            ir::const_stage::types::PrimitiveType::U64 => I64,
            ir::const_stage::types::PrimitiveType::U128 => I128,
            ir::const_stage::types::PrimitiveType::F32 => F32,
            ir::const_stage::types::PrimitiveType::F64 => F64,
            ir::const_stage::types::PrimitiveType::Char => I32,
            ir::const_stage::types::PrimitiveType::Bool => I8,
            _ => todo!("{ty:?}"),
        },
        _ => todo!("{ty:?}"),
    }
}
