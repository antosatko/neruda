use std::borrow::Cow;

use crate::{
    Diagnostics, Expression, Literal, LoweringDiagnostic, LoweringWarning, Number, NumberValue,
    Operator, Span, Value,
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
                            &l.literal.inner,
                            &r.literal.inner,
                            l.postfix.len() + r.postfix.len() == 0,
                        ) {
                            (Literal::Number(l), Literal::Number(r), true) => {
                                match op.apply(&l.value, &r.value, diagnostics) {
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
                                        Cow::Owned(Expression::Value(v))
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
    pub fn apply(
        &self,
        l: &NumberValue,
        r: &NumberValue,
        diagnostics: &mut Diagnostics,
    ) -> Option<NumberValue> {
        use NumberValue::*;
        use Operator::*;

        Some(match (self.inner, l, r) {
            (Add, Float(l), Float(r)) => Float(l + r),
            (Add, Number(l), Number(r)) => Number(l + r),
            (Add, Int(l), Int(r)) => Int(l + r),
            (Add, Uint(l), Uint(r)) => Uint(l + r),

            (Sub, Float(l), Float(r)) => Float(l - r),
            (Sub, Number(l), Number(r)) => Number(l - r),
            (Sub, Int(l), Int(r)) => Int(l - r),
            (Sub, Uint(l), Uint(r)) => Uint(l - r),

            (Mul, Float(l), Float(r)) => Float(l * r),
            (Mul, Number(l), Number(r)) => Number(l * r),
            (Mul, Int(l), Int(r)) => Int(l * r),
            (Mul, Uint(l), Uint(r)) => Uint(l * r),

            (Div, Float(l), Float(r)) => Float(l / r),
            (Div, Number(l), Number(r)) => match (*l).checked_div(*r) {
                Some(v) => Number(v),
                None => {
                    diagnostics
                        .warns
                        .push(Span::new(LoweringWarning::DivisionByZero, self.location));
                    return None;
                }
            },
            (Div, Int(l), Int(r)) => match (*l).checked_div(*r) {
                Some(v) => Int(v),
                None => {
                    diagnostics
                        .warns
                        .push(Span::new(LoweringWarning::DivisionByZero, self.location));
                    return None;
                }
            },
            (Div, Uint(l), Uint(r)) => match (*l).checked_div(*r) {
                Some(v) => Uint(v),
                None => {
                    diagnostics
                        .warns
                        .push(Span::new(LoweringWarning::DivisionByZero, self.location));
                    return None;
                }
            },

            (Mod, Float(l), Float(r)) => Float(l % r),
            (Mod, Number(l), Number(r)) => match (*l).checked_rem_euclid(*r) {
                Some(v) => Number(v),
                None => {
                    diagnostics
                        .warns
                        .push(Span::new(LoweringWarning::DivisionByZero, self.location));
                    return None;
                }
            },
            (Mod, Int(l), Int(r)) => match (*l).checked_rem_euclid(*r) {
                Some(v) => Int(v),
                None => {
                    diagnostics
                        .warns
                        .push(Span::new(LoweringWarning::DivisionByZero, self.location));
                    return None;
                }
            },
            (Mod, Uint(l), Uint(r)) => match (*l).checked_rem_euclid(*r) {
                Some(v) => Uint(v),
                None => {
                    diagnostics
                        .warns
                        .push(Span::new(LoweringWarning::DivisionByZero, self.location));
                    return None;
                }
            },
            _ => return None,
        })
    }
}
