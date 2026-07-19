use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;
use std::{collections::HashMap, fs};

use ir::ast::AccessModifiers;
use ir::{
    const_stage::{Context, objects::IrCache},
    ir::{Instruction, Terminator},
};
use parser::parse_directory;

fn stringify_instruction(instr: &Instruction) -> String {
    match instr {
        Instruction::LoadConst { src, dst } => format!("{:?} = const {}", dst, src.stringify()),
        Instruction::BinOp { op, l, r, dst } => format!("{:?} = {:?} {:?} {:?}", dst, l, op, r),
        Instruction::UnaryOp { op, src, dst } => format!("{:?} = {:?}{:?}", dst, op, src),
        Instruction::StoreVar { dst, src } => format!("var[{:?}] = {:?}", dst, src),
        Instruction::LoadVar { src, dst } => format!("{:?} = var[{:?}]", dst, src),
        Instruction::Call {
            fun,
            arguments,
            result,
        } => {
            let args = arguments
                .iter()
                .map(|v| format!("{v:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{:?} = call {:?}({})", result, fun, args)
        }
        Instruction::AddressOfObj { obj, dst } => format!("{:?} = &obj {:?}", dst, obj),
        Instruction::AddressOfFun { fun, dst } => format!("{:?} = &fun {:?}", dst, fun),
        Instruction::AddressOfVar { var, dst } => format!("{:?} = &var {:?}", dst, var),
        Instruction::AddressOfVal { val, dst } => format!("{:?} = &val {:?}", dst, val),
        Instruction::Deref { src, dst } => format!("{:?} = *{:?}", dst, src),
        Instruction::Exit(val) => format!("exit {:?}", val),
    }
}

fn generate_instr_elements(instr: &Instruction) -> Vec<IrElement> {
    use IrElement::*;

    match instr {
        Instruction::LoadConst { src, dst } => vec![
            Value { id: dst.id() },
            Text(" = const ".into()),
            Text(src.stringify().to_string()), // Assuming src stringifies to constant value[cite: 5]
        ],
        Instruction::BinOp { op, l, r, dst } => vec![
            Value { id: dst.id() },
            Text(" = ".into()),
            Value { id: l.id() },
            Operator(format!(" {:?} ", op)),
            Value { id: r.id() },
        ],
        Instruction::UnaryOp { op, src, dst } => vec![
            Value { id: dst.id() },
            Text(" = ".into()),
            Operator(format!("{:?} ", op)),
            Value { id: src.id() },
        ],
        Instruction::StoreVar { dst, src } => vec![
            Variable { id: dst.id() },
            Text(" = ".into()),
            Value { id: src.id() },
        ],
        Instruction::LoadVar { src, dst } => vec![
            Value { id: dst.id() },
            Text(" = ".into()),
            Variable { id: src.id() },
        ],
        Instruction::Call {
            fun,
            arguments,
            result,
        } => {
            let mut elements = vec![
                Value { id: result.id() },
                Text(" = call ".into()),
                Function { id: fun.id() },
                Text("(".into()),
            ];

            for (i, arg) in arguments.iter().enumerate() {
                elements.push(Value { id: arg.id() });
                if i < arguments.len() - 1 {
                    elements.push(Text(", ".into()));
                }
            }
            elements.push(Text(")".into()));
            elements
        }
        Instruction::AddressOfObj { obj, dst } => {
            vec![
                Value { id: dst.id() },
                Text(format!(" = ref obj {:?}", obj)),
            ]
        }
        Instruction::AddressOfFun { fun, dst } => vec![
            Value { id: dst.id() },
            Text(" = ref fun ".into()),
            Function { id: fun.id() },
        ],
        Instruction::AddressOfVar { var, dst } => vec![
            Value { id: dst.id() },
            Text(" = ref var ".into()),
            Variable { id: var.id() },
        ],
        Instruction::AddressOfVal { val, dst } => vec![
            Value { id: dst.id() },
            Text(" = ref val ".into()),
            Value { id: val.id() },
        ],
        Instruction::Deref { src, dst } => vec![
            Value { id: dst.id() },
            Text(" = deref ".into()),
            Value { id: src.id() },
        ],
        Instruction::Exit(val) => vec![Text("exit ".into()), Value { id: val.id() }],
    }
}

fn instruction_description(instr: &Instruction) -> &'static str {
    match instr {
        Instruction::LoadConst { .. } => "Load a constant value",
        Instruction::BinOp { .. } => "Binary arithmetic or logical operation",
        Instruction::UnaryOp { .. } => "Unary arithmetic or logical operation",
        Instruction::StoreVar { .. } => "Store a value into a variable",
        Instruction::LoadVar { .. } => "Load a value from a variable",
        Instruction::Call { .. } => "Execute a function call",
        Instruction::AddressOfObj { .. } => "Get the memory address of an object",
        Instruction::AddressOfFun { .. } => "Get the memory address of a function",
        Instruction::AddressOfVar { .. } => "Get the memory address of a variable",
        Instruction::AddressOfVal { .. } => "Get the memory address of a value",
        Instruction::Deref { .. } => "Dereference a pointer to a value",
        Instruction::Exit(_) => "Terminate execution",
    }
}

