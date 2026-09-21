use std::{collections::HashSet, ops::Deref};

use arena::Arena;
use arena_scope::stack::{Stack, StackKey};

use crate::{
    const_stage::Context,
    ir::{
        BasicBlock, BlockKey, FunctionIrKey, Terminator, Value, ValueTag, Variable, VariableArena,
        VariableTag,
    },
};

pub type PastBlocks = HashSet<BlockKey>;

pub fn dead_code_elimination(ctx: &mut Context, ir_key: &FunctionIrKey) {
    //return;
    let ir = ctx.ir_cache.get_mut_unchecked(ir_key);
    let mut past_block = HashSet::new();
    block_dead_code_elim(
        &mut ir.blocks,
        &ir.blocks_entry,
        &mut ir.values,
        &mut ir.variables,
        &mut past_block,
    );
}

fn block_dead_code_elim(
    blocks: &mut Stack<BasicBlock>,
    block_key: &StackKey,
    values: &mut Arena<Value, ValueTag>,
    variables: &mut VariableArena,
    past_blocks: &mut PastBlocks,
) {
    macro_rules! mark_used {
        ($val: expr) => {
            values.get_mut_unchecked($val).used = true;
        };
    }
    match past_blocks.get(block_key) {
        None => past_blocks.insert(*block_key),
        _ => return,
    };
    let block = &blocks.arena_mut().get_unchecked(block_key).value;
    match block.terminator().clone() {
        Some(Terminator::Jump(key, val)) => {
            if let Some(val) = &val {
                mark_used!(val);
            }
            block_dead_code_elim(blocks, &key, values, variables, past_blocks);
        }
        Some(Terminator::Branch {
            condition,
            then_block,
            else_block,
        }) => {
            mark_used!(&condition);
            block_dead_code_elim(blocks, &then_block, values, variables, past_blocks);
            block_dead_code_elim(blocks, &else_block, values, variables, past_blocks);
        }
        Some(Terminator::Return(Some(val))) => {
            mark_used!(&val);
        }
        _ => (),
    }
    let block = &mut blocks.arena_mut().get_mut_unchecked(block_key).value;
    let mut eliminated = Vec::with_capacity(block.instructions().len());
    for instr in block.instructions.iter_mut().rev() {
        match &mut instr.inner.deref() {
            super::Instruction::BinOp {
                op: _,
                l,
                r,
                dst,
                ty: _,
            } => {
                if !values.get_unchecked(dst).used {
                    continue;
                }
                mark_used!(l);
                mark_used!(r);
            }
            super::Instruction::UnaryOp {
                op: _,
                src,
                dst,
                ty: _,
            } => {
                if !values.get_unchecked(dst).used {
                    continue;
                }
                mark_used!(src);
            }
            super::Instruction::StoreVar { dst, src } => {
                if !variables.get_unchecked(dst).used {
                    continue;
                }
                mark_used!(src);
            }
            super::Instruction::LoadVar { src, dst } => {
                if !values.get_unchecked(dst).used {
                    continue;
                }
                variables.get_mut_unchecked(src).used = true;
            }
            super::Instruction::Call {
                fun: _,
                arguments,
                result: _,
            } => {
                for arg in arguments {
                    values.get_mut_unchecked(arg).used = true;
                }
            }
            super::Instruction::AddressOfObj { obj: _, dst }
            | super::Instruction::AddressOfFun { fun: _, dst }
            | super::Instruction::LoadConst { src: _, dst } => {
                if !values.get_unchecked(dst).used {
                    continue;
                }
            }
            super::Instruction::AddressOfVar { var, dst } => {
                if !values.get_unchecked(dst).used {
                    continue;
                }
                variables.get_mut_unchecked(var).used = true;
            }
            super::Instruction::AddressOfVal { val, dst } => {
                if !values.get_unchecked(dst).used {
                    continue;
                }
                mark_used!(val);
            }
            super::Instruction::Deref { src, dst } => {
                if !values.get_unchecked(dst).used {
                    continue;
                }
                mark_used!(src);
            }
        }
        eliminated.push(instr.clone());
    }
    eliminated.reverse();
    block.instructions = eliminated;
}
