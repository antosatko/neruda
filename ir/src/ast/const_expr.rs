use std::borrow::Cow;

use crate::{
    ast::{
        ConstValue, Diagnostics, Expression, Literal, LoweringDiagnostic, Number, NumberValue,
        Operator, Span, SpanIndex, Value,
    },
    ir::{Diagnostic, Error, Errors, types::ModuleKey},
};

impl Expression {
    pub fn const_reduce<'a>(&'a self, diagnostics: &mut Diagnostics) -> Cow<'a, Expression> {
        match self {
            Expression::Binary {
                l: left,
                r: right,
                op,
            } => {
                let l = left.const_reduce(diagnostics);
                let r = right.const_reduce(diagnostics);
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
                                            postfix: Vec::with_capacity(0),
                                        };
                                        diagnostics.diagnostics.push(Span::new(
                                            LoweringDiagnostic::ReducedConstExpr(v.clone()),
                                            v.literal.location,
                                        ));
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
            Expression::Value(_) => Cow::Borrowed(self),
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
        Some(match (self.inner.as_ref(), l, r) {
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
            (Div, Any(l), Any(r)) => match (*l).checked_div(*r) {
                Some(v) => Any(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },
            (Div, Int(l), Int(r)) => match (*l).checked_div(*r) {
                Some(v) => Int(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },
            (Div, Uint(l), Uint(r)) => match (*l).checked_div(*r) {
                Some(v) => Uint(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },

            (Mod, Float(l), Float(r)) => Float(l % r),
            (Mod, Any(l), Any(r)) => match (*l).checked_rem_euclid(*r) {
                Some(v) => Any(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },
            (Mod, Int(l), Int(r)) => match (*l).checked_rem_euclid(*r) {
                Some(v) => Int(v),
                None => {
                    /*diagnostics
                    .warns
                    .push(Span::new(LoweringWarning::DivisionByZero, self.location));*/
                    return None;
                }
            },
            (Mod, Uint(l), Uint(r)) => match (*l).checked_rem_euclid(*r) {
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
