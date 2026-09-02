use std::path::Path;
use std::sync::Arc;
use std::{collections::HashMap, fs};

use ir::ast::AccessModifiers;
use ir::ir::{BlockKey, FunctionIrKey, ValueKey, VariableKey};
use ir::{
    const_stage::{Context, objects::IrCache},
    ir::{Instruction, Terminator},
};
use parser::parse_directory;

fn generate_instr_elements(instr: &Instruction) -> Vec<IrElement> {
    use IrElement::*;

    match instr.clone() {
        Instruction::LoadConst { src, dst } => vec![
            Value { id: dst },
            Text(" = const ".into()),
            Text(format!("key_{}", src.id())),
        ],
        Instruction::BinOp {
            op,
            l,
            r,
            dst,
            ty: _,
        } => vec![
            Value { id: dst },
            Text(" = ".into()),
            Value { id: l },
            Operator(format!(" {:?} ", op)),
            Value { id: r },
        ],
        Instruction::UnaryOp { op, src, dst } => vec![
            Value { id: dst },
            Text(" = ".into()),
            Operator(format!("{:?} ", op)),
            Value { id: src },
        ],
        Instruction::StoreVar { dst, src } => {
            vec![Variable { id: dst }, Text(" = ".into()), Value { id: src }]
        }
        Instruction::LoadVar { src, dst } => {
            vec![Value { id: dst }, Text(" = ".into()), Variable { id: src }]
        }
        Instruction::Call {
            fun,
            arguments,
            result,
        } => {
            let mut elements = vec![
                Value { id: result },
                Text(" = call ".into()),
                Function { id: fun },
                Text("(".into()),
            ];

            for (i, arg) in arguments.iter().enumerate() {
                elements.push(Value { id: *arg });
                if i < arguments.len() - 1 {
                    elements.push(Text(", ".into()));
                }
            }
            elements.push(Text(")".into()));
            elements
        }
        Instruction::AddressOfObj { obj, dst } => {
            vec![Value { id: dst }, Text(format!(" = ref obj {:?}", obj))]
        }
        Instruction::AddressOfFun { fun, dst } => vec![
            Value { id: dst },
            Text(" = ref fun ".into()),
            Function { id: fun },
        ],
        Instruction::AddressOfVar { var, dst } => vec![
            Value { id: dst },
            Text(" = ref var ".into()),
            Variable { id: var },
        ],
        Instruction::AddressOfVal { val, dst } => vec![
            Value { id: dst },
            Text(" = ref val ".into()),
            Value { id: val },
        ],
        Instruction::Deref { src, dst } => vec![
            Value { id: dst },
            Text(" = deref ".into()),
            Value { id: src },
        ],
    }
}

fn generate_terminator_elements(term: &Terminator) -> Vec<IrElement> {
    use IrElement::*;

    match term.clone() {
        Terminator::Return(Some(val)) => vec![Text("ret ".into()), Value { id: val }],
        Terminator::Return(None) => vec![Text("ret".into())],
        Terminator::Jump(blk, Some(val)) => vec![
            Text("jmp ".into()),
            Block(blk),
            Text(" (".into()),
            Value { id: val },
            Text(")".into()),
        ],
        Terminator::Jump(blk, None) => vec![Text("jmp ".into()), Block(blk)],
        Terminator::Branch {
            condition,
            then_block,
            else_block,
        } => vec![
            Text("br ".into()),
            Value { id: condition },
            Text(", ".into()),
            Block(then_block),
            Text(", ".into()),
            Block(else_block),
        ],
        Terminator::Unreachable => vec![Text("unreachable".into())],
        Terminator::Exit(code) => vec![Text(format!("exit {code}"))],
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
    }
}

