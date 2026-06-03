use std::borrow::Cow;
use std::iter::repeat_with;
use std::ops::Deref;

use crate::ast::{
    ConstValue, Expression, Literal, Number, NumberValue, Operator, Span, SpanIndex, UnaryOp, Value,
};
use crate::const_stage::Context;
use crate::const_stage::types::AnyTypeKey;
use crate::const_stage::{Diagnostic, Error, Errors, types::ModuleKey};

impl Expression {
    pub fn const_reduce<'a>(&'a self) -> Cow<'a, Expression> {
        match self {
            Expression::Binary {
                l: left,
                r: right,
                op,
            } => {
                let l = left.const_reduce();
                let r = right.const_reduce();
                match (Cow::as_ref(&l), Cow::as_ref(&r)) {
                    (Expression::Value(l), Expression::Value(r)) => {
                        match (
                            l.literal.inner.as_ref(),
                            r.literal.inner.as_ref(),
                            l.postfix.len() + r.postfix.len() == 0,
                        ) {
                            (Literal::Number(l), Literal::Number(r), true) => {
                                match op.const_apply_numeric(&l.value, &r.value) {
                                    Some(value) => {
                                        let v = Value {
                                            literal: Span::new(
                                                Literal::Number(Number { value, size: None }),
                                                op.location,
                                            ),
                                            unary: Vec::with_capacity(0),
                                            postfix: Vec::with_capacity(0),
                                        };
                                        Cow::Owned(Expression::Value(Span::new(v, op.location)))
                                    }
                                    None => Cow::Borrowed(self),
                                }
                            }
                            _ => Cow::Borrowed(self),
                        }
                    }
                    _ => Cow::Borrowed(self),
                }
            }
            Expression::Value(v) => {
                let new = match self.apply_unary(v) {
                    Ok(value) => value,
                    Err(value) => return value,
                };
                return Cow::Owned(Expression::Value(Span::new(
                    Value {
                        literal: Span::new(new, v.location),
                        postfix: Vec::with_capacity(0),
                        unary: Vec::with_capacity(0),
                    },
                    v.location,
                )));
            }
        }
    }

    fn apply_unary(&self, v: &Span<Value>) -> Result<Literal, Cow<'_, Expression>> {
        if v.unary.is_empty() {
            return Err(Cow::Borrowed(self));
        }
        let mut new = v.inner.as_ref().literal.inner.as_ref().clone();
        for op in v.unary.iter().rev() {
            let replace = match (op.inner.as_ref(), new) {
                (UnaryOp::Neg, _) => todo!("bool"),
                (
                    UnaryOp::Sub,
                    Literal::Number(Number {
                        value: NumberValue::Any(n),
                        size,
                    }),
                ) => Literal::Number(Number {
                    value: NumberValue::Int(-(n as i128)),
                    size,
                }),
                (
                    UnaryOp::Sub,
                    Literal::Number(Number {
                        value: NumberValue::Int(n),
                        size,
                    }),
                ) => Literal::Number(Number {
                    value: NumberValue::Int(-(n as i128)),
                    size,
                }),
                (
                    UnaryOp::Sub,
                    Literal::Number(Number {
                        value: NumberValue::Float(n),
                        size,
                    }),
                ) => Literal::Number(Number {
                    value: NumberValue::Float(-n),
                    size,
                }),
                _ => todo!("nah"),
            };
            new = replace;
        }
        Ok(new)
    }
}