fn stringify_terminator(term: &Terminator) -> String {
    match term {
        Terminator::Return(Some(val)) => format!("ret {:?}", val),
        Terminator::Return(None) => "ret".to_string(),
        Terminator::Jump(blk, Some(val)) => format!("jmp {:?} ({:?})", blk, val),
        Terminator::Jump(blk, None) => format!("jmp {:?}", blk),
        Terminator::Branch {
            condition,
            then_block,
            else_block,
        } => {
            format!("br {:?}, {:?}, {:?}", condition, then_block, else_block)
        }
        Terminator::Eval(val) => format!("eval {:?}", val),
        Terminator::Unreachable => "unreachable".to_string(),
        Terminator::Exit(val) => format!("exit {:?}", val),
    }
}

fn terminator_description(term: &Terminator) -> &'static str {
    match term {
        Terminator::Return(_) => "Return from function",
        Terminator::Jump(..) => "Jump to block",
        Terminator::Branch { .. } => "Conditional branch",
        Terminator::Eval(_) => "Evaluate and discard value",
        Terminator::Unreachable => "Unreachable path",
        Terminator::Exit(_) => "Terminate execution",
    }
}

fn byte_to_line_index(raw_source: &str, byte_index: usize) -> usize {
    raw_source
        .char_indices()
        .take_while(|&(idx, _)| idx < byte_index)
        .filter(|&(_, ch)| ch == '\n')
        .count()
}

