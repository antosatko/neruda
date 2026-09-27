use std::error::Error;
use std::path::Path;
use std::{collections::HashMap, ops::Deref};

use inkwell::basic_block::BasicBlock;
pub use inkwell::context::Context as LLVMContext;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::{
    AddressSpace, FloatPredicate, IntPredicate, OptimizationLevel,
    builder::Builder,
    module::Module,
    types::{BasicType, BasicTypeEnum},
    values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue},
};

use ir::{
    const_stage::{
        ConstValueKey, Context,
        types::{AnyTypeKey, PrimitiveType, Vector},
    },
    ir::{FunctionIrKey, ValueKey},
};

pub struct LLVMLoweringContext<'a, 'ctx> {
    ctx: &'a Context,
    llvm: &'ctx LLVMContext,
    module: Module<'ctx>,
    builder: Builder<'ctx>,

    functions: HashMap<FunctionIrKey, FunctionValue<'ctx>>,
}

pub enum ValueLocation<'ctx> {
    Value(BasicValueEnum<'ctx>),
    Pointer(PointerValue<'ctx>),
}

pub struct Local<'ctx> {
    pub ptr: PointerValue<'ctx>,
    pub ty: BasicTypeEnum<'ctx>,
}

impl<'a, 'ctx> LLVMLoweringContext<'a, 'ctx> {
    pub fn new(ctx: &'a Context, llvm: &'ctx LLVMContext) -> Self {
        Self {
            ctx,
            llvm,
            module: llvm.create_module("my_module"),
            builder: llvm.create_builder(),
            functions: HashMap::new(),
        }
    }

    pub fn lower(&mut self) -> Result<(), Box<dyn Error>> {
        self.declaration_pass()?;

        for ir in self.ctx.ir_cache.iter_keys() {
            self.lower_function(ir)?;
        }

        self.module.verify()?;
        Ok(())
    }