impl ConstValue {
    pub fn implicit_cast(
        &self,
        ctx: &mut Context,
        target: AnyTypeKey,
    ) -> Result<ConstValue, Errors> {
        let typeof_self = self.type_of();
        let type_check_err = match typeof_self.check(&ctx.types, &target) {
            Ok(_) => return Ok(self.clone()),
            Err(e) => e,
        };
        Ok(match (self, target) {
            (ConstValue::Structure { fields, ty }, target) => Err(Errors::FailedImplicitCast {
                from: typeof_self,
                to: target,
            })?,
            (ConstValue::Number(Number { value, size }), AnyTypeKey::Primitive(ty)) => {
                match ty.default() {
                    ConstValue::Number(Number {
                        value: default,
                        size,
                    }) => ConstValue::Number(Number {
                        value: value.implicit_cast(&default).ok_or(type_check_err)?,
                        size,
                    }),
                    _ => Err(type_check_err)?,
                }
            }
            (ConstValue::EnumVariant { parent, variant }, target) => {
                match parent.unwrap_full(&ctx.types) {
                    AnyTypeKey::Enum(key) => {
                        match ctx
                            .types
                            .enums
                            .get_unchecked(&key)
                            .variants
                            .iter()
                            .find(|(ident, _)| ident == variant)
                            .cloned()
                        {
                            Some((_, actual_value)) => actual_value.implicit_cast(ctx, target)?,
                            None => unreachable!("variant {variant} not found"),
                        }
                    }
                    _ => unreachable!(
                        "concretized type '{}' must be enum",
                        parent.stringify(&ctx.types)
                    ),
                }
            }
            (ConstValue::String(smol_str), target) => Err(type_check_err)?,
            (ConstValue::Char(_), target) => Err(type_check_err)?,
            (ConstValue::Bool(_), target) => Err(type_check_err)?,
            (ConstValue::Array { elements, ty }, target) => Err(type_check_err)?,
            (ConstValue::Tuple { elements, ty }, target) => Err(type_check_err)?,
            (val, ty) => Err(type_check_err)?,
        })
    }
}

impl NumberValue {
    pub fn implicit_cast(&self, casts_to: &Self) -> Option<Self> {
        Some(match (self.clone(), casts_to) {
            (Self::Any(n), Self::Int(_)) => Self::Int(n as _),
            (Self::Any(n), Self::Uint(_)) => Self::Uint(n as _),
            (Self::Any(n), Self::Float(_)) => Self::Float(n as _),

            (Self::Int(n), Self::Any(_)) => Self::Any(n as _),
            (Self::Int(n), Self::Uint(_)) => Self::Uint(n as _),
            (Self::Int(n), Self::Float(_)) => Self::Float(n as _),

            (Self::Uint(n), Self::Int(_)) => Self::Int(n as _),
            (Self::Uint(n), Self::Any(_)) => Self::Any(n as _),
            (Self::Uint(n), Self::Float(_)) => Self::Float(n as _),

            (Self::Uint(_), Self::Uint(_)) => self.clone(),
            (Self::Any(_), Self::Any(_)) => self.clone(),
            (Self::Float(_), Self::Float(_)) => self.clone(),
            (Self::Int(_), Self::Int(_)) => self.clone(),
            _ => None?,
        })
    }
}

