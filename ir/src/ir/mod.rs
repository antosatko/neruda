#[cfg(feature = "format")]
pub mod format_err;
pub mod lowering;
pub mod objects;
pub mod types;

use std::{collections::HashMap, sync::Arc};

use smol_str::SmolStr;

use crate::{
    ast::{self, ConstValue, Operator, SpanIndex},
    ir::{
        objects::{AnyObjectKey, Objects},
        types::{AnyTypeKey, AutoTypes, ModuleKey},
    },
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
}

#[derive(Debug)]
pub enum Warnings {}

pub struct Context {
    pub types: Types,
    pub objects: Objects,
    pub auto_types: AutoTypes,
    pub ast: HashMap<Vec<SmolStr>, Arc<ast::Module>>,
    pub diagnostics: Diagnostics,
}

impl Context {
    pub fn from_ast(ast: HashMap<Vec<SmolStr>, Arc<ast::Module>>) -> Result<Self, (Self, Error)> {
        let mut types = Types::default();
        let auto_types = AutoTypes::new(&mut types);
        let mut this = Self {
            types,
            objects: Default::default(),
            auto_types,
            diagnostics: Diagnostics::default(),
            ast,
        };

        if let Err(e) = this.lower_import_stage() {
            return Err((this, e));
        }

        if let Err(e) = this.lower_const_stage() {
            return Err((this, e));
        }
        // next stages here

        Ok(this)
    }
}
