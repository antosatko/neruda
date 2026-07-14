use std::{collections::HashMap, ops::Deref};

use crate::{
    ast::{ConstValue, Operator, Span, UnaryOp},
    ir::{
        FunctionIr, FunctionIrArena, FunctionIrKey, Instruction, Terminator, ValueKey, VariableKey,
    },
};

#[derive(Debug)]
pub struct Interpreter {
    pub functions: FunctionIrArena,
}

impl Interpreter {
    pub fn new(functions: FunctionIrArena) -> Self {
        Self { functions }
    }

    pub fn interpret_function(
        &self,
        function: FunctionIrKey,
        args: Vec<InterpreterValue>,
    ) -> Result<InterpreterValue, String> {
        let function_ir = self.functions.get_unchecked(&function);

        let mut frame = Frame::new();

        for ((_, variable), value) in function_ir.parameters.iter().zip(args) {
            frame.variables.insert(*variable, value);
        }

        self.execute_function(function_ir, &mut frame)
    }

    fn execute_function(
        &self,
        function: &FunctionIr,
        frame: &mut Frame,
    ) -> Result<InterpreterValue, String> {
        let mut block = function.blocks_entry;

        loop {
            let instructions = {
                let block_ref = function.blocks.node(&block);
                block_ref.value.instructions.clone()
            };

            for instruction in instructions {
                self.execute_instruction(frame, instruction)?;
            }

            let terminator = {
                let block_ref = function.blocks.node(&block);
                block_ref.value.terminator.clone()
            };

            match terminator {
                Some(Terminator::Return(value)) => {
                    return match value {
                        Some(value) => Ok(frame.get_value(value)?),

                        None => Ok(InterpreterValue::Unit),
                    };
                }

                Some(Terminator::Jump(target, _)) => {
                    block = target;
                }

                Some(Terminator::Eval(value)) => return frame.get_value(value),

                Some(Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                }) => {
                    let value = frame.get_value(condition)?;

                    match value {
                        InterpreterValue::Bool(true) => {
                            block = then_block;
                        }

                        InterpreterValue::Bool(false) => {
                            block = else_block;
                        }

                        _ => {
                            return Err(format!("branch condition must be bool, got {:?}", value));
                        }
                    }
                }

                Some(Terminator::Unreachable) => return Err("Reached unreachable".into()),

                None => return dbg!(Ok(InterpreterValue::Unit)),
            }
        }
    }

    fn execute_instruction(
        &self,
        frame: &mut Frame,
        instruction: Span<Instruction>,
    ) -> Result<(), String> {
        match instruction.deref() {
            Instruction::LoadConst { src, dst } => {
                frame
                    .values
                    .insert(*dst, InterpreterValue::from_const_value(&src));
            }

            Instruction::LoadVar { src, dst } => {
                let value = frame
                    .variables
                    .get(&src)
                    .cloned()
                    .ok_or_else(|| format!("uninitialized variable {:?}", src))?;

                frame.values.insert(*dst, value);
            }

            Instruction::StoreVar { dst, src } => {
                let value = frame.get_value(*src)?;

                frame.variables.insert(*dst, value);
            }

            Instruction::BinOp { op, l, r, dst } => {
                let left = frame.get_value(*l)?;
                let right = frame.get_value(*r)?;

                let result = evaluate_binary_op(&op, &left, &right)?;

                frame.values.insert(*dst, result);
            }

            Instruction::UnaryOp { op, src, dst } => {
                let value = frame.get_value(*src)?;

                let result = evaluate_unary_op(&op, &value)?;

                frame.values.insert(*dst, result);
            }

            Instruction::Call {
                fun,
                arguments,
                result,
            } => {
                let args = arguments
                    .into_iter()
                    .map(|x| frame.get_value(*x))
                    .collect::<Result<Vec<_>, _>>()?;

                let function = self.functions.get_unchecked(&fun);

                let mut child = Frame::new();

                for ((_, variable), value) in function.parameters.iter().zip(args) {
                    child.variables.insert(*variable, value);
                }

                let returned = self.execute_function(function, &mut child)?;

                frame.values.insert(*result, returned);
            }

            Instruction::AddressOfVar { var, dst } => {
                frame
                    .values
                    .insert(*dst, InterpreterValue::Pointer(Pointer::Variable(*var)));
            }

            Instruction::AddressOfVal { val, dst } => {
                frame
                    .values
                    .insert(*dst, InterpreterValue::Pointer(Pointer::Value(*val)));
            }

            Instruction::Deref { src, dst } => {
                let pointer = frame.get_value(*src)?;

                let value = match pointer {
                    InterpreterValue::Pointer(Pointer::Variable(variable)) => frame
                        .variables
                        .get(&variable)
                        .cloned()
                        .ok_or("invalid variable pointer")?,

                    InterpreterValue::Pointer(Pointer::Value(value)) => frame
                        .values
                        .get(&value)
                        .cloned()
                        .ok_or("invalid value pointer")?,

                    _ => return Err("attempted dereference of non pointer".into()),
                };

                println!("Val: {value:?}");

                frame.values.insert(*dst, value);
            }

            Instruction::AddressOfObj { .. } => return Err("objects not implemented".into()),

            Instruction::AddressOfFun { fun, dst } => {
                frame.values.insert(*dst, InterpreterValue::Function(*fun));
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct Frame {
    pub values: HashMap<ValueKey, InterpreterValue>,

    pub variables: HashMap<VariableKey, InterpreterValue>,
}

impl Frame {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            variables: HashMap::new(),
        }
    }

    fn get_value(&self, key: ValueKey) -> Result<InterpreterValue, String> {
        self.values
            .get(&key)
            .cloned()
            .ok_or_else(|| format!("missing value {:?}", key))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InterpreterValue {
    Number(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Char(char),

    Pointer(Pointer),

    Function(FunctionIrKey),

    Unit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pointer {
    Variable(VariableKey),

    Value(ValueKey),
}

impl InterpreterValue {
    pub fn from_const_value(value: &ConstValue) -> Self {
        match value {
            ConstValue::Number(number) => match number.value {
                crate::ast::NumberValue::Any(v) => Self::Number(v as i64),
                crate::ast::NumberValue::Float(v) => Self::Float(v),
                _ => todo!("add number variants"),
            },

            ConstValue::Bool(v) => Self::Bool(*v),

            ConstValue::Char(v) => Self::Char(*v),

            ConstValue::String(v) => Self::String(v.to_string()),

            _ => todo!("complex constants"),
        }
    }
}
fn evaluate_binary_op(
    op: &Operator,
    left: &InterpreterValue,
    right: &InterpreterValue,
) -> Result<InterpreterValue, String> {
    match op {
        Operator::Add => match (left, right) {
            (InterpreterValue::Number(l), InterpreterValue::Number(r)) => {
                Ok(InterpreterValue::Number(l + r))
            }
            (InterpreterValue::Float(l), InterpreterValue::Float(r)) => {
                Ok(InterpreterValue::Float(l + r))
            }
            (InterpreterValue::Number(l), InterpreterValue::Float(r)) => {
                Ok(InterpreterValue::Float(*l as f64 + r))
            }
            (InterpreterValue::Float(l), InterpreterValue::Number(r)) => {
                Ok(InterpreterValue::Float(l + *r as f64))
            }
            _ => Err("Invalid types for addition".to_string()),
        },
        Operator::Sub => match (left, right) {
            (InterpreterValue::Number(l), InterpreterValue::Number(r)) => {
                Ok(InterpreterValue::Number(l - r))
            }
            (InterpreterValue::Float(l), InterpreterValue::Float(r)) => {
                Ok(InterpreterValue::Float(l - r))
            }
            (InterpreterValue::Number(l), InterpreterValue::Float(r)) => {
                Ok(InterpreterValue::Float(*l as f64 - r))
            }
            (InterpreterValue::Float(l), InterpreterValue::Number(r)) => {
                Ok(InterpreterValue::Float(l - *r as f64))
            }
            _ => Err("Invalid types for subtraction".to_string()),
        },
        Operator::Mul => match (left, right) {
            (InterpreterValue::Number(l), InterpreterValue::Number(r)) => {
                Ok(InterpreterValue::Number(l * r))
            }
            (InterpreterValue::Float(l), InterpreterValue::Float(r)) => {
                Ok(InterpreterValue::Float(l * r))
            }
            (InterpreterValue::Number(l), InterpreterValue::Float(r)) => {
                Ok(InterpreterValue::Float(*l as f64 * r))
            }
            (InterpreterValue::Float(l), InterpreterValue::Number(r)) => {
                Ok(InterpreterValue::Float(l * *r as f64))
            }
            _ => Err("Invalid types for multiplication".to_string()),
        },
        Operator::Div => match (left, right) {
            (InterpreterValue::Number(l), InterpreterValue::Number(r)) => {
                if r == &0 {
                    Err("Division by zero".to_string())
                } else {
                    Ok(InterpreterValue::Number(l / r))
                }
            }
            (InterpreterValue::Float(l), InterpreterValue::Float(r)) => {
                if r == &0.0 {
                    Err("Division by zero".to_string())
                } else {
                    Ok(InterpreterValue::Float(l / r))
                }
            }
            (InterpreterValue::Number(l), InterpreterValue::Float(r)) => {
                if r == &0.0 {
                    Err("Division by zero".to_string())
                } else {
                    Ok(InterpreterValue::Float(*l as f64 / r))
                }
            }
            (InterpreterValue::Float(l), InterpreterValue::Number(r)) => {
                if r == &0 {
                    Err("Division by zero".to_string())
                } else {
                    Ok(InterpreterValue::Float(l / *r as f64))
                }
            }
            (l, r) => Err(format!("Invalid values for division: '{l:?}', '{r:?}'")),
        },
        Operator::Eq => Ok(InterpreterValue::Bool(left == right)),
        Operator::NEq => Ok(InterpreterValue::Bool(left != right)),
        _ => Err(format!("Operator {:?} not implemented in interpreter", op)),
    }
}

fn evaluate_unary_op(op: &UnaryOp, value: &InterpreterValue) -> Result<InterpreterValue, String> {
    match op {
        UnaryOp::Sub => match value {
            InterpreterValue::Number(v) => Ok(InterpreterValue::Number(-v)),
            InterpreterValue::Float(v) => Ok(InterpreterValue::Float(-v)),
            _ => Err("Invalid type for unary minus".to_string()),
        },
        UnaryOp::Neg => match value {
            InterpreterValue::Bool(v) => Ok(InterpreterValue::Bool(!v)),
            _ => Err("Invalid type for negation".to_string()),
        },
        UnaryOp::Ref => {
            // Return a reference to the value (we'll just return the value for simplicity)
            Ok(value.clone())
        }
        UnaryOp::Deref => {
            // Dereference the value (we'll just return the value for simplicity)
            Ok(value.clone())
        }
    }
}
