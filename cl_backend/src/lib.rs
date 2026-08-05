use std::{collections::HashMap, ops::Deref, path::Path, sync::Arc};

use cranelift::{
    codegen::{
        ir::{
            AbiParam, BlockArg, Function, Signature, StackSlot, Type, UserExternalName,
            UserFuncName,
            types::{F32, F64, I8, I8X4, I16, I32, I64, I128},
        },
        isa::{TargetFrontendConfig, TargetIsa},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext},
    prelude::*,
};
use cranelift_module::{FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use ir::{
    const_stage::{
        Context,
        types::{AnyTypeKey, PrimitiveType, Types, Vector},
    },
    ir::{FunctionIrKey, ValueKey},
};

use crate::layouts::Layouts;

mod layouts;

pub struct CLLoweringCtx<'a> {
    ctx: &'a Context,
    frontend_config: TargetFrontendConfig,
    isa: Arc<dyn TargetIsa>,
    fb_ctx: FunctionBuilderContext,
    layouts: Layouts,
    module: ObjectModule,
    functions: HashMap<FunctionIrKey, FuncId>,
}

pub enum Local {
    Variable(Variable),
    StackSlot(StackSlot),
}

impl<'a> CLLoweringCtx<'a> {
    pub fn init(ctx: &'a Context) -> Self {
        let mut fb_ctx = FunctionBuilderContext::new();
        let mut flag_builder = settings::builder();
        let flags = settings::Flags::new(flag_builder);

        let isa_builder = isa::lookup(target_lexicon::Triple::host()).unwrap();
        let isa = isa_builder.finish(flags).unwrap();

        let frontend_config = isa.frontend_config();

        let builder = ObjectBuilder::new(
            isa.clone(),
            "my_module",
            cranelift_module::default_libcall_names(),
        )
        .unwrap();

        let module = ObjectModule::new(builder);

        let fb_ctx = FunctionBuilderContext::new();

        let mut this = Self {
            ctx,
            frontend_config,
            fb_ctx,
            layouts: Layouts::default(),
            functions: HashMap::default(),
            isa,
            module,
        };

        this.declaration_pass();
        this
    }

    pub fn lower(&mut self) {
        for ir in self.ctx.ir_cache.iter_keys() {
            self.lower_function(ir);
        }
    }

    pub fn emit(self, out: impl AsRef<Path>) {
        let object = self.module.finish();

        let bytes = object.emit().unwrap();

        std::fs::write(out, bytes).unwrap();
    }

    fn declaration_pass(&mut self) {
        for (fun_key, fun) in self.ctx.ir_cache.iter_pairs() {
            let mut sig = self.module.make_signature();

            for (_, var) in &fun.parameters {
                let ty = fun.variables.get_unchecked(var).ty;
                sig.params.push(AbiParam::new(convert_type(
                    &self.ctx.types,
                    &self.frontend_config,
                    ty,
                )));
            }
            if let Some(ty) = &fun.returns {
                sig.returns.push(AbiParam::new(convert_type(
                    &self.ctx.types,
                    &self.frontend_config,
                    *ty,
                )));
            }

            let id = self
                .module
                .declare_function(&format!("fun_{}", fun_key.id()), Linkage::Export, &sig)
                .unwrap();

            self.functions.insert(fun_key, id);
        }
    }