fn terminator_description(term: &Terminator) -> &'static str {
    match term {
        Terminator::Return(_) => "Return from function",
        Terminator::Jump(..) => "Jump to block",
        Terminator::Branch { .. } => "Conditional branch",
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
    pub fn stringify(&self) -> String {
        match self {
            IrElement::Text(t) => t.clone(),
            IrElement::Variable { id } => format!("var_{}", id.id()),
            IrElement::Value { id } => format!("val_{}", id.id()),
            IrElement::Function { id } => format!("fn_{}", id.id()),
            IrElement::Operator(op) => op.clone(),
            IrElement::Block(id) => format!("block_{}", id.id()),
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
    pub morphed_versions: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct UiVariable {
    pub id: VariableKey,
    pub identifier: String,
    pub ty: String,
}

#[derive(Debug, Clone)]
pub struct UiValue {
    pub id: ValueKey,
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
    Variable { id: VariableKey },
    Value { id: ValueKey },
    Function { id: FunctionIrKey },
    Operator(String),
    Block(BlockKey),
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
    pub function_map: HashMap<usize, String>,
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

        for (_key, fun) in ir_ctx.objects.functions.iter_pairs() {
            if let Some(module) = ir_ctx.types.modules.get(&fun.module) {
                if module.src == module_ok.module.src {
                    let mut morphed_versions = Vec::new();

                    match &fun.data.ir {
                        IrCache::Single(ir_handle) => {
                            let ir_key = ir_handle.get_done();
                            let target_name = format!("{}::{}", module_name, fun.identifier);
                            morphed_versions.push((String::new(), target_name, ir_key));
                        }
                        IrCache::Polymorphic(map) => {
                            for (type_key, ir_handle) in map {
                                let ir_key = ir_handle.get_done();
                                let signature = type_key.stringify(&ir_ctx.types).to_string();
                                let target_name =
                                    format!("{}::{}<{}>", module_name, fun.identifier, signature);
                                morphed_versions.push((signature, target_name, ir_key));
                            }
                        }
                    }

                    morphed_versions.sort_by(|a, b| a.1.cmp(&b.1));

                    let is_polymorphic = matches!(fun.data.ir, IrCache::Polymorphic(_));

                    ui_objects.push(UiModuleObject {
                        name: fun.identifier.to_string(),
                        is_exported: !matches!(fun.access, AccessModifiers::Private),
                        is_polymorphic,
                        morphed_versions: morphed_versions
                            .iter()
                            .map(|(sig, target, _)| (sig.clone(), target.clone()))
                            .collect(),
                    });

                    for (_signature, target_name, ir_key) in morphed_versions {
                        function_map.insert(ir_key.id(), target_name.clone());

                        if let Some(ir) = ir_ctx.ir_cache.get(ir_key) {
                            let mut ui_variables = Vec::new();
                            for (i, v) in ir.variables.iter_pairs() {
                                ui_variables.push(UiVariable {
                                    id: i,
                                    identifier: v.identifier.to_string(),
                                    ty: v.ty.stringify(&ir_ctx.types).to_string(),
                                });
                            }

                            let mut ui_values = Vec::new();
                            for (i, val) in ir.values.iter_pairs() {
                                ui_values.push(UiValue {
                                    id: i,
                                    ty: val.ty.stringify(&ir_ctx.types).to_string(),
                                });
                            }

                            let mut ui_instructions = Vec::new();
                            for (b_idx, block) in ir.blocks.arena().iter_pairs() {
                                ui_instructions.push(UiIrInstruction {
                                    elements: match block.value.parameter {
                                        None => vec![IrElement::Block(b_idx)],
                                        Some(param) => {
                                            vec![
                                                IrElement::Block(b_idx),
                                                IrElement::Text(" (".into()),
                                                IrElement::Value { id: param },
                                                IrElement::Text(")".into()),
                                            ]
                                        }
                                    },
                                    source_line_index: None,
                                    block_idx: b_idx.id(),
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
                                        block_idx: b_idx.id(),
                                        description: instruction_description(&wrapped.inner).into(),
                                        kind: InstructionKind::Normal,
                                    });
                                }

                                if let Some(term) = block.value.terminator() {
                                    ui_instructions.push(UiIrInstruction {
                                        elements: generate_terminator_elements(term),
                                        source_line_index: None,
                                        block_idx: b_idx.id(),
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
