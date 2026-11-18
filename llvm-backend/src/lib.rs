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
    values::{ArrayValue, BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue},
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

    pub fn lower(&mut self) {
        self.declaration_pass();

        for ir in self.ctx.ir_cache.iter_keys() {
            self.lower_function(ir);
        }

        self.module.verify().unwrap();
    }

    pub fn module(&self) -> &Module<'ctx> {
        &self.module
    }

    fn declaration_pass(&mut self) {
        for (fun_key, fun) in self.ctx.ir_cache.iter_pairs() {
            let mut params = Vec::new();

            for (_, var) in &fun.parameters {
                let ty = fun.variables.get_unchecked(var).ty;
                params.push(self.convert_type(ty).unwrap().into());
            }

            let return_type = match fun.returns {
                Some(ty) => Some(self.convert_type(ty).unwrap()),
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

        let mut locals = Vec::new();

        // Allocate locals in the entry block.
        let entry = *blocks.first().ok_or("Function has no blocks").unwrap();
        self.builder.position_at_end(entry);

        for var in fun.variables.iter() {
            let ty = self.convert_type(var.ty).unwrap();
            let ptr = self.builder.build_alloca(ty, "local").unwrap();

            locals.push(Local { ptr, ty });
        }

        // Store function parameters in their corresponding locals.
        for (param, (_, var)) in function.get_params().iter().zip(&fun.parameters) {
            let local = &locals[var.id()];
            self.builder.build_store(local.ptr, *param).unwrap();
        }

        let mut value_map: HashMap<ValueKey, ValueLocation<'ctx>> = HashMap::new();

        for (block_idx, block) in fun.blocks.arena().iter_pairs().map(|(k, s)| (k, &s.value)) {
            self.builder.position_at_end(blocks[block_idx.id()]);

            for instr in block.instructions() {
                match instr.deref() {
                    ir::ir::Instruction::LoadConst { src, dst } => {
                        let dst_ty = fun.values.get_unchecked(dst).ty;
                        self.load_const(&mut value_map, src, *dst, dst_ty).unwrap();
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
                        let value = self.load_value(&value_map[val]).unwrap();

                        let ty = fun.values.get_unchecked(val).ty;
                        let ident = ty.stringify(&self.ctx.types);
                        let ty = self.convert_type(ty).unwrap();

                        let ptr = self
                            .builder
                            .build_alloca(ty, &format!("temp: {ident}"))
                            .unwrap();
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
                    ir::ir::Instruction::GetField { src, idx, dst } => {
                        let aggregate = self.load_value(&value_map[src]).unwrap();
                        let value = self
                            .builder
                            .build_extract_value(
                                aggregate.into_struct_value(),
                                *idx as u32,
                                "field",
                            )
                            .unwrap();
                        value_map.insert(*dst, ValueLocation::Value(value));
                    }
                    ir::ir::Instruction::GetIndex { src, index, dst } => {
                        let aggregate = self.load_value(&value_map[src]).unwrap();
                        let index = self.load_value(&value_map[index]).unwrap().into_int_value();
                        let source_ty = fun
                            .values
                            .get_unchecked(src)
                            .ty
                            .unwrap_full(&self.ctx.types);
                        let value = match source_ty {
                            AnyTypeKey::Vector(_) => self
                                .builder
                                .build_extract_element(
                                    aggregate.into_vector_value(),
                                    index,
                                    "index",
                                )
                                .unwrap(),
                            AnyTypeKey::Array(_) => {
                                let array_ty =
                                    self.convert_type(source_ty).unwrap().into_array_type();
                                let ptr = self
                                    .builder
                                    .build_alloca(array_ty, "indexable.array")
                                    .unwrap();
                                self.builder.build_store(ptr, aggregate).unwrap();
                                let zero = self.llvm.i32_type().const_zero();
                                let element_ptr = unsafe {
                                    self.builder.build_gep(
                                        array_ty,
                                        ptr,
                                        &[zero, index],
                                        "array.index",
                                    )
                                }
                                .unwrap();
                                let element_ty =
                                    self.convert_type(fun.values.get_unchecked(dst).ty).unwrap();
                                self.builder
                                    .build_load(element_ty, element_ptr, "index.load")
                                    .unwrap()
                            }
                            _ => unreachable!("IR GetIndex source must be an array or vector"),
                        };
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
                Some(ir::ir::Terminator::Jump(key, _)) => {
                    self.builder
                        .build_unconditional_branch(blocks[key.id()])
                        .unwrap();
                }
                Some(ir::ir::Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                }) => {
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
                            "branch.condition",
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
                Some(ir::ir::Terminator::Unreachable) => {
                    self.builder.build_unreachable().unwrap();
                }
                Some(ir::ir::Terminator::Exit(_key)) => {
                    todo!("Exit terminator");
                }
                None => {}
            }
        }
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
            PrimitiveType::Bool => self.llvm.i8_type().into(),
            _ => return Err(format!("Unsupported type: {ty:?}").into()),
        };
        Ok(ty)
    }

    fn convert_type(&self, ty: AnyTypeKey) -> Result<BasicTypeEnum<'ctx>, Box<dyn Error>> {
        let ty = ty.unwrap_full(&self.ctx.types);

        let ty = match ty {
            AnyTypeKey::Primitive(primitive) => self.convert_primitive_type(primitive)?,
            AnyTypeKey::Reference(_) => self.llvm.ptr_type(AddressSpace::default()).into(),
            AnyTypeKey::Array(array) => {
                let array = self.ctx.types.arrays.get_unchecked(&array);
                let size = array
                    .size
                    .ok_or("Unsized arrays cannot be represented as LLVM value types")?;
                self.convert_type(array.element_type)?
                    .array_type(size as u32)
                    .into()
            }
            AnyTypeKey::Tuple(tuple) => {
                let tuple = self.ctx.types.tuples.get_unchecked(&tuple);
                let fields = tuple
                    .parameters
                    .iter()
                    .map(|ty| self.convert_type(*ty))
                    .collect::<Result<Vec<_>, _>>()?;
                self.llvm.struct_type(&fields, false).into()
            }
            AnyTypeKey::Struct(structure) => {
                let structure = self.ctx.types.structures.get_unchecked(&structure);
                let fields = structure
                    .parameters
                    .iter()
                    .map(|(_, ty, _)| self.convert_type(*ty))
                    .collect::<Result<Vec<_>, _>>()?;
                self.llvm.struct_type(&fields, false).into()
            }
            AnyTypeKey::Enum(enumeration) => {
                let enumeration = self.ctx.types.enums.get_unchecked(&enumeration);
                self.convert_type(enumeration.repr)?
            }
            AnyTypeKey::Vector(Vector { element, lanes }) => {
                match self.convert_primitive_type(element)? {
                    BasicTypeEnum::FloatType(float_type) => float_type.vec_type(lanes as _).into(),
                    BasicTypeEnum::IntType(int_type) => int_type.vec_type(lanes as _).into(),
                    _ => unreachable!(
                        "unsupported vector type: '{}'",
                        ty.stringify(&self.ctx.types)
                    ),
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
        let is_unsigned = matches!(
            ty,
            PrimitiveType::U8
                | PrimitiveType::U16
                | PrimitiveType::U32
                | PrimitiveType::U64
                | PrimitiveType::U128
        );

        // LLVM comparisons produce i1. Neruda's bool is represented as i8,
        // so widen comparison results before they enter the value map.
        let comparison = match (op, is_float) {
            (ir::ast::Operator::Eq, false) => Some(IntPredicate::EQ),
            (ir::ast::Operator::NEq, false) => Some(IntPredicate::NE),
            (ir::ast::Operator::Gr, false) => Some(if is_unsigned {
                IntPredicate::UGT
            } else {
                IntPredicate::SGT
            }),
            (ir::ast::Operator::Le, false) => Some(if is_unsigned {
                IntPredicate::ULT
            } else {
                IntPredicate::SLT
            }),
            (ir::ast::Operator::GrEq, false) => Some(if is_unsigned {
                IntPredicate::UGE
            } else {
                IntPredicate::SGE
            }),
            (ir::ast::Operator::LeEq, false) => Some(if is_unsigned {
                IntPredicate::ULE
            } else {
                IntPredicate::SLE
            }),
            _ => None,
        };

        if let Some(predicate) = comparison {
            let result = self.builder.build_int_compare(
                predicate,
                l.into_int_value(),
                r.into_int_value(),
                "icmp",
            )?;
            return Ok(self
                .builder
                .build_int_z_extend(result, self.llvm.i8_type(), "bool.zext")?
                .into());
        }

        if is_float {
            let l = l.into_float_value();
            let r = r.into_float_value();
            let value = match op {
                ir::ast::Operator::Add => self.builder.build_float_add(l, r, "fadd")?.into(),
                ir::ast::Operator::Sub => self.builder.build_float_sub(l, r, "fsub")?.into(),
                ir::ast::Operator::Mul => self.builder.build_float_mul(l, r, "fmul")?.into(),
                ir::ast::Operator::Div => self.builder.build_float_div(l, r, "fdiv")?.into(),
                ir::ast::Operator::Mod => self.builder.build_float_rem(l, r, "frem")?.into(),
                ir::ast::Operator::Eq => self
                    .builder
                    .build_float_compare(FloatPredicate::OEQ, l, r, "fcmp")?
                    .into(),
                ir::ast::Operator::NEq => self
                    .builder
                    .build_float_compare(FloatPredicate::ONE, l, r, "fcmp")?
                    .into(),
                ir::ast::Operator::Gr => self
                    .builder
                    .build_float_compare(FloatPredicate::OGT, l, r, "fcmp")?
                    .into(),
                ir::ast::Operator::Le => self
                    .builder
                    .build_float_compare(FloatPredicate::OLT, l, r, "fcmp")?
                    .into(),
                ir::ast::Operator::GrEq => self
                    .builder
                    .build_float_compare(FloatPredicate::OGE, l, r, "fcmp")?
                    .into(),
                ir::ast::Operator::LeEq => self
                    .builder
                    .build_float_compare(FloatPredicate::OLE, l, r, "fcmp")?
                    .into(),
                _ => return Err(format!("Unsupported floating-point operator: {op:?}").into()),
            };

            return match value {
                inkwell::values::BasicValueEnum::FloatValue(value) => Ok(value.into()),
                inkwell::values::BasicValueEnum::IntValue(value) => Ok(self
                    .builder
                    .build_int_z_extend(value, self.llvm.i8_type(), "bool.zext")?
                    .into()),
                _ => Err("Unexpected LLVM value produced by floating-point operation".into()),
            };
        }

        let l = l.into_int_value();
        let r = r.into_int_value();
        let value = match op {
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
            ir::ast::Operator::And | ir::ast::Operator::BitAnd => {
                self.builder.build_and(l, r, "and")?.into()
            }
            ir::ast::Operator::Or | ir::ast::Operator::BitOr => {
                self.builder.build_or(l, r, "or")?.into()
            }
            ir::ast::Operator::Eq => self
                .builder
                .build_int_compare(IntPredicate::EQ, l, r, "icmp")?
                .into(),
            ir::ast::Operator::NEq => self
                .builder
                .build_int_compare(IntPredicate::NE, l, r, "icmp")?
                .into(),
            _ => return Err(format!("Unsupported integer operator: {op:?}").into()),
        };

        match value {
            BasicValueEnum::IntValue(value)
                if matches!(op, ir::ast::Operator::Eq | ir::ast::Operator::NEq) =>
            {
                Ok(self
                    .builder
                    .build_int_z_extend(value, self.llvm.i8_type(), "bool.zext")?
                    .into())
            }
            other => Ok(other),
        }
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
                .build_float_neg(src.into_float_value(), "fneg")
                .unwrap()
                .into(),
            ir::ast::UnaryOp::Sub => self
                .builder
                .build_int_neg(src.into_int_value(), "ineg")
                .unwrap()
                .into(),
            ir::ast::UnaryOp::Neg => {
                let value = src.into_int_value();
                let is_zero = self.builder.build_int_compare(
                    IntPredicate::EQ,
                    value,
                    value.get_type().const_zero(),
                    "logical.not",
                )?;
                self.builder
                    .build_int_z_extend(is_zero, self.llvm.i8_type(), "bool.zext")?
                    .into()
            }
            ir::ast::UnaryOp::Ref => unreachable!("Deprecated: ref"),
            ir::ast::UnaryOp::Deref => unreachable!("Deprecated: deref"),
        };

        Ok(out)
    }

    fn lower_const_value(
        &self,
        value: &ir::ast::ConstValue,
        dst_ty: AnyTypeKey,
    ) -> Result<BasicValueEnum<'ctx>, Box<dyn Error>> {
        use ir::ast::ConstValue;

        let full_ty = dst_ty.unwrap_full(&self.ctx.types);
        let result = match value {
            ConstValue::Number(number) => match &number.value {
                ir::ast::NumberValue::Float(v) => match full_ty {
                    AnyTypeKey::Primitive(PrimitiveType::F32) => {
                        self.llvm.f32_type().const_float(*v).into()
                    }
                    AnyTypeKey::Primitive(PrimitiveType::F64) => {
                        self.llvm.f64_type().const_float(*v).into()
                    }
                    _ => {
                        return Err(format!(
                            "Floating-point constant has non-floating destination '{}'",
                            dst_ty.stringify(&self.ctx.types)
                        )
                        .into());
                    }
                },
                ir::ast::NumberValue::Uint(v) | ir::ast::NumberValue::Any(v) => {
                    let ty = self.convert_type(dst_ty)?.into_int_type();
                    ty.const_int(*v as u64, false).into()
                }
                ir::ast::NumberValue::Int(v) => {
                    let ty = self.convert_type(dst_ty)?.into_int_type();
                    ty.const_int(*v as u64, true).into()
                }
            },
            ConstValue::String(value) => self
                .builder
                .build_global_string_ptr(value.as_str(), "str")?
                .as_pointer_value()
                .into(),
            ConstValue::Char(value) => self.llvm.i32_type().const_int(*value as u64, false).into(),
            ConstValue::Bool(value) => self.llvm.i8_type().const_int(*value as u64, false).into(),
            ConstValue::EnumVariant { parent, variant } => {
                let enumeration = match parent.unwrap_full(&self.ctx.types) {
                    AnyTypeKey::Enum(key) => self.ctx.types.enums.get_unchecked(&key),
                    _ => return Err("Enum variant constant has a non-enum parent type".into()),
                };
                let (_, value_key) = enumeration
                    .variants
                    .iter()
                    .find(|(name, _)| name == variant)
                    .ok_or_else(|| format!("Unknown enum variant '{variant}'"))?;
                let inner = self.ctx.constants.data.get_unchecked(value_key);
                return self.lower_const_value(inner, enumeration.repr);
            }
            ConstValue::Array { elements, .. } => {
                let array_ty = match full_ty {
                    AnyTypeKey::Array(key) => self.ctx.types.arrays.get_unchecked(&key),
                    _ => return Err("Array constant has a non-array destination type".into()),
                };
                let element_ty = array_ty.element_type;
                let values = elements
                    .iter()
                    .map(|element| self.lower_const_value(element.deref(), element_ty))
                    .collect::<Result<Vec<_>, _>>()?;
                let llvm_ty = self.convert_type(dst_ty)?;
                let array_ty = llvm_ty.into_array_type();
                let expected = array_ty.len() as usize;
                if expected != values.len() {
                    return Err(format!(
                        "Array constant has {} elements, expected {expected}",
                        values.len()
                    )
                    .into());
                }
                let element_llvm_ty = array_ty.get_element_type();
                for (index, value) in values.iter().enumerate() {
                    if value.get_type() != element_llvm_ty {
                        return Err(format!(
                            "Array constant element {index} has LLVM type '{}', expected '{}'",
                            value.get_type().print_to_string(),
                            element_llvm_ty.print_to_string(),
                        )
                        .into());
                    }
                }

                // Inkwell 0.10 exposes ArrayType::const_array only for arrays
                // whose elements are themselves ArrayValues. For arbitrary
                // element types, use the generic constructor after validating
                // that every value has exactly the array's element type.
                unsafe { ArrayValue::new_const_array(&element_llvm_ty, &values) }.into()
            }
            ConstValue::Tuple { elements, .. } => {
                let tuple = match full_ty {
                    AnyTypeKey::Tuple(key) => self.ctx.types.tuples.get_unchecked(&key),
                    _ => return Err("Tuple constant has a non-tuple destination type".into()),
                };
                if elements.len() != tuple.parameters.len() {
                    return Err("Tuple constant element count does not match its type".into());
                }
                let values = elements
                    .iter()
                    .zip(&tuple.parameters)
                    .map(|(element, ty)| self.lower_const_value(element.deref(), *ty))
                    .collect::<Result<Vec<_>, _>>()?;
                self.llvm.const_struct(&values, false).into()
            }
            ConstValue::Structure { fields, .. } => {
                let structure = match full_ty {
                    AnyTypeKey::Struct(key) => self.ctx.types.structures.get_unchecked(&key),
                    _ => return Err("Structure constant has a non-struct destination type".into()),
                };
                let mut values = Vec::with_capacity(structure.parameters.len());
                for (name, field_ty, default) in &structure.parameters {
                    let field = fields.iter().find(|field| field.deref().0.deref() == name);
                    let field_value = if let Some(field) = field {
                        field.deref().1.deref()
                    } else if let Some(default) = default {
                        default
                    } else {
                        return Err(format!("Missing field '{name}' in structure constant").into());
                    };
                    values.push(self.lower_const_value(field_value, *field_ty)?);
                }
                self.llvm.const_struct(&values, false).into()
            }
        };
        Ok(result)
    }

    fn load_const(
        &self,
        value_map: &mut HashMap<ValueKey, ValueLocation<'ctx>>,
        src: &ConstValueKey,
        dst: ValueKey,
        dst_ty: AnyTypeKey,
    ) -> Result<(), Box<dyn Error>> {
        let value = self.ctx.constants.data.get_unchecked(src);

        let value = self.lower_const_value(value, dst_ty)?;

        value_map.insert(dst, ValueLocation::Value(value));
        Ok(())
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