    fn lower_function(&mut self, fun_key: FunctionIrKey) {
        let func_id = self.functions[&fun_key];
        let mut ctx = self.module.make_context();
        let fun = self.ctx.ir_cache.get_unchecked(&fun_key);

        ctx.func.name = UserFuncName::user(0, func_id.as_u32());
        {
            let mut builder = FunctionBuilder::new(&mut ctx.func, &mut self.fb_ctx);

            let blocks: Vec<_> = fun
                .blocks
                .arena()
                .iter()
                .map(|_| builder.create_block())
                .collect();
            let locals: Vec<_> = fun
                .variables
                .iter()
                .map(|var| match var.needs_address {
                    false => Local::Variable(builder.declare_var(convert_type(
                        &self.ctx.types,
                        &self.frontend_config,
                        var.ty,
                    ))),
                    true => {
                        let layout =
                            self.layouts
                                .of(&var.ty, &self.ctx.types, &self.frontend_config);
                        Local::StackSlot(builder.create_sized_stack_slot(StackSlotData {
                            kind: StackSlotKind::ExplicitSlot,
                            size: layout.size().expect("must have size") as _,
                            align_shift: layout.align().expect("must have align").bit_width() as _,
                            key: None,
                        }))
                    }
                })
                .collect();

            let block_0 = *blocks.first().unwrap();
            builder.append_block_params_for_function_params(block_0);

            builder.switch_to_block(block_0);
            builder.seal_block(block_0);

            for (param, local) in builder.block_params(block_0).to_vec().iter().zip(&locals) {
                match local {
                    Local::Variable(variable) => {
                        builder.def_var(*variable, *param);
                    }
                    Local::StackSlot(stack_slot) => {
                        builder.ins().stack_store(
                            self.frontend_config.pointer_type(),
                            *param,
                            *stack_slot,
                            0,
                        );
                    }
                }
            }

            let mut value_map = HashMap::new();

            for (block_idx, block) in fun.blocks.arena().iter_pairs().map(|(k, s)| (k, &s.value)) {
                builder.switch_to_block(blocks[block_idx.id()]);

                for instr in block.instructions() {
                    match instr.deref() {
                        ir::ir::Instruction::LoadConst { src, dst } => {
                            load_const(&mut builder, &mut value_map, src, *dst, &self.ctx.types)
                        }
                        ir::ir::Instruction::BinOp { op, l, r, dst } => todo!(),
                        ir::ir::Instruction::UnaryOp { op, src, dst } => todo!(),
                        ir::ir::Instruction::StoreVar { dst, src } => todo!(),
                        ir::ir::Instruction::LoadVar { src, dst } => todo!(),
                        ir::ir::Instruction::Call {
                            fun,
                            arguments,
                            result,
                        } => {
                            let callee = self.functions[fun];
                            let func_ref = self.module.declare_func_in_func(callee, builder.func);

                            let call = builder.ins().call(
                                func_ref,
                                &arguments
                                    .iter()
                                    .map(|arg| value_map[arg])
                                    .collect::<Vec<_>>(),
                            );

                            let results = builder.inst_results(call);
                        }
                        ir::ir::Instruction::AddressOfObj { obj, dst } => todo!(),
                        ir::ir::Instruction::AddressOfFun { fun, dst } => {
                            let fun = self.functions[fun];
                            let func_ref = self.module.declare_func_in_func(fun, builder.func);
                            let addr = builder
                                .ins()
                                .func_addr(self.frontend_config.pointer_type(), func_ref);
                            value_map.insert(*dst, addr);
                        }
                        ir::ir::Instruction::AddressOfVar { var, dst } => todo!(),
                        ir::ir::Instruction::AddressOfVal { val, dst } => todo!(),
                        ir::ir::Instruction::Deref { src, dst } => todo!(),
                    }
                }
                match block.terminator().as_ref().unwrap() {
                    ir::ir::Terminator::Return(key) => {
                        match key {
                            Some(key) => builder.ins().return_(&[*value_map.get(key).unwrap()]),
                            None => builder.ins().return_(&[]),
                        };
                    }
                    ir::ir::Terminator::Jump(key, key1) => {
                        match key1 {
                            Some(arg) => builder
                                .ins()
                                .jump(blocks[key.id()], [&BlockArg::Value(value_map[arg])]),
                            None => builder.ins().jump(blocks[key.id()], &[]),
                        };
                    }
                    ir::ir::Terminator::Branch {
                        condition,
                        then_block,
                        else_block,
                    } => {
                        builder.ins().brif(
                            *value_map.get(condition).unwrap(),
                            blocks[then_block.id()],
                            &[],
                            blocks[else_block.id()],
                            &[],
                        );
                    }
                    ir::ir::Terminator::Unreachable => {
                        builder
                            .ins()
                            .trap(TrapCode::user(10).expect("bad user trapcode"));
                    }
                    ir::ir::Terminator::Exit(key) => {
                        builder
                            .ins()
                            .trap(TrapCode::user(*key).expect("bad user trapcode"));
                    }
                };
            }

            builder.finalize(self.frontend_config);
        }

        self.module.define_function(func_id, &mut ctx).unwrap();

        self.module.clear_context(&mut ctx);
    }
}

fn convert_type(types: &Types, frontend_config: &TargetFrontendConfig, ty: AnyTypeKey) -> Type {
    match ty.unwrap_full(types) {
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
        AnyTypeKey::Reference(_) => frontend_config.pointer_type(),
        AnyTypeKey::Void | AnyTypeKey::Never => cranelift::codegen::ir::types::INVALID,
        AnyTypeKey::Vector(Vector {
            element: PrimitiveType::U8,
            lanes: 4,
        }) => I8X4,
        _ => todo!("{ty:?}"),
    }
}

fn load_const(
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueKey, Value>,
    src: &ir::ast::ConstValue,
    dst: ValueKey,
    types: &Types,
) {
    match src {
        ir::ast::ConstValue::Structure { fields, ty } => todo!(),
        ir::ast::ConstValue::Number(number) => match number.value {
            ir::ast::NumberValue::Float(v) => {
                let value = builder.ins().f64const(Ieee64::from(v));
                value_map.insert(dst, value);
            }
            ir::ast::NumberValue::Uint(v) => {
                let value = builder.ins().iconst(I64, Imm64::new(v as _));
                value_map.insert(dst, value);
            }
            ir::ast::NumberValue::Int(v) => {
                let value = builder.ins().iconst(I64, Imm64::new(v as _));
                value_map.insert(dst, value);
            }
            ir::ast::NumberValue::Any(v) => {
                let value = builder.ins().iconst(I64, Imm64::new(v as _));
                value_map.insert(dst, value);
            }
        },
        ir::ast::ConstValue::String(smol_str) => todo!(),
        ir::ast::ConstValue::Char(c) => {
            let value = builder.ins().iconst(I32, Imm64::new(*c as _));
            value_map.insert(dst, value);
        }
        ir::ast::ConstValue::Bool(b) => {
            let value = builder.ins().iconst(I8, Imm64::new(*b as _));
            value_map.insert(dst, value);
        }
        ir::ast::ConstValue::EnumVariant { parent, variant } => {
            let enum_obj = types.enums.get_unchecked(match parent {
                AnyTypeKey::Enum(e) => e,
                _ => unreachable!("expected enum type"),
            });
            let (_, value) = enum_obj
                .variants
                .iter()
                .find(|(v, _)| v == variant)
                .unwrap();
            load_const(builder, value_map, value, dst, types);
        }
        ir::ast::ConstValue::Array { elements, ty } => todo!(),
        ir::ast::ConstValue::Tuple { elements, ty } => todo!(),
    }
}