    pub fn module(&self) -> &Module<'ctx> {
        &self.module
    }

    fn declaration_pass(&mut self) -> Result<(), Box<dyn Error>> {
        for (fun_key, fun) in self.ctx.ir_cache.iter_pairs() {
            let mut params = Vec::new();

            for (_, var) in &fun.parameters {
                let ty = fun.variables.get_unchecked(var).ty;
                params.push(self.convert_type(ty)?.into());
            }

            let return_type = match fun.returns {
                Some(ty) => Some(self.convert_type(ty)?),
                None => None,
            };

            let fn_type = match return_type {
                Some(ty) => ty.fn_type(&params, false),
                None => self.llvm.void_type().fn_type(&params, false),
            };

            let label = match fun.get_label(self.ctx) {
                Some(label) => label.to_string(),
                None => format!("fun_{}", fun_key.id()),
            };

            let function = self.module.add_function(&label, fn_type, None);

            self.functions.insert(fun_key, function);
        }

        Ok(())
    }

    fn lower_function(&mut self, fun_key: FunctionIrKey) -> Result<(), Box<dyn Error>> {
        let function = self.functions[&fun_key];
        let fun = self.ctx.ir_cache.get_unchecked(&fun_key);

        let blocks: Vec<BasicBlock<'ctx>> = fun
            .blocks
            .arena()
            .iter()
            .map(|_| self.llvm.append_basic_block(function, "block"))
            .collect();

        let mut locals = Vec::new();

        // Allocate locals in the entry block.
        let entry = *blocks.first().ok_or("Function has no blocks")?;
        self.builder.position_at_end(entry);

        for var in fun.variables.iter() {
            let ty = self.convert_type(var.ty)?;
            let ptr = self.builder.build_alloca(ty, "local")?;

            locals.push(Local { ptr, ty });
        }

        // Store function parameters in their corresponding locals.
        for (param, (_, var)) in function.get_params().iter().zip(&fun.parameters) {
            let local = &locals[var.id()];
            self.builder.build_store(local.ptr, *param)?;
        }

        let mut value_map: HashMap<ValueKey, ValueLocation<'ctx>> = HashMap::new();

        for (block_idx, block) in fun.blocks.arena().iter_pairs().map(|(k, s)| (k, &s.value)) {
            self.builder.position_at_end(blocks[block_idx.id()]);

            for instr in block.instructions() {
                match instr.deref() {
                    ir::ir::Instruction::LoadConst { src, dst } => {
                        self.load_const(&mut value_map, src, *dst)?;
                    }
                    ir::ir::Instruction::BinOp { op, l, r, dst, ty } => {
                        let l = self.load_value(&value_map[l])?;
                        let r = self.load_value(&value_map[r])?;

                        let out = self.lower_binop(*op, l, r, *ty)?;

                        value_map.insert(*dst, ValueLocation::Value(out));
                    }
                    ir::ir::Instruction::UnaryOp { op, src, dst, ty } => {
                        let src = self.load_value(&value_map[src])?;
                        let out = self.lower_unary(*op, src, *ty)?;

                        value_map.insert(*dst, ValueLocation::Value(out));
                    }
                    ir::ir::Instruction::StoreVar { dst, src } => {
                        let value = self.load_value(&value_map[src])?;
                        let local = &locals[dst.id()];

                        self.builder.build_store(local.ptr, value)?;
                    }
                    ir::ir::Instruction::LoadVar { src, dst } => {
                        let local = &locals[src.id()];

                        let value = self.builder.build_load(local.ty, local.ptr, "load")?;

                        value_map.insert(*dst, ValueLocation::Value(value));
                    }
                    ir::ir::Instruction::Call {
                        fun,
                        arguments,
                        result,
                    } => {
                        let callee = self.functions[fun];

                        let args = arguments
                            .iter()
                            .map(|arg| {
                                self.load_value(&value_map[arg])
                                    .map(BasicMetadataValueEnum::from)
                            })
                            .collect::<Result<Vec<_>, _>>()?;

                        let call = self.builder.build_call(callee, &args, "call")?;

                        if let Some(value) = call.try_as_basic_value().basic() {
                            value_map.insert(*result, ValueLocation::Value(value));
                        }
                    }
                    ir::ir::Instruction::AddressOfObj { obj, dst } => {
                        todo!("AddressOfObj: {obj:?}");
                    }
                    ir::ir::Instruction::AddressOfFun { fun, dst } => {
                        let function = self.functions[fun];

                        let ptr = function.as_global_value().as_pointer_value();

                        value_map.insert(*dst, ValueLocation::Pointer(ptr));
                    }
                    ir::ir::Instruction::AddressOfVar { var, dst } => {
                        let ptr = locals[var.id()].ptr;

                        value_map.insert(*dst, ValueLocation::Pointer(ptr));
                    }
                    ir::ir::Instruction::AddressOfVal { val, dst } => {
                        todo!("AddressOfVal: {val:?}");
                    }
                    ir::ir::Instruction::Deref { src, dst } => {
                        let ptr = self.load_pointer(&value_map[src])?;

                        let dst_ty = fun.values.get_unchecked(dst).ty;
                        dbg!(dst_ty.unwrap_full(&self.ctx.types));
                        let ty = self.convert_type(dst_ty)?;

                        let value = self.builder.build_load(ty, ptr, "deref")?;

                        value_map.insert(*dst, ValueLocation::Value(value));
                    }
                }
            }

            match block.terminator().as_ref() {
                Some(ir::ir::Terminator::Return(key)) => match key {
                    Some(key) => {
                        let value = self.load_value(&value_map[key])?;
                        self.builder.build_return(Some(&value))?;
                    }
                    None => {
                        self.builder.build_return(None)?;
                    }
                },

                Some(ir::ir::Terminator::Jump(key, _)) => {
                    self.builder.build_unconditional_branch(blocks[key.id()])?;
                }

                Some(ir::ir::Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                }) => {
                    let condition = self.load_value(&value_map[condition])?.into_int_value();

                    self.builder.build_conditional_branch(
                        condition,
                        blocks[then_block.id()],
                        blocks[else_block.id()],
                    )?;
                }

                Some(ir::ir::Terminator::Unreachable) => {
                    self.builder.build_unreachable()?;
                }

                Some(ir::ir::Terminator::Exit(_key)) => {
                    todo!("Exit terminator");
                }

                None => {}
            }
        }

        Ok(())
    }

    fn convert_type(&self, ty: AnyTypeKey) -> Result<BasicTypeEnum<'ctx>, Box<dyn Error>> {
        let ty = ty.unwrap_full(&self.ctx.types);

        let ty = match ty {
            AnyTypeKey::Primitive(primitive) => match primitive {
                PrimitiveType::I8 | PrimitiveType::U8 => self.llvm.i8_type().into(),

                PrimitiveType::I16 | PrimitiveType::U16 => self.llvm.i16_type().into(),

                PrimitiveType::I32 | PrimitiveType::U32 => self.llvm.i32_type().into(),

                PrimitiveType::I64 | PrimitiveType::U64 => self.llvm.i64_type().into(),

                PrimitiveType::I128 | PrimitiveType::U128 => self.llvm.i128_type().into(),

                PrimitiveType::F32 => self.llvm.f32_type().into(),

                PrimitiveType::F64 => self.llvm.f64_type().into(),

                PrimitiveType::Char => self.llvm.i32_type().into(),

                PrimitiveType::Bool => self.llvm.i8_type().into(),

                _ => return Err(format!("Unsupported type: {primitive:?}").into()),
            },

            AnyTypeKey::Reference(_) => self.llvm.ptr_type(AddressSpace::default()).into(),

            AnyTypeKey::Vector(Vector {
                element: PrimitiveType::U8,
                lanes: 4,
            }) => self.llvm.i8_type().vec_type(4).into(),

            AnyTypeKey::Void | AnyTypeKey::Never => {
                return Err("Void/Never cannot be used as a basic type".into());
            }

            _ => return Err(format!("Unsupported type: {ty:?}").into()),
        };

        Ok(ty)
    }

    fn load_value(
        &self,
        value: &ValueLocation<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
        match value {
            ValueLocation::Value(value) => Ok(*value),

            ValueLocation::Pointer(_) => Err("Expected a value, found a pointer".into()),
        }
    }

    fn load_pointer(
        &self,
        value: &ValueLocation<'ctx>,
    ) -> Result<PointerValue<'ctx>, Box<dyn Error>> {
        match value {
            ValueLocation::Pointer(ptr) => Ok(*ptr),

            ValueLocation::Value(value) => Ok(value.into_pointer_value()),
        }
    }

    fn lower_binop(
        &self,
        op: ir::ast::Operator,
        l: BasicValueEnum<'ctx>,
        r: BasicValueEnum<'ctx>,
        ty: PrimitiveType,
    ) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
        let is_float = matches!(ty, PrimitiveType::F32 | PrimitiveType::F64);

        let out = match (op, is_float) {
            (ir::ast::Operator::Add, false) => self
                .builder
                .build_int_add(l.into_int_value(), r.into_int_value(), "add")?
                .into(),

            (ir::ast::Operator::Sub, false) => self
                .builder
                .build_int_sub(l.into_int_value(), r.into_int_value(), "sub")?
                .into(),

            (ir::ast::Operator::Mul, false) => self
                .builder
                .build_int_mul(l.into_int_value(), r.into_int_value(), "mul")?
                .into(),

            (ir::ast::Operator::Div, false) => self
                .builder
                .build_int_signed_div(l.into_int_value(), r.into_int_value(), "div")?
                .into(),

            (ir::ast::Operator::Mod, false) => self
                .builder
                .build_int_signed_rem(l.into_int_value(), r.into_int_value(), "mod")?
                .into(),

            (ir::ast::Operator::Add, true) => self
                .builder
                .build_float_add(l.into_float_value(), r.into_float_value(), "fadd")?
                .into(),

            (ir::ast::Operator::Sub, true) => self
                .builder
                .build_float_sub(l.into_float_value(), r.into_float_value(), "fsub")?
                .into(),

            (ir::ast::Operator::Mul, true) => self
                .builder
                .build_float_mul(l.into_float_value(), r.into_float_value(), "fmul")?
                .into(),

            (ir::ast::Operator::Div, true) => self
                .builder
                .build_float_div(l.into_float_value(), r.into_float_value(), "fdiv")?
                .into(),

            (ir::ast::Operator::Mod, true) => self
                .builder
                .build_float_rem(l.into_float_value(), r.into_float_value(), "frem")?
                .into(),

            (ir::ast::Operator::Eq, false) => self
                .builder
                .build_int_compare(
                    IntPredicate::EQ,
                    l.into_int_value(),
                    r.into_int_value(),
                    "eq",
                )?
                .into(),

            (ir::ast::Operator::NEq, false) => self
                .builder
                .build_int_compare(
                    IntPredicate::NE,
                    l.into_int_value(),
                    r.into_int_value(),
                    "neq",
                )?
                .into(),

            (ir::ast::Operator::Gr, false) => self
                .builder
                .build_int_compare(
                    IntPredicate::SGT,
                    l.into_int_value(),
                    r.into_int_value(),
                    "gt",
                )?
                .into(),

            (ir::ast::Operator::Le, false) => self
                .builder
                .build_int_compare(
                    IntPredicate::SLT,
                    l.into_int_value(),
                    r.into_int_value(),
                    "lt",
                )?
                .into(),

            (ir::ast::Operator::GrEq, false) => self
                .builder
                .build_int_compare(
                    IntPredicate::SGE,
                    l.into_int_value(),
                    r.into_int_value(),
                    "gte",
                )?
                .into(),

            (ir::ast::Operator::LeEq, false) => self
                .builder
                .build_int_compare(
                    IntPredicate::SLE,
                    l.into_int_value(),
                    r.into_int_value(),
                    "lte",
                )?
                .into(),

            (ir::ast::Operator::Eq, true) => self
                .builder
                .build_float_compare(
                    FloatPredicate::OEQ,
                    l.into_float_value(),
                    r.into_float_value(),
                    "feq",
                )?
                .into(),

            (ir::ast::Operator::NEq, true) => self
                .builder
                .build_float_compare(
                    FloatPredicate::ONE,
                    l.into_float_value(),
                    r.into_float_value(),
                    "fneq",
                )?
                .into(),

            (ir::ast::Operator::Gr, true) => self
                .builder
                .build_float_compare(
                    FloatPredicate::OGT,
                    l.into_float_value(),
                    r.into_float_value(),
                    "fgt",
                )?
                .into(),

            (ir::ast::Operator::Le, true) => self
                .builder
                .build_float_compare(
                    FloatPredicate::OLT,
                    l.into_float_value(),
                    r.into_float_value(),
                    "flt",
                )?
                .into(),

            (ir::ast::Operator::GrEq, true) => self
                .builder
                .build_float_compare(
                    FloatPredicate::OGE,
                    l.into_float_value(),
                    r.into_float_value(),
                    "fgte",
                )?
                .into(),

            (ir::ast::Operator::LeEq, true) => self
                .builder
                .build_float_compare(
                    FloatPredicate::OLE,
                    l.into_float_value(),
                    r.into_float_value(),
                    "flte",
                )?
                .into(),

            (ir::ast::Operator::And, false) => self
                .builder
                .build_and(l.into_int_value(), r.into_int_value(), "and")?
                .into(),

            (ir::ast::Operator::Or, false) => self
                .builder
                .build_or(l.into_int_value(), r.into_int_value(), "or")?
                .into(),

            (ir::ast::Operator::BitOr, false) => self
                .builder
                .build_or(l.into_int_value(), r.into_int_value(), "bitor")?
                .into(),

            (ir::ast::Operator::BitAnd, false) => self
                .builder
                .build_and(l.into_int_value(), r.into_int_value(), "bitand")?
                .into(),

            (
                ir::ast::Operator::ModAssign
                | ir::ast::Operator::DivAssign
                | ir::ast::Operator::MulAssign
                | ir::ast::Operator::SubAssign
                | ir::ast::Operator::Assign
                | ir::ast::Operator::AddAssign,
                _,
            ) => unreachable!("Assignment operator in LLVM IR"),

            _ => return Err(format!("Unsupported operator: {op:?}").into()),
        };

        Ok(out)
    }

    fn lower_unary(
        &self,
        op: ir::ast::UnaryOp,
        src: BasicValueEnum<'ctx>,
        ty: PrimitiveType,
    ) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
        let is_float = matches!(ty, PrimitiveType::F32 | PrimitiveType::F64);

        let out = match op {
            ir::ast::UnaryOp::Sub if is_float => self
                .builder
                .build_float_neg(src.into_float_value(), "fneg")?
                .into(),

            ir::ast::UnaryOp::Sub => self
                .builder
                .build_int_neg(src.into_int_value(), "ineg")?
                .into(),

            ir::ast::UnaryOp::Neg => todo!("Neg"),

            ir::ast::UnaryOp::Ref => unreachable!("Deprecated: ref"),

            ir::ast::UnaryOp::Deref => unreachable!("Deprecated: deref"),
        };

        Ok(out)
    }

    fn load_const(
        &self,
        value_map: &mut HashMap<ValueKey, ValueLocation<'ctx>>,
        src: &ConstValueKey,
        dst: ValueKey,
    ) -> Result<(), Box<dyn Error>> {
        let value = self.ctx.constants.data.get_unchecked(src);

        let value: BasicValueEnum<'ctx> = match value {
            ir::ast::ConstValue::Structure { fields, ty } => {
                todo!("Structure constants")
            }

            ir::ast::ConstValue::Number(number) => match number.value {
                ir::ast::NumberValue::Float(v) => self.llvm.f64_type().const_float(v).into(),

                ir::ast::NumberValue::Uint(v) => {
                    let bits = number.size.unwrap_or(32);

                    let ty = self
                        .llvm
                        .custom_width_int_type(bits.try_into().unwrap())
                        .unwrap();

                    ty.const_int(v as u64, false).into()
                }

                ir::ast::NumberValue::Int(v) => {
                    let bits = number.size.unwrap_or(32);

                    let ty = self
                        .llvm
                        .custom_width_int_type(bits.try_into().unwrap())
                        .unwrap();

                    ty.const_int(v as u64, true).into()
                }

                ir::ast::NumberValue::Any(v) => {
                    let bits = number.size.unwrap_or(32);

                    let ty = self
                        .llvm
                        .custom_width_int_type(bits.try_into().unwrap())
                        .unwrap();

                    ty.const_int(v as u64, false).into()
                }
            },

            ir::ast::ConstValue::String(s) => {
                let global = self.builder.build_global_string_ptr(s.as_str(), "str")?;

                global.as_pointer_value().into()
            }

            ir::ast::ConstValue::Char(c) => self.llvm.i32_type().const_int(*c as u64, false).into(),

            ir::ast::ConstValue::Bool(b) => self.llvm.i8_type().const_int(*b as u64, false).into(),

            ir::ast::ConstValue::EnumVariant { parent, variant } => {
                let enum_obj = self.ctx.types.enums.get_unchecked(match parent {
                    AnyTypeKey::Enum(e) => e,
                    _ => unreachable!("Expected enum type"),
                });

                let (_, value) = enum_obj
                    .variants
                    .iter()
                    .find(|(v, _)| v == variant)
                    .unwrap();

                return self.load_const(value_map, value, dst);
            }

            ir::ast::ConstValue::Array { elements, ty } => {
                todo!("Array constants")
            }

            ir::ast::ConstValue::Tuple { elements, ty } => {
                todo!("Tuple constants")
            }
        };

        value_map.insert(dst, ValueLocation::Value(value));

        Ok(())
    }

    pub fn emit_object(&self, path: impl AsRef<Path>) -> Result<(), Box<dyn Error>> {
        Target::initialize_native(&InitializationConfig::default())?;

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple)?;

        let machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .ok_or("failed to create LLVM target machine")?;

        self.module().set_triple(&triple);

        self.module()
            .set_data_layout(&machine.get_target_data().get_data_layout());

        let options = PassBuilderOptions::create();

        self.module.run_passes("default<O2>", &machine, options)?;

        machine.write_to_file(self.module(), FileType::Object, path.as_ref())?;

        Ok(())
    }
}