impl IrElement {
    /// Convert an element to its string representation for rendering/debugging.
    pub fn stringify(&self) -> String {
        match self {
            IrElement::Text(t) => t.clone(),
            IrElement::Variable { id } => format!("var_{}", id),
            IrElement::Value { id } => format!("val_{}", id),
            IrElement::Function { id } => format!("fn_{}", id),
            IrElement::Operator(op) => op.clone(),
            IrElement::Block(id) => format!("block_{}", id),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UiModule {
    pub id: usize,
    pub name: String,
    pub objects: Vec<UiModuleObject>,
}

#[derive(Debug, Clone)]
pub struct UiModuleObject {
    pub name: String,
    pub is_exported: bool,
    pub is_polymorphic: bool,
}

#[derive(Debug, Clone)]
pub struct UiVariable {
    pub id: usize,
    pub identifier: String,
    pub ty: String,
}

#[derive(Debug, Clone)]
pub struct UiValue {
    pub id: usize,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstructionKind {
    BlockLabel,
    Normal,
    Terminator,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IrElement {
    Text(String),
    Variable { id: usize },
    Value { id: usize },
    Function { id: usize },
    Operator(String),
    Block(usize),
}

#[derive(Debug, Clone)]
pub struct UiIrInstruction {
    pub elements: Vec<IrElement>,
    pub source_line_index: Option<usize>,
    pub block_idx: usize,
    pub description: String,
    pub kind: InstructionKind,
}

#[derive(Debug, Clone)]
pub struct ObjectDetails {
    pub source_lines: Vec<String>,
    pub ir_variables: Vec<UiVariable>,
    pub ir_values: Vec<UiValue>,
    pub ir_instructions: Vec<UiIrInstruction>,
    pub color_index: usize,
}

pub struct LoadedProject {
    pub modules: Vec<UiModule>,
    pub details: HashMap<String, ObjectDetails>,
    pub function_map: HashMap<usize, String>, // Feature 4: Map Function ID to global String target
}

pub fn load_project(dir_path: &Path) -> Result<LoadedProject, String> {
    let modules = parse_directory(dir_path, None, |str, path, e| {
        let _ = e.print(str, Some(path));
    })
    .map_err(|_| "Failed to parse directory structure".to_string())?;

    let mut details_map = HashMap::new();
    let mut ui_modules = Vec::new();
    let mut function_map = HashMap::new();

    let ast_map = HashMap::from_iter(
        modules
            .iter()
            .map(|(key, mok)| (key.clone(), Arc::new(mok.module.clone()))),
    );
    let ir_ctx = Context::from_ast(ast_map).map_err(|(_, _)| "IR error".to_string())?;

    for (mod_idx, (path_vec, module_ok)) in modules.iter().enumerate() {
        let module_name = path_vec
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("::");
        let source_path = module_ok
            .module
            .path
            .clone()
            .unwrap_or_else(|| dir_path.to_path_buf());
        let raw_source = fs::read_to_string(&source_path).unwrap_or_default();
        let source_lines: Vec<String> = raw_source.lines().map(String::from).collect();
        let mut ui_objects = Vec::new();

        for (key, fun) in ir_ctx.objects.functions.iter_pairs() {
            if let Some(module) = ir_ctx.types.modules.get(&fun.module) {
                if module.src == module_ok.module.src {
                    ui_objects.push(UiModuleObject {
                        name: fun.identifier.to_string(),
                        is_exported: !matches!(fun.access, AccessModifiers::Private),
                        is_polymorphic: matches!(fun.data.ir, IrCache::Polymorphic(_)),
                    });

                    let target_name = format!("{}::{}", module_name, fun.identifier);
                    // Map the compiler's function ID to the global target name for navigation
                    function_map.insert(key.id(), target_name.clone());

                    if let IrCache::Single(ir_handle) = &fun.data.ir {
                        let ir = ir_ctx.ir_cache.get(ir_handle.get_done()).unwrap();

                        let mut ui_variables = Vec::new();
                        for (i, v) in ir.variables.iter().enumerate() {
                            ui_variables.push(UiVariable {
                                id: i, // Or use a specific ID if the variable object exposes one
                                identifier: v.identifier.to_string(),
                                ty: v.ty.stringify(&ir_ctx.types).to_string(),
                            });
                        }

                        let mut ui_values = Vec::new();
                        for (i, val) in ir.values.iter().enumerate() {
                            ui_values.push(UiValue {
                                id: i,
                                ty: val.ty.stringify(&ir_ctx.types).to_string(),
                            });
                        }

                        let mut ui_instructions = Vec::new();
                        for (b_idx, block) in ir.blocks.arena().iter().enumerate() {
                            ui_instructions.push(UiIrInstruction {
                                elements: vec![IrElement::Block(b_idx)],
                                source_line_index: None,
                                block_idx: b_idx,
                                description: "Block Label".into(),
                                kind: InstructionKind::BlockLabel,
                            });

                            for wrapped in block.value.instructions() {
                                ui_instructions.push(UiIrInstruction {
                                    elements: generate_instr_elements(&wrapped.inner),
                                    source_line_index: Some(byte_to_line_index(
                                        &raw_source,
                                        wrapped.location.index,
                                    )),
                                    block_idx: b_idx,
                                    description: instruction_description(&wrapped.inner).into(),
                                    kind: InstructionKind::Normal,
                                });
                            }

                            if let Some(term) = block.value.terminator() {
                                ui_instructions.push(UiIrInstruction {
                                    elements: vec![IrElement::Text(stringify_terminator(term))], // Terminator[cite: 5]
                                    source_line_index: None, // Terminators often don't map to source lines in same way[cite: 5]
                                    block_idx: b_idx,
                                    description: terminator_description(term).into(),
                                    kind: InstructionKind::Terminator,
                                });
                            }
                        }

                        let color_index = (details_map.len()) % 4;

                        details_map.insert(
                            target_name,
                            ObjectDetails {
                                source_lines: source_lines.clone(),
                                ir_variables: ui_variables,
                                ir_values: ui_values,
                                ir_instructions: ui_instructions,
                                color_index,
                            },
                        );
                    }
                }
            }
        }
        ui_modules.push(UiModule {
            id: mod_idx,
            name: module_name,
            objects: ui_objects,
        });
    }
    Ok(LoadedProject {
        modules: ui_modules,
        details: details_map,
        function_map,
    })
}