impl AnyTypeKey {
    pub fn const_default(&self, ctx: &mut Context) -> Result<ConstValue, Errors> {
        match self {
            AnyTypeKey::Primitive(primitive_type) => Ok(primitive_type.default()),
            AnyTypeKey::Array(key) => {
                let this = ctx.types.arrays.get_unchecked(key);
                let elem_type = this.element_type;
                let size = this.size.ok_or(Errors::UndefinedDefault(elem_type))?;
                let val = elem_type.const_default(ctx)?;
                let value = ConstValue::Array {
                    elements: repeat_with(|| Span::new(val.clone(), SpanIndex::default()))
                        .take(size)
                        .collect(),
                    ty: *self,
                };
                Ok(value)
            }
            AnyTypeKey::Tuple(key) => {
                let this = ctx.types.tuples.get_unchecked(key);
                let mut elements = Vec::with_capacity(this.parameters.len());
                for elem in this.parameters.clone() {
                    elements.push(Span::new(elem.const_default(ctx)?, SpanIndex::default()));
                }
                let value = ConstValue::Tuple {
                    elements,
                    ty: *self,
                };
                Ok(value)
            }
            AnyTypeKey::Struct(key) => {
                let this = ctx.types.structures.get_unchecked(key);
                let mut fields = Vec::with_capacity(this.parameters.len());
                let sp_idx = SpanIndex::default();
                for (ident, ty, default) in this.parameters.clone() {
                    let val = match default {
                        Some(val) => val,
                        None => ty.const_default(ctx)?,
                    };
                    fields.push(Span::new(
                        (Span::new(ident, sp_idx), Span::new(val, sp_idx)),
                        sp_idx,
                    ));
                }
                let value = ConstValue::Structure { fields, ty: *self };
                Ok(value)
            }
            AnyTypeKey::Named(key) => ctx
                .types
                .named
                .get_unchecked(key)
                .repr
                .clone()
                .const_default(ctx),
            AnyTypeKey::Morphed(key) => {
                let this = ctx.types.morphs.get_unchecked(key);
                this.this.clone().const_default(ctx)
            }
            AnyTypeKey::Trait(_) => Err(Errors::UndefinedDefault(*self)),
            AnyTypeKey::Enum(_) => Err(Errors::UndefinedDefault(*self)),
            AnyTypeKey::Reference(_) => Err(Errors::UndefinedDefault(*self)),
            AnyTypeKey::ModuleRef(_) => Err(Errors::UndefinedDefault(*self)),
            AnyTypeKey::Function(_) => Err(Errors::UndefinedDefault(*self)),
            // AnyTypeKey::Constraint(_) => Err(Errors::UndefinedDefault(*self)),
            AnyTypeKey::AnonymousStruct => Err(Errors::UndefinedDefault(*self)),
            AnyTypeKey::Polymorph(_) => Err(Errors::UndefinedDefault(*self)),
            AnyTypeKey::Generic(_) => Err(Errors::UndefinedDefault(*self)),
        }
    }
}

impl Span<Operator> {
    pub fn const_apply(
        &self,
        left: &ConstValue,
        right: &ConstValue,
        module: ModuleKey,
    ) -> Result<ConstValue, Error> {
        use Operator::*;

        match self.inner.as_ref() {
            Add | Sub | Mul | Div | Mod => match (left, right) {
                (ConstValue::Number(l), ConstValue::Number(r)) => {
                    match self.const_apply_numeric(&l.value, &r.value) {
                        Some(value) => Ok(ConstValue::Number(Number { value, size: None })),
                        None => Err(Diagnostic {
                            span: self.location,
                            module,
                            inner: Errors::CanNotApplyConst {
                                op: self.inner.as_ref().clone(),
                                left: left.clone(),
                                right: right.clone(),
                            },
                        }),
                    }
                }
                _ => Err(Diagnostic {
                    span: self.location,
                    module,
                    inner: Errors::CanNotApplyConst {
                        op: self.inner.as_ref().clone(),
                        left: left.clone(),
                        right: right.clone(),
                    },
                }),
            },
            Eq | NEq | Gr | Le | GrEq | LeEq => match (left, right) {
                (ConstValue::Number(l), ConstValue::Number(r)) => {
                    match self.const_apply_numeric_to_bool(&l.value, &r.value) {
                        Some(value) => Ok(ConstValue::Bool(value)),
                        None => Err(Diagnostic {
                            span: self.location,
                            module,
                            inner: Errors::CanNotApplyConst {
                                op: self.inner.as_ref().clone(),
                                left: left.clone(),
                                right: right.clone(),
                            },
                        }),
                    }
                }
                _ => Err(Diagnostic {
                    span: self.location,
                    module,
                    inner: Errors::CanNotApplyConst {
                        op: self.inner.as_ref().clone(),
                        left: left.clone(),
                        right: right.clone(),
                    },
                }),
            },
            And | Or => todo!(),
            Assign | AddAssign | SubAssign | MulAssign | DivAssign | ModAssign => Err(Diagnostic {
                span: self.location,
                module,
                inner: Errors::CanNotApplyConst {
                    op: self.inner.as_ref().clone(),
                    left: left.clone(),
                    right: right.clone(),
                },
            }),
        }
    }

