use std::{collections::HashMap, fs::File, io::Write, ops::Deref, path::Path, sync::Arc};

use cranelift::{
    codegen::{
        ir::{
            AbiParam, BlockArg, Function, InstBuilder, Signature, StackSlot, StackSlotData,
            StackSlotKind, TrapCode, Type, UserExternalName, UserFuncName,
            condcodes::CondCode,
            types::{F32, F64, I8, I8X4, I16, I32, I64, I128, INVALID},
        },
        isa::{TargetFrontendConfig, TargetIsa},
    },
    frontend::{FunctionBuilder, FunctionBuilderContext},
    prelude::*,
};
use cranelift_module::{FuncId, Init, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use ir::{
    const_stage::{
        ConstValueKey, Context,
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
    signatures: HashMap<FunctionIrKey, Signature>,
}

pub struct Local {
    pub slot: StackSlot,
    pub ty: Type,
}

pub enum ValueLocation {
    StackSlot {
        slot: StackSlot,
        offset: usize,
        size: usize,
    },
    Value(Value),
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
            signatures: Default::default(),
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

        let mut file = File::create(out).unwrap();

        file.write_all(&bytes).unwrap();
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
                let vt = convert_type(&self.ctx.types, &self.frontend_config, *ty);
                if vt != INVALID {
                    sig.returns.push(AbiParam::new(vt));
                }
            }
            println!("{:?}", fun_key);
            dbg!(&sig);

            let id = self
                .module
                .declare_function(&format!("fun_{}", fun_key.id()), Linkage::Export, &sig)
                .unwrap();

            self.functions.insert(fun_key, id);
            self.signatures.insert(fun_key, sig);
        }
    }

    fn lower_function(&mut self, fun_key: FunctionIrKey) {
        let func_id = self.functions[&fun_key];
        let mut ctx = self.module.make_context();
        ctx.func.signature = self.signatures[&fun_key].clone();
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

            let locals: Vec<Local> = fun
                .variables
                .iter()
                .map(|var| {
                    let ty = convert_type(&self.ctx.types, &self.frontend_config, var.ty);
                    let layout = self
                        .layouts
                        .of(&var.ty, &self.ctx.types, &self.frontend_config);
                    let slot = builder.create_sized_stack_slot(StackSlotData {
                        kind: StackSlotKind::ExplicitSlot,
                        size: layout.size().expect("must have size") as _,
                        align_shift: layout.align().expect("must have align").bit_width() as _,
                        key: None,
                    });

                    Local { slot, ty }
                })
                .collect();

            let block_0 = *blocks.first().unwrap();
            builder.append_block_params_for_function_params(block_0);

            builder.switch_to_block(block_0);
            builder.seal_block(block_0);

            // Using frontend_config.pointer_type() for memory instructions
            for (param, local) in builder.block_params(block_0).to_vec().iter().zip(&locals) {
                builder.ins().stack_store(
                    self.frontend_config.pointer_type(),
                    *param,
                    local.slot,
                    0,
                );
            }

            let mut value_map: HashMap<ValueKey, ValueLocation> = HashMap::new();

            for (block_idx, block) in fun.blocks.arena().iter_pairs().map(|(k, s)| (k, &s.value)) {
                builder.switch_to_block(blocks[block_idx.id()]);

                for instr in block.instructions() {
                    match instr.deref() {
                        ir::ir::Instruction::LoadConst { src, dst } => {
                            load_const(&mut builder, &mut value_map, src, *dst, &self.ctx)
                        }
                        ir::ir::Instruction::BinOp { op, l, r, dst, ty } => {
                            let l = load_value_into_ssa(&mut builder, &value_map[l], self.ctx);
                            let r = load_value_into_ssa(&mut builder, &value_map[r], self.ctx);
                            let out = match (*op, is_float(*ty)) {
                                (ir::ast::Operator::Add, false) => builder.ins().iadd(l, r),
                                (ir::ast::Operator::Sub, false) => builder.ins().isub(l, r),
                                (ir::ast::Operator::Mul, false) => builder.ins().imul(l, r),
                                (ir::ast::Operator::Div, false) => builder.ins().sdiv(l, r),
                                (ir::ast::Operator::Mod, false) => todo!(),
                                (ir::ast::Operator::Eq, false) => {
                                    builder.ins().icmp(IntCC::Equal, l, r)
                                }
                                (ir::ast::Operator::NEq, false) => {
                                    builder.ins().icmp(IntCC::NotEqual, l, r)
                                }
                                (ir::ast::Operator::Gr, false) => {
                                    builder.ins().icmp(IntCC::SignedGreaterThan, l, r)
                                }
                                (ir::ast::Operator::Le, false) => {
                                    builder.ins().icmp(IntCC::SignedLessThan, l, r)
                                }
                                (ir::ast::Operator::GrEq, false) => {
                                    builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, l, r)
                                }
                                (ir::ast::Operator::LeEq, false) => {
                                    builder.ins().icmp(IntCC::SignedLessThanOrEqual, l, r)
                                }
                                (ir::ast::Operator::And, false) => todo!(),
                                (ir::ast::Operator::Or, false) => todo!(),
                                (ir::ast::Operator::Assign, false) => todo!(),
                                (ir::ast::Operator::AddAssign, false) => todo!(),
                                (ir::ast::Operator::SubAssign, false) => todo!(),
                                (ir::ast::Operator::MulAssign, false) => todo!(),
                                (ir::ast::Operator::DivAssign, false) => todo!(),
                                (ir::ast::Operator::ModAssign, false) => todo!(),
                                (ir::ast::Operator::BitOr, false) => todo!(),
                                (ir::ast::Operator::BitAnd, false) => todo!(),
                                _ => unreachable!(),
                            };
                            value_map.insert(*dst, out.into());
                        }
                        ir::ir::Instruction::UnaryOp { op, src, dst } => todo!(),

                        ir::ir::Instruction::StoreVar { dst, src } => {
                            let local = &locals[dst.id()];
                            let val = load_value_into_ssa(&mut builder, &value_map[src], self.ctx);
                            builder.ins().stack_store(
                                self.frontend_config.pointer_type(),
                                val,
                                local.slot,
                                0,
                            );
                        }

                        ir::ir::Instruction::LoadVar { src, dst } => {
                            let local = &locals[src.id()];
                            let val = builder.ins().stack_load(
                                self.frontend_config.pointer_type(),
                                local.ty,
                                local.slot,
                                0,
                            );
                            value_map.insert(*dst, val.into());
                        }
                        ir::ir::Instruction::Call {
                            fun,
                            arguments,
                            result,
                        } => {
                            let callee = self.functions[fun];
                            let func_ref = self.module.declare_func_in_func(callee, builder.func);

                            let collect = arguments
                                .iter()
                                .map(|arg| {
                                    let load_value_into_ssa = load_value_into_ssa(
                                        &mut builder,
                                        &value_map[arg],
                                        &self.ctx,
                                    );
                                    load_value_into_ssa
                                })
                                .collect::<Vec<_>>();
                            let call = builder.ins().call(func_ref, &collect);

                            let mut results = builder.inst_results(call).iter();
                            match results.next() {
                                Some(r) => {
                                    value_map.insert(*result, (*r).into());
                                }
                                None => (),
                            }
                        }
                        ir::ir::Instruction::AddressOfObj { obj, dst } => todo!(),
                        ir::ir::Instruction::AddressOfFun { fun, dst } => {
                            let fun = self.functions[fun];
                            let func_ref = self.module.declare_func_in_func(fun, builder.func);
                            let addr = builder
                                .ins()
                                .func_addr(self.frontend_config.pointer_type(), func_ref);
                            value_map.insert(*dst, addr.into());
                        }

                        ir::ir::Instruction::AddressOfVar { var, dst } => {
                            let local = &locals[var.id()];
                            let val = builder.ins().stack_addr(
                                self.frontend_config.pointer_type(),
                                local.slot,
                                0,
                            );
                            value_map.insert(*dst, val.into());
                        }

                        ir::ir::Instruction::AddressOfVal { val, dst } => todo!(),
                        ir::ir::Instruction::Deref { src, dst } => todo!(),
                    }
                }
                match block
                    .terminator()
                    .as_ref()
                    .expect("Block must be terminated")
                {
                    ir::ir::Terminator::Return(key) => {
                        match key {
                            Some(key) => {
                                let load_value_into_ssa =
                                    load_value_into_ssa(&mut builder, &value_map[key], self.ctx);
                                builder.ins().return_(&[load_value_into_ssa])
                            }
                            None => builder.ins().return_(&[]),
                        };
                    }
                    ir::ir::Terminator::Jump(key, _key1) => {
                        // Our target blocks do not declare parameters,
                        // so passing arguments here throws validation errors in Cranelift.
                        builder.ins().jump(blocks[key.id()], &[]);
                    }
                    ir::ir::Terminator::Branch {
                        condition,
                        then_block,
                        else_block,
                    } => {
                        let ssav =
                            load_value_into_ssa(&mut builder, &value_map[condition], self.ctx);
                        builder.ins().brif(
                            ssav,
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

            // finalize now expects frontend config
            builder.finalize(self.frontend_config);
        }
        println!("finalizing '{fun_key:?}'");
        match self.module.define_function(func_id, &mut ctx) {
            Ok(_) => (),
            Err(e) => {
                println!("func: {}", ctx.func);
                panic!("Unrecoverable cranelift err: {:?}", e)
            }
        }

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

fn load_value_into_ssa(
    builder: &mut FunctionBuilder<'_>,
    value: &ValueLocation,
    ctx: &Context,
) -> Value {
    match value {
        ValueLocation::StackSlot { slot, offset, size } => todo!(),
        ValueLocation::Value(value) => *value,
    }
}

fn is_float(ty: PrimitiveType) -> bool {
    match ty {
        PrimitiveType::F32 | PrimitiveType::F64 => true,
        _ => false,
    }
}

fn load_const(
    builder: &mut FunctionBuilder<'_>,
    value_map: &mut HashMap<ValueKey, ValueLocation>,
    src: &ConstValueKey,
    dst: ValueKey,
    ctx: &Context,
) {
    let value = ctx.constants.data.get_unchecked(src);
    match value {
        ir::ast::ConstValue::Structure { fields, ty } => todo!(),
        ir::ast::ConstValue::Number(number) => {
            let intsize = match number.size {
                Some(8) => I8,
                Some(16) => I16,
                Some(32) => I32,
                Some(64) => I64,
                Some(128) => I128,
                None => I32,
                _ => unreachable!(),
            };
            match number.value {
                ir::ast::NumberValue::Float(v) => {
                    let value = builder.ins().f64const(Ieee64::from(v));
                    value_map.insert(dst, value.into());
                }
                ir::ast::NumberValue::Uint(v) => {
                    let value = builder.ins().iconst(intsize, Imm64::new(v as _));
                    value_map.insert(dst, value.into());
                }
                ir::ast::NumberValue::Int(v) => {
                    let value = builder.ins().iconst(intsize, Imm64::new(v as _));
                    value_map.insert(dst, value.into());
                }
                ir::ast::NumberValue::Any(v) => {
                    let value = builder.ins().iconst(intsize, Imm64::new(v as _));
                    value_map.insert(dst, value.into());
                }
            }
        }
        ir::ast::ConstValue::String(smol_str) => {
            let bytes = smol_str.as_bytes();
        }
        ir::ast::ConstValue::Char(c) => {
            let value = builder.ins().iconst(I32, Imm64::new(*c as _));
            value_map.insert(dst, value.into());
        }
        ir::ast::ConstValue::Bool(b) => {
            let value = builder.ins().iconst(I8, Imm64::new(*b as _));
            value_map.insert(dst, value.into());
        }
        ir::ast::ConstValue::EnumVariant { parent, variant } => {
            let enum_obj = ctx.types.enums.get_unchecked(match parent {
                AnyTypeKey::Enum(e) => e,
                _ => unreachable!("expected enum type"),
            });
            let (_, value) = enum_obj
                .variants
                .iter()
                .find(|(v, _)| v == variant)
                .unwrap();
            load_const(builder, value_map, value, dst, ctx);
        }
        ir::ast::ConstValue::Array { elements, ty } => todo!(),
        ir::ast::ConstValue::Tuple { elements, ty } => todo!(),
    }
}

impl From<&Value> for ValueLocation {
    fn from(value: &Value) -> Self {
        ValueLocation::Value(*value)
    }
}

impl From<Value> for ValueLocation {
    fn from(value: Value) -> Self {
        ValueLocation::Value(value)
    }
}
