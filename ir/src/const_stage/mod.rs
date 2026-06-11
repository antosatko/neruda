pub mod lowering;
pub mod objects;
pub mod types;

use std::{collections::HashMap, sync::Arc};

use arena_scope::ScopeTree;
use smol_str::SmolStr;

use crate::{
    ast::{self, ConstValue, Operator, SpanIndex},
    const_stage::{
        objects::{AnyObjectKey, FunctionObjKey, Objects},
        types::{AnyTypeKey, AutoTypes, FunctionKey, ModuleKey},
    },
    generics::GContext,
    ir::VariableKey,
};

use self::types::Types;

#[derive(Default, Debug)]
pub struct Diagnostics {
    pub warnings: Vec<Warning>,
}

#[derive(Debug)]
pub struct Diagnostic<T> {
    pub span: SpanIndex,
    pub module: ModuleKey,
    pub inner: T,
}

pub type Error = Diagnostic<Errors>;
pub type Warning = Diagnostic<Warnings>;

#[derive(Debug)]
pub enum Errors {
    IllegalType(AnyObjectKey),
    TypeNotFound(Vec<SmolStr>),
    ObjectNotFound(Vec<SmolStr>),
    TypeMismatch {
        expected: AnyTypeKey,
        got: AnyTypeKey,
    },
    NonConstraintType(AnyObjectKey),
    CouldNotSubstituteType(AnyTypeKey),
    NotConst,
    CanNotApplyConst {
        op: Operator,
        left: ConstValue,
        right: ConstValue,
    },
    EvalModule(Vec<SmolStr>, ModuleKey),
    UndefinedSelf,
    NonPrimitiveType {
        got: AnyTypeKey,
    },
    ExpectedNumericConst {
        got: ConstValue,
    },
    DuplicateIdentifier(SmolStr),
    GenericArityMismatch {
        expected: usize,
        found: usize,
    },
    FailedTypeInfer,
    UndefinedAutostep(AnyTypeKey),
    UndefinedDefault(AnyTypeKey),
    FailedImplicitCast {
        from: AnyTypeKey,
        to: AnyTypeKey,
    },
    ArrayElementCountMismatch {
        expected: (AnyTypeKey, usize),
        got: (AnyTypeKey, Option<usize>),
    },
    ExpectedOptionalResource {
        ident: SmolStr,
    },
    VariableNotFound(SmolStr),
    ObjectInaccessibleInBlock(AnyObjectKey),
    ExpectedReturnExpression(FunctionObjKey),
}

#[derive(Debug)]
pub enum Warnings {
    VariableUnused {
        function: FunctionObjKey,
        var: VariableKey,
    },
}

pub struct Context {
    pub types: Types,
    pub objects: Objects,
    pub auto_types: AutoTypes,
    pub ast: HashMap<Vec<SmolStr>, Arc<ast::Module>>,
    pub diagnostics: Diagnostics,
    pub generic_ctx: GContext,
}

impl Context {
    pub fn from_ast(ast: HashMap<Vec<SmolStr>, Arc<ast::Module>>) -> Result<Self, (Self, Error)> {
        let mut types = Types::default();
        let objects = Default::default();
        let auto_types = AutoTypes::new(&mut types);
        let mut this = Self {
            types,
            objects,
            auto_types,
            diagnostics: Diagnostics::default(),
            ast,
            generic_ctx: ScopeTree::default(),
        };

        if let Err(e) = this.lower_import_stage() {
            return Err((this, e));
        }

        if let Err(e) = this.lower_const_stage() {
            return Err((this, e));
        }

        if let Err(e) = this.lower_ir() {
            return Err((this, e));
        }
        // next stages here

        Ok(this)
    }
}
