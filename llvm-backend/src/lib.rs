use std::{collections::HashMap, error::Error, ops::Deref, path::Path};

use inkwell::basic_block::BasicBlock;
pub use inkwell::context::Context as LLVMContext;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::{
    builder::Builder,
    module::Module,
    types::{BasicType, BasicTypeEnum},
    values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue},
    AddressSpace, FloatPredicate, IntPredicate, OptimizationLevel,
};

use ir::{
    ast::{ConstValue, NumberValue},
    const_stage::{
        objects::{AnyObjectKey, InitState, IrCache},
        types::{AnyTypeKey, PrimitiveType, Vector},
        ConstValueKey, Context,
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

    pub fn lower(&mut self) {
        self.declaration_pass();

        for ir in self.ctx.ir_cache.iter_keys() {
            self.lower_function(ir);
        }

        if let Err(error) = self.module.verify() {
            panic!("LLVM module verification failed:\n{error}");
        }
    }

    pub fn module(&self) -> &Module<'ctx> {
        &self.module
    }

    fn declaration_pass(&mut self) {
        for (fun_key, fun) in self.ctx.ir_cache.iter_pairs() {
            let mut params = Vec::with_capacity(fun.parameters.len());

            for (_, var) in &fun.parameters {
                let ty = fun.variables.get_unchecked(var).ty;
                params.push(self.convert_type(ty).unwrap().into());
            }

            // `FunctionIr::new` stores `Some(Void)`, while some of the other
            // constructors represent a void return with `None`.
            let return_type = match fun.returns.map(|ty| ty.unwrap_full(&self.ctx.types)) {
                Some(AnyTypeKey::Void | AnyTypeKey::Never) | None => None,
                Some(ty) => Some(self.convert_type(ty).unwrap()),
            };

            let fn_type = match return_type {
                Some(ty) => ty.fn_type(&params, false),
                None => self.llvm.void_type().fn_type(&params, false),
            };

            let base_label = match fun.get_label(self.ctx) {
                Some(label) => label.to_string(),
                None => format!("fun_{}", fun_key.id()),
            };
            // Multiple IR instances can belong to one generic function. Keep
            // the source name for the first instance and mangle later ones.
            let mut label = base_label.clone();
            if self.module.get_function(&label).is_some() {
                label = format!("{base_label}__ir_{}", fun_key.id());
                let mut collision = 0_u32;
                while self.module.get_function(&label).is_some() {
                    collision += 1;
                    label = format!("{base_label}__ir_{}_{}", fun_key.id(), collision);
                }
            }

            let function = self.module.add_function(&label, fn_type, None);
            self.functions.insert(fun_key, function);
        }
    }

    fn lower_function(&mut self, fun_key: FunctionIrKey) {
        let function = self.functions[&fun_key];
        let fun = self.ctx.ir_cache.get_unchecked(&fun_key);

        let blocks: Vec<BasicBlock<'ctx>> = fun
            .blocks
            .arena()
            .iter()
            .map(|_| self.llvm.append_basic_block(function, "block"))
            .collect();

        let entry = *blocks
            .get(fun.blocks_entry.id())
            .expect("Function entry block is missing");

        let mut value_map: HashMap<ValueKey, ValueLocation<'ctx>> = HashMap::new();
        let mut block_parameters: Vec<Option<inkwell::values::PhiValue<'ctx>>> =
            vec![None; blocks.len()];

        // Materialize block parameters as PHI nodes before emitting ordinary
        // instructions. PHIs must be the first instructions in an LLVM block.
        for (block_key, block) in fun.blocks.arena().iter_pairs() {
            if let Some(parameter) = block.value.parameter {
                self.builder.position_at_end(blocks[block_key.id()]);
                let ty = self
                    .convert_type(fun.values.get_unchecked(&parameter).ty)
                    .unwrap();
                let phi = self.builder.build_phi(ty, "block_param").unwrap();
                value_map.insert(parameter, ValueLocation::Value(phi.as_basic_value()));
                block_parameters[block_key.id()] = Some(phi);
            }
        }

        // All local storage is allocated in the entry block. This also avoids
        // growing the stack when an AddressOfVal occurs inside a loop. PHIs,
        // if any, have already been placed before these allocas.
        self.builder.position_at_end(entry);
        let mut locals = Vec::with_capacity(fun.variables.len());
        for var in fun.variables.iter() {
            let ty = self.convert_type(var.ty).unwrap();
            let ptr = self.builder.build_alloca(ty, "local").unwrap();
            locals.push(Local { ptr, ty });
        }

        // Preallocate AddressOfVal temporaries too. Allocating them while
        // lowering later blocks would place allocas after the entry terminator.
        let mut address_temps = HashMap::new();
        for (_, block) in fun.blocks.arena().iter_pairs() {
            for instruction in block.value.instructions() {
                if let ir::ir::Instruction::AddressOfVal { val, dst } = instruction.deref() {
                    let ty = self.convert_type(fun.values.get_unchecked(val).ty).unwrap();
                    let name = format!("address_temp_{}", dst.id());
                    let ptr = self.builder.build_alloca(ty, &name).unwrap();
                    address_temps.insert(*dst, ptr);
                }
            }
        }

        // Store function parameters in their corresponding locals.
        for (param, (_, var)) in function.get_params().iter().zip(&fun.parameters) {
            let local = &locals[var.id()];
            self.builder.build_store(local.ptr, *param).unwrap();
        }

        for (block_idx, block) in fun.blocks.arena().iter_pairs().map(|(k, s)| (k, &s.value)) {
            self.builder.position_at_end(blocks[block_idx.id()]);

            for instr in block.instructions() {
                match instr.deref() {
                    ir::ir::Instruction::LoadConst { src, dst } => {
                        let ty = fun.values.get_unchecked(dst).ty;
                        self.load_const(&mut value_map, src, *dst, ty).unwrap();
                    }
                    ir::ir::Instruction::BinOp { op, l, r, dst, ty } => {
                        let l = self.load_value(&value_map[l]).unwrap();
                        let r = self.load_value(&value_map[r]).unwrap();
                        let out = self.lower_binop(*op, l, r, *ty).unwrap();
                        value_map.insert(*dst, ValueLocation::Value(out));
                    }
                    ir::ir::Instruction::UnaryOp { op, src, dst, ty } => {
                        let src = self.load_value(&value_map[src]).unwrap();
                        let out = self.lower_unary(*op, src, *ty).unwrap();
                        value_map.insert(*dst, ValueLocation::Value(out));
                    }
                    ir::ir::Instruction::StoreVar { dst, src } => {
                        let value = self.load_value(&value_map[src]).unwrap();
                        let local = &locals[dst.id()];
                        self.builder.build_store(local.ptr, value).unwrap();
                    }
                    ir::ir::Instruction::LoadVar { src, dst } => {
                        let local = &locals[src.id()];
                        let value = self
                            .builder
                            .build_load(local.ty, local.ptr, "load")
                            .unwrap();
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
                            .collect::<Result<Vec<_>, _>>()
                            .unwrap();

                        let call = self.builder.build_call(callee, &args, "call").unwrap();
                        if let Some(value) = call.try_as_basic_value().basic() {
                            value_map.insert(*result, ValueLocation::Value(value));
                        }
                    }
                    ir::ir::Instruction::AddressOfObj { obj, dst } => {
                        let object_ty = fun.values.get_unchecked(dst).ty;
                        let target_ir = self.resolve_function_object(*obj, object_ty).unwrap();
                        let target = self.functions[&target_ir];
                        let ptr = target.as_global_value().as_pointer_value();
                        value_map.insert(*dst, ValueLocation::Pointer(ptr));
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
                        let value = self.load_value(&value_map[val]).unwrap();
                        let ptr = address_temps[dst];
                        self.builder.build_store(ptr, value).unwrap();
                        value_map.insert(*dst, ValueLocation::Pointer(ptr));
                    }
                    ir::ir::Instruction::Deref { src, dst } => {
                        let ptr = self.load_pointer(&value_map[src]).unwrap();
                        let dst_ty = fun.values.get_unchecked(dst).ty;
                        let ident = dst_ty.stringify(&self.ctx.types);
                        let ty = self.convert_type(dst_ty).unwrap();

                        let value = self
                            .builder
                            .build_load(ty, ptr, &format!("deref: {ident}"))
                            .unwrap();
                        value_map.insert(*dst, ValueLocation::Value(value));
                    }
                }
            }

            match block.terminator().as_ref() {
                Some(ir::ir::Terminator::Return(key)) => match key {
                    Some(key) => {
                        let value = self.load_value(&value_map[key]).unwrap();
                        self.builder.build_return(Some(&value)).unwrap();
                    }
                    None => {
                        self.builder.build_return(None).unwrap();
                    }
                },
                Some(ir::ir::Terminator::Jump(target, argument)) => {
                    let target_parameter = fun.blocks.arena().get_unchecked(target).value.parameter;

                    match (target_parameter, argument) {
                        (Some(_), Some(argument)) => {
                            let incoming = self.load_value(&value_map[argument]).unwrap();
                            let phi = block_parameters[target.id()]
                                .expect("target block parameter has no LLVM PHI");
                            phi.add_incoming(&[(
                                &incoming as &dyn inkwell::values::BasicValue<'ctx>,
                                blocks[block_idx.id()],
                            )]);
                        }
                        (Some(_), None) => panic!(
                            "Jump to block {:?} is missing its block-parameter value",
                            target
                        ),
                        (None, Some(_)) => panic!(
                            "Jump to block {:?} supplies a value but the block has no parameter",
                            target
                        ),
                        (None, None) => {}
                    }

                    self.builder
                        .build_unconditional_branch(blocks[target.id()])
                        .unwrap();
                }
                Some(ir::ir::Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                }) => {
                    if fun
                        .blocks
                        .arena()
                        .get_unchecked(then_block)
                        .value
                        .parameter
                        .is_some()
                        || fun
                            .blocks
                            .arena()
                            .get_unchecked(else_block)
                            .value
                            .parameter
                            .is_some()
                    {
                        panic!(
                            "Branch cannot target a block with a parameter; use Jump with an argument"
                        );
                    }

                    let condition = self
                        .load_value(&value_map[condition])
                        .unwrap()
                        .into_int_value();
                    let condition = self
                        .builder
                        .build_int_compare(
                            IntPredicate::NE,
                            condition,
                            condition.get_type().const_zero(),
                            "branch_condition",
                        )
                        .unwrap();

                    self.builder
                        .build_conditional_branch(
                            condition,
                            blocks[then_block.id()],
                            blocks[else_block.id()],
                        )
                        .unwrap();
                }
                Some(ir::ir::Terminator::Unreachable) => self.emit_trap(None),
                Some(ir::ir::Terminator::Exit(code)) => self.emit_trap(Some(*code)),
                None => {
                    // LLVM requires every basic block to end in a terminator.
                    // A block without an IR terminator is dead/unreachable in
                    // this IR; do not accidentally introduce fallthrough.
                    self.builder.build_unreachable().unwrap();
                }
            }
        }
    }

    fn resolve_function_object(
        &self,
        object: AnyObjectKey,
        value_ty: AnyTypeKey,
    ) -> Result<FunctionIrKey, Box<dyn Error>> {
        let AnyObjectKey::Function(object_key) = object else {
            return Err(format!("AddressOfObj only supports functions, got {object:?}").into());
        };

        let function = &self.ctx.objects.functions.get_unchecked(&object_key).data;
        match &function.ir {
            IrCache::Single(state) => match state {
                InitState::Done(key) | InitState::Progress(key) => Ok(*key),
                InitState::Uninitialized => {
                    Err(format!("Function object {object_key:?} has not been lowered").into())
                }
            },
            IrCache::Polymorphic(cache) => {
                let value_ty = value_ty.unwrap_full(&self.ctx.types);
                match cache.get(&value_ty) {
                    Some(InitState::Done(key) | InitState::Progress(key)) => Ok(*key),
                    Some(InitState::Uninitialized) | None => Err(format!(
                        "No lowered specialization of function object {object_key:?} matches {}",
                        value_ty.stringify(&self.ctx.types)
                    )
                    .into()),
                }
            }
        }
    }

    fn emit_trap(&self, code: Option<u8>) {
        match code {
            Some(code) => {
                // Unlike llvm.trap, llvm.ubsantrap accepts an immediate i8
                // payload, so the IR's Exit(code) value survives lowering.
                let trap_type = self
                    .llvm
                    .void_type()
                    .fn_type(&[self.llvm.i8_type().into()], false);
                let trap = self
                    .module
                    .get_function("llvm.ubsantrap")
                    .unwrap_or_else(|| self.module.add_function("llvm.ubsantrap", trap_type, None));
                let argument = self.llvm.i8_type().const_int(code.into(), false);
                self.builder
                    .build_call(trap, &[argument.into()], "trap")
                    .unwrap();
            }
            None => {
                let trap = self.module.get_function("llvm.trap").unwrap_or_else(|| {
                    let trap_type = self.llvm.void_type().fn_type(&[], false);
                    self.module.add_function("llvm.trap", trap_type, None)
                });
                self.builder.build_call(trap, &[], "trap").unwrap();
            }
        }
        self.builder.build_unreachable().unwrap();
    }

    fn convert_primitive_type(
        &self,
        ty: PrimitiveType,
    ) -> Result<BasicTypeEnum<'ctx>, Box<dyn Error>> {
        let ty = match ty {
            PrimitiveType::I8 | PrimitiveType::U8 => self.llvm.i8_type().into(),
            PrimitiveType::I16 | PrimitiveType::U16 => self.llvm.i16_type().into(),
            PrimitiveType::I32 | PrimitiveType::U32 => self.llvm.i32_type().into(),
            PrimitiveType::I64 | PrimitiveType::U64 => self.llvm.i64_type().into(),
            PrimitiveType::I128 | PrimitiveType::U128 => self.llvm.i128_type().into(),
            PrimitiveType::F32 => self.llvm.f32_type().into(),
            PrimitiveType::F64 => self.llvm.f64_type().into(),
            PrimitiveType::Char => self.llvm.i32_type().into(),
            // Keep language bool byte-sized. LLVM comparisons still produce
            // i1, so lower_binop widens their results before exposing them.
            PrimitiveType::Bool => self.llvm.i8_type().into(),
            PrimitiveType::EntityRef => {
                return Err("EntityRef has no defined LLVM representation yet".into());
            }
        };
        Ok(ty)
    }

    fn convert_type(&self, ty: AnyTypeKey) -> Result<BasicTypeEnum<'ctx>, Box<dyn Error>> {
        let ty = ty.unwrap_full(&self.ctx.types);
        let ty = match ty {
            AnyTypeKey::Primitive(primitive) => self.convert_primitive_type(primitive)?,
            AnyTypeKey::Reference(_) | AnyTypeKey::Function(_) => {
                self.llvm.ptr_type(AddressSpace::default()).into()
            }
            AnyTypeKey::Array(key) => {
                let array = self.ctx.types.arrays.get_unchecked(&key);
                let size = array
                    .size
                    .ok_or_else(|| "Unsized arrays do not have a by-value LLVM type".to_owned())?;
                let size = u32::try_from(size)
                    .map_err(|_| "Array length exceeds LLVM's u32 limit".to_owned())?;
                self.convert_type(array.element_type)?
                    .array_type(size)
                    .into()
            }
            AnyTypeKey::Tuple(key) => {
                let tuple = self.ctx.types.tuples.get_unchecked(&key);
                let fields = tuple
                    .parameters
                    .iter()
                    .map(|ty| self.convert_type(*ty))
                    .collect::<Result<Vec<_>, _>>()?;
                self.llvm.struct_type(&fields, false).into()
            }
            AnyTypeKey::Struct(key) => {
                let structure = self.ctx.types.structures.get_unchecked(&key);
                let fields = structure
                    .parameters
                    .iter()
                    .map(|(_, ty, _)| self.convert_type(*ty))
                    .collect::<Result<Vec<_>, _>>()?;
                self.llvm.struct_type(&fields, false).into()
            }
            AnyTypeKey::Enum(key) => {
                let repr = self.ctx.types.enums.get_unchecked(&key).repr;
                self.convert_type(repr)?
            }
            AnyTypeKey::Vector(Vector { element, lanes }) => {
                match self.convert_primitive_type(element)? {
                    BasicTypeEnum::FloatType(float_type) => {
                        float_type.vec_type(lanes as u32).into()
                    }
                    BasicTypeEnum::IntType(int_type) => int_type.vec_type(lanes as u32).into(),
                    _ => unreachable!("unsupported vector element type: {element:?}"),
                }
            }
            AnyTypeKey::Void | AnyTypeKey::Never => {
                return Err("Void/Never cannot be used as a basic type".into());
            }
            _ => {
                return Err(
                    format!("Unsupported type: '{}'", ty.stringify(&self.ctx.types)).into(),
                );
            }
        };
        Ok(ty)
    }

    fn load_value(
        &self,
        value: &ValueLocation<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
        match value {
            ValueLocation::Value(value) => Ok(*value),
            ValueLocation::Pointer(value) => Ok((*value).into()),
        }
    }

    fn load_pointer(
        &self,
        value: &ValueLocation<'ctx>,
    ) -> Result<PointerValue<'ctx>, Box<dyn Error>> {
        match value {
            ValueLocation::Pointer(ptr) => Ok(*ptr),
            ValueLocation::Value(value) if value.is_pointer_value() => {
                Ok(value.into_pointer_value())
            }
            ValueLocation::Value(value) => Err(format!(
                "Expected a pointer value, got LLVM type {}",
                value.get_type()
            )
            .into()),
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
        let is_unsigned = ty.uint_size().is_some();

        if is_float {
            let l = l.into_float_value();
            let r = r.into_float_value();
            let out = match op {
                ir::ast::Operator::Add => self.builder.build_float_add(l, r, "fadd")?.into(),
                ir::ast::Operator::Sub => self.builder.build_float_sub(l, r, "fsub")?.into(),
                ir::ast::Operator::Mul => self.builder.build_float_mul(l, r, "fmul")?.into(),
                ir::ast::Operator::Div => self.builder.build_float_div(l, r, "fdiv")?.into(),
                ir::ast::Operator::Mod => self.builder.build_float_rem(l, r, "frem")?.into(),
                ir::ast::Operator::Eq => self
                    .builder
                    .build_float_compare(FloatPredicate::OEQ, l, r, "feq")?
                    .into(),
                ir::ast::Operator::NEq => self
                    .builder
                    .build_float_compare(FloatPredicate::ONE, l, r, "fneq")?
                    .into(),
                ir::ast::Operator::Gr => self
                    .builder
                    .build_float_compare(FloatPredicate::OGT, l, r, "fgt")?
                    .into(),
                ir::ast::Operator::Le => self
                    .builder
                    .build_float_compare(FloatPredicate::OLT, l, r, "flt")?
                    .into(),
                ir::ast::Operator::GrEq => self
                    .builder
                    .build_float_compare(FloatPredicate::OGE, l, r, "fgte")?
                    .into(),
                ir::ast::Operator::LeEq => self
                    .builder
                    .build_float_compare(FloatPredicate::OLE, l, r, "flte")?
                    .into(),
                _ => return Err(format!("Unsupported floating-point operator: {op:?}").into()),
            };
            return if Self::is_comparison(op) {
                self.widen_boolean(out)
            } else {
                Ok(out)
            };
        }

        let l = l.into_int_value();
        let r = r.into_int_value();
        let out = match op {
            ir::ast::Operator::Add => self.builder.build_int_add(l, r, "add")?.into(),
            ir::ast::Operator::Sub => self.builder.build_int_sub(l, r, "sub")?.into(),
            ir::ast::Operator::Mul => self.builder.build_int_mul(l, r, "mul")?.into(),
            ir::ast::Operator::Div if is_unsigned => {
                self.builder.build_int_unsigned_div(l, r, "udiv")?.into()
            }
            ir::ast::Operator::Div => self.builder.build_int_signed_div(l, r, "sdiv")?.into(),
            ir::ast::Operator::Mod if is_unsigned => {
                self.builder.build_int_unsigned_rem(l, r, "urem")?.into()
            }
            ir::ast::Operator::Mod => self.builder.build_int_signed_rem(l, r, "srem")?.into(),
            ir::ast::Operator::Eq => self
                .builder
                .build_int_compare(IntPredicate::EQ, l, r, "eq")?
                .into(),
            ir::ast::Operator::NEq => self
                .builder
                .build_int_compare(IntPredicate::NE, l, r, "neq")?
                .into(),
            ir::ast::Operator::Gr if is_unsigned => self
                .builder
                .build_int_compare(IntPredicate::UGT, l, r, "ugt")?
                .into(),
            ir::ast::Operator::Gr => self
                .builder
                .build_int_compare(IntPredicate::SGT, l, r, "sgt")?
                .into(),
            ir::ast::Operator::Le if is_unsigned => self
                .builder
                .build_int_compare(IntPredicate::ULT, l, r, "ult")?
                .into(),
            ir::ast::Operator::Le => self
                .builder
                .build_int_compare(IntPredicate::SLT, l, r, "slt")?
                .into(),
            ir::ast::Operator::GrEq if is_unsigned => self
                .builder
                .build_int_compare(IntPredicate::UGE, l, r, "uge")?
                .into(),
            ir::ast::Operator::GrEq => self
                .builder
                .build_int_compare(IntPredicate::SGE, l, r, "sge")?
                .into(),
            ir::ast::Operator::LeEq if is_unsigned => self
                .builder
                .build_int_compare(IntPredicate::ULE, l, r, "ule")?
                .into(),
            ir::ast::Operator::LeEq => self
                .builderla
                .build_int_compare(IntPredicate::SLE, l, r, "sle")?
                .into(),
            ir::ast::Operator::And | ir::ast::Operator::BitAnd => {
                self.builder.build_and(l, r, "and")?.into()
            }
            ir::ast::Operator::Or | ir::ast::Operator::BitOr => {
                self.builder.build_or(l, r, "or")?.into()
            }
            ir::ast::Operator::ModAssign
            | ir::ast::Operator::DivAssign
            | ir::ast::Operator::MulAssign
            | ir::ast::Operator::SubAssign
            | ir::ast::Operator::Assign
            | ir::ast::Operator::AddAssign => {
                return Err(format!("Assignment operator reached LLVM lowering: {op:?}").into());
            }
        };

        if Self::is_comparison(op) {
            self.widen_boolean(out)
        } else {
            Ok(out)
        }
    }

    fn is_comparison(op: ir::ast::Operator) -> bool {
        matches!(
            op,
            ir::ast::Operator::Eq
                | ir::ast::Operator::NEq
                | ir::ast::Operator::Gr
                | ir::ast::Operator::Le
                | ir::ast::Operator::GrEq
                | ir::ast::Operator::LeEq
        )
    }

    fn widen_boolean(
        &self,
        value: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
        Ok(self
            .builder
            .build_int_z_extend(value.into_int_value(), self.llvm.i8_type(), "bool")?
            .into())
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
            ir::ast::UnaryOp::Neg if ty == PrimitiveType::Bool => {
                let value = src.into_int_value();
                let result = self.builder.build_int_compare(
                    IntPredicate::EQ,
                    value,
                    value.get_type().const_zero(),
                    "logical_not",
                )?;
                self.builder
                    .build_int_z_extend(result, self.llvm.i8_type(), "logical_not_bool")?
                    .into()
            }
            ir::ast::UnaryOp::Neg => {
                return Err(format!("Logical negation is not defined for {ty:?}").into());
            }
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
        expected_ty: AnyTypeKey,
    ) -> Result<(), Box<dyn Error>> {
        let value = self.ctx.constants.data.get_unchecked(src);
        let value = self.lower_const_value(value, expected_ty)?;
        value_map.insert(dst, ValueLocation::Value(value));
        Ok(())
    }

    fn lower_const_value(
        &self,
        value: &ConstValue,
        expected_ty: AnyTypeKey,
    ) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
        let expected_ty = expected_ty.unwrap_full(&self.ctx.types);

        match value {
            ConstValue::Number(number) => {
                let AnyTypeKey::Primitive(primitive) = expected_ty else {
                    return Err(format!(
                        "Numeric constant cannot initialize {}",
                        expected_ty.stringify(&self.ctx.types)
                    )
                    .into());
                };

                match (&number.value, primitive) {
                    (NumberValue::Float(value), PrimitiveType::F32) => {
                        Ok(self.llvm.f32_type().const_float(*value).into())
                    }
                    (NumberValue::Float(value), PrimitiveType::F64) => {
                        Ok(self.llvm.f64_type().const_float(*value).into())
                    }
                    (NumberValue::Int(value), ty) if ty.int_size().is_some() => {
                        self.const_integer(ty, *value as u128)
                    }
                    (NumberValue::Uint(value), ty) if ty.uint_size().is_some() => {
                        self.const_integer(ty, *value)
                    }
                    (NumberValue::Any(value), ty)
                        if ty.int_size().is_some() || ty.uint_size().is_some() =>
                    {
                        self.const_integer(ty, *value)
                    }
                    (number, ty) => Err(format!(
                        "Numeric constant {number:?} is incompatible with {ty:?}"
                    )
                    .into()),
                }
            }
            ConstValue::String(value) => {
                let global = self
                    .builder
                    .build_global_string_ptr(value.as_str(), "str")?;
                Ok(global.as_pointer_value().into())
            }
            ConstValue::Char(value) => match expected_ty {
                AnyTypeKey::Primitive(PrimitiveType::Char)
                | AnyTypeKey::Primitive(PrimitiveType::I32)
                | AnyTypeKey::Primitive(PrimitiveType::U32) => Ok(self
                    .llvm
                    .i32_type()
                    .const_int(*value as u32 as u64, false)
                    .into()),
                _ => Err(format!(
                    "Character constant is incompatible with {}",
                    expected_ty.stringify(&self.ctx.types)
                )
                .into()),
            },
            ConstValue::Bool(value) => match expected_ty {
                AnyTypeKey::Primitive(PrimitiveType::Bool) => Ok(self
                    .llvm
                    .i8_type()
                    .const_int(u64::from(*value), false)
                    .into()),
                _ => Err(format!(
                    "Boolean constant is incompatible with {}",
                    expected_ty.stringify(&self.ctx.types)
                )
                .into()),
            },
            ConstValue::EnumVariant { parent, variant } => {
                let AnyTypeKey::Enum(enum_key) = expected_ty else {
                    return Err(format!(
                        "Enum variant {variant} cannot initialize {}",
                        expected_ty.stringify(&self.ctx.types)
                    )
                    .into());
                };
                if parent.unwrap_full(&self.ctx.types) != AnyTypeKey::Enum(enum_key) {
                    return Err(
                        format!("Enum variant {variant} has a mismatched parent type").into(),
                    );
                }

                let enum_type = self.ctx.types.enums.get_unchecked(&enum_key);
                let (_, value_key) = enum_type
                    .variants
                    .iter()
                    .find(|(name, _)| name == variant)
                    .ok_or_else(|| format!("Unknown enum variant: {variant}"))?;
                let value = self.ctx.constants.data.get_unchecked(value_key);
                self.lower_const_value(value, enum_type.repr)
            }
            ConstValue::Array { elements, .. } => {
                let AnyTypeKey::Array(array_key) = expected_ty else {
                    return Err(format!(
                        "Array constant cannot initialize {}",
                        expected_ty.stringify(&self.ctx.types)
                    )
                    .into());
                };
                let array = self.ctx.types.arrays.get_unchecked(&array_key);
                let size = array
                    .size
                    .ok_or_else(|| "Cannot lower an unsized array constant".to_owned())?;
                if elements.len() != size {
                    return Err(format!(
                        "Array constant has {} elements, expected {size}",
                        elements.len()
                    )
                    .into());
                }

                let values = elements
                    .iter()
                    .map(|element| self.lower_const_value(element.deref(), array.element_type))
                    .collect::<Result<Vec<_>, _>>()?;
                let array_type = self.convert_type(expected_ty)?.into_array_type();
                self.const_array(array_type.get_element_type(), &values)
            }
            ConstValue::Tuple { elements, .. } => {
                let AnyTypeKey::Tuple(tuple_key) = expected_ty else {
                    return Err(format!(
                        "Tuple constant cannot initialize {}",
                        expected_ty.stringify(&self.ctx.types)
                    )
                    .into());
                };
                let tuple = self.ctx.types.tuples.get_unchecked(&tuple_key);
                if elements.len() != tuple.parameters.len() {
                    return Err(format!(
                        "Tuple constant has {} elements, expected {}",
                        elements.len(),
                        tuple.parameters.len()
                    )
                    .into());
                }

                let values = elements
                    .iter()
                    .zip(&tuple.parameters)
                    .map(|(element, ty)| self.lower_const_value(element.deref(), *ty))
                    .collect::<Result<Vec<_>, _>>()?;
                let tuple_type = self.convert_type(expected_ty)?.into_struct_type();
                Ok(tuple_type.const_named_struct(&values).into())
            }
            ConstValue::Structure { fields, .. } => {
                let AnyTypeKey::Struct(struct_key) = expected_ty else {
                    return Err(format!(
                        "Structure constant cannot initialize {}",
                        expected_ty.stringify(&self.ctx.types)
                    )
                    .into());
                };
                let structure = self.ctx.types.structures.get_unchecked(&struct_key);

                let mut provided = HashMap::with_capacity(fields.len());
                for field in fields {
                    let (name, value) = field.deref();
                    if provided.insert(name.as_str(), value.deref()).is_some() {
                        return Err(format!("Duplicate structure field: {}", name.as_str()).into());
                    }
                }

                for name in provided.keys() {
                    if !structure
                        .parameters
                        .iter()
                        .any(|(field_name, _, _)| field_name.as_str() == *name)
                    {
                        return Err(format!("Unknown structure field: {name}").into());
                    }
                }

                let mut values = Vec::with_capacity(structure.parameters.len());
                for (name, field_ty, default) in &structure.parameters {
                    let field_value = provided
                        .get(name.as_str())
                        .copied()
                        .or(default.as_ref())
                        .ok_or_else(|| format!("Missing structure field: {name}"))?;
                    values.push(self.lower_const_value(field_value, *field_ty)?);
                }

                let structure_type = self.convert_type(expected_ty)?.into_struct_type();
                Ok(structure_type.const_named_struct(&values).into())
            }
        }
    }

    fn const_integer(
        &self,
        primitive: PrimitiveType,
        value: u128,
    ) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
        let ty = self.convert_primitive_type(primitive)?.into_int_type();
        let words = if ty.get_bit_width() > 64 {
            vec![value as u64, (value >> 64) as u64]
        } else {
            vec![value as u64]
        };
        Ok(ty.const_int_arbitrary_precision(&words).into())
    }

    fn const_array(
        &self,
        element_type: BasicTypeEnum<'ctx>,
        values: &[BasicValueEnum<'ctx>],
    ) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
        let value = match element_type {
            BasicTypeEnum::IntType(ty) => {
                let values = values
                    .iter()
                    .map(|value| value.into_int_value())
                    .collect::<Vec<_>>();
                ty.const_array(&values).into()
            }
            BasicTypeEnum::FloatType(ty) => {
                let values = values
                    .iter()
                    .map(|value| value.into_float_value())
                    .collect::<Vec<_>>();
                ty.const_array(&values).into()
            }
            BasicTypeEnum::PointerType(ty) => {
                let values = values
                    .iter()
                    .map(|value| value.into_pointer_value())
                    .collect::<Vec<_>>();
                ty.const_array(&values).into()
            }
            BasicTypeEnum::ArrayType(ty) => {
                let values = values
                    .iter()
                    .map(|value| value.into_array_value())
                    .collect::<Vec<_>>();
                ty.const_array(&values).into()
            }
            BasicTypeEnum::StructType(ty) => {
                let values = values
                    .iter()
                    .map(|value| value.into_struct_value())
                    .collect::<Vec<_>>();
                ty.const_array(&values).into()
            }
            BasicTypeEnum::VectorType(ty) => {
                let values = values
                    .iter()
                    .map(|value| value.into_vector_value())
                    .collect::<Vec<_>>();
                ty.const_array(&values).into()
            }
            BasicTypeEnum::ScalableVectorType(_) => {
                return Err("Arrays of scalable vectors are not supported".into());
            }
        };
        Ok(value)
    }

    pub fn emit_object(&self, path: impl AsRef<Path>) {
        Target::initialize_native(&InitializationConfig::default()).unwrap();

        let triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&triple).unwrap();

        let machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .ok_or("failed to create LLVM target machine")
            .unwrap();

        self.module().set_triple(&triple);
        self.module()
            .set_data_layout(&machine.get_target_data().get_data_layout());

        let options = PassBuilderOptions::create();
        self.module
            .run_passes("default<O3>", &machine, options)
            .unwrap();

        machine
            .write_to_file(self.module(), FileType::Object, path.as_ref())
            .unwrap();
    }
}