    fn _const_apply_bool(&self, _l: bool, _r: bool) -> Option<bool> {
        todo!()
    }

    fn const_apply_numeric_to_bool(&self, l: &NumberValue, r: &NumberValue) -> Option<bool> {
        use NumberValue::*;
        use Operator::*;
        Some(match (self.inner.as_ref(), l, r) {
            (Eq, Float(l), Float(r)) => l == r,
            (Eq, Any(l), Any(r)) => l == r,
            (Eq, Int(l), Int(r)) => l == r,
            (Eq, Uint(l), Uint(r)) => l == r,

            (NEq, Float(l), Float(r)) => l != r,
            (NEq, Any(l), Any(r)) => l != r,
            (NEq, Int(l), Int(r)) => l != r,
            (NEq, Uint(l), Uint(r)) => l != r,

            (Le, Float(l), Float(r)) => l < r,
            (Le, Any(l), Any(r)) => l < r,
            (Le, Int(l), Int(r)) => l < r,
            (Le, Uint(l), Uint(r)) => l < r,

            (LeEq, Float(l), Float(r)) => l <= r,
            (LeEq, Any(l), Any(r)) => l <= r,
            (LeEq, Int(l), Int(r)) => l <= r,
            (LeEq, Uint(l), Uint(r)) => l <= r,

            (Gr, Float(l), Float(r)) => l > r,
            (Gr, Any(l), Any(r)) => l > r,
            (Gr, Int(l), Int(r)) => l > r,
            (Gr, Uint(l), Uint(r)) => l > r,

            (GrEq, Float(l), Float(r)) => l >= r,
            (GrEq, Any(l), Any(r)) => l >= r,
            (GrEq, Int(l), Int(r)) => l >= r,
            (GrEq, Uint(l), Uint(r)) => l >= r,
            _ => return None,
        })
    }

    fn const_apply_numeric(&self, l: &NumberValue, r: &NumberValue) -> Option<NumberValue> {
        use NumberValue::*;
        use Operator::*;
        Some(match (self.inner.as_ref(), l, r.implicit_cast(l)?) {
            (Add, Float(l), Float(r)) => Float(l + r),
            (Add, Any(l), Any(r)) => Any(l + r),
            (Add, Int(l), Int(r)) => Int(l + r),
            (Add, Uint(l), Uint(r)) => Uint(l + r),

            (Sub, Float(l), Float(r)) => Float(l - r),
            (Sub, Any(l), Any(r)) => Any(l - r),
            (Sub, Int(l), Int(r)) => Int(l - r),
            (Sub, Uint(l), Uint(r)) => Uint(l - r),

            (Mul, Float(l), Float(r)) => Float(l * r),
            (Mul, Any(l), Any(r)) => Any(l * r),
            (Mul, Int(l), Int(r)) => Int(l * r),
            (Mul, Uint(l), Uint(r)) => Uint(l * r),

            (Div, Float(l), Float(r)) => Float(l / r),
            (Div, Any(l), Any(r)) => match (*l).checked_div(r) {
                Some(v) => Any(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },
            (Div, Int(l), Int(r)) => match (*l).checked_div(r) {
                Some(v) => Int(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },
            (Div, Uint(l), Uint(r)) => match (*l).checked_div(r) {
                Some(v) => Uint(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },

            (Mod, Float(l), Float(r)) => Float(l % r),
            (Mod, Any(l), Any(r)) => match (*l).checked_rem_euclid(r) {
                Some(v) => Any(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },
            (Mod, Int(l), Int(r)) => match (*l).checked_rem_euclid(r) {
                Some(v) => Int(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },
            (Mod, Uint(l), Uint(r)) => match (*l).checked_rem_euclid(r) {
                Some(v) => Uint(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },
            _ => return None,
        })
    }
}
