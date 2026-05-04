use std::{borrow::Cow, path::PathBuf, sync::Arc};

use arena::Arena;
use ruparse::{
    lexer::{Token, TokenKinds},
    parser::{Node, Nodes},
};
use smol_str::SmolStr;

use ir::ast::{
    ActionClause, Alias, Associativity, Body, Clauses, Diagnostics, Else, ElseIf, ExprItem,
    Expression, Function, GenericParameter, IdentifierPath, Keyword, Literal, LoweringError,
    LoweringWarning, Module, Mutability, Object, Operator, Parameter, RestrictionClause,
    SelectClause, Span, SpanIndex, Statement, SystemInclusion, Type, TypeLiteral, Value,
    char_literal, numeric_literal, string_literal,
};

#[derive(Debug, Clone)]
pub struct ModuleOk {
    pub module: Module,
    pub diagnostics: Diagnostics,
}

pub fn module_named(
    name: impl Into<SmolStr>,
    src: &str,
    node: Node,
    path: Option<PathBuf>,
) -> Result<ModuleOk, Span<LoweringError>> {
    let mut diagnostics = Diagnostics::default();
    let node = &Nodes::Node(node);
    let mut module = Module {
        name: name.into(),
        path,
        docs: docstrings(src, node),
        objects: Arena::new(),
        src: Arc::new(src.into()),
    };

    for s in node.get_list("top level statements") {
        match s.get_name() {
            "scheduler" => {
                let ident = expect_ident(src, s, &mut diagnostics);
                let docs = docstrings(src, s);

                let mut resources = None;
                let mut systems = None;
                let mut init = None;

                if let Some(res) = s.try_get_node("resources").as_ref() {
                    let mut resources_vec = Vec::new();
                    for r in res.get_list("resources") {
                        resources_vec.push(value(src, r, &mut diagnostics)?);
                    }
                    resources = Some(span(resources_vec, res));
                }

                if let Some(sys) = s.try_get_node("systems").as_ref() {
                    let mut systems_vec = Vec::new();
                    for s in sys.get_list("systems") {
                        let path = ident_path(src, s.expect_node("identifier"));
                        let generics =
                            generic_arguments(src, s.try_get_node("generics"), &mut diagnostics)?;
                        systems_vec.push(span(SystemInclusion { path, generics }, s));
                    }
                    systems = Some(span(systems_vec, sys));
                }

                if let Some(init_node) = s.try_get_node("initialization").as_ref() {
                    let body = body(src, init_node.expect_node("body"), &mut diagnostics)?;
                    init = Some((body, Keyword(span((), init_node))));
                }

                let obj = Object::Scheduler {
                    ident: ident.clone(),
                    resources,
                    systems,
                    init,
                    docs,
                };

                module.objects.push(span(obj, s));
            }

            "function" => {
                let obj = function(src, s, &mut diagnostics)?;
                module.objects.push(span(Object::Function(obj), s));
            }

            "system" => {
                let ident = expect_ident(src, s, &mut diagnostics);
                let sys_body = body(src, s.expect_node("main body"), &mut diagnostics)?;
                let docs = docstrings(src, s);
                let mut query = Vec::new();
                for clause in s.expect_node("query").get_list("clauses") {
                    query.push(clause_variant(src, clause, &mut diagnostics)?);
                }
                let before = match s.try_get_node("before body").as_ref() {
                    Some(before) => Some(span(
                        body(src, before.expect_node("body"), &mut diagnostics)?,
                        before,
                    )),
                    None => None,
                };
                let after = match s.try_get_node("after body").as_ref() {
                    Some(after) => Some(span(
                        body(src, after.expect_node("body"), &mut diagnostics)?,
                        after,
                    )),
                    None => None,
                };
                let generics =
                    generic_params(src, s.try_get_node("generic parameters"), &mut diagnostics);

                let obj = Object::System {
                    ident: ident.clone(),
                    query,
                    docs,
                    body: sys_body,
                    before,
                    after,
                    generics,
                };

                module.objects.push(span(obj, s));
            }

            "component" => {
                let ident = expect_ident(src, s, &mut diagnostics);
                let docs = docstrings(src, s);
                let ty = match s.try_get_node("type") {
                    Some(t) => Some(ty(src, t, &mut diagnostics)?),
                    None => None,
                };

                let obj = Object::Component {
                    ident: ident.clone(),
                    ty,
                    docs,
                };

                module.objects.push(span(obj, s));
            }

            "type definition" => {
                let ident = expect_ident(src, s, &mut diagnostics);
                let docs = docstrings(src, s);
                let ty = match s.try_get_node("type") {
                    Some(t) => Some(ty(src, t, &mut diagnostics)?),
                    None => None,
                };
                let generics =
                    generic_params(src, s.try_get_node("generic parameters"), &mut diagnostics);

                let obj = Object::Type {
                    ident,
                    generics,
                    ty,
                    docs,
                };

                module.objects.push(span(obj, s));
            }

            "import" => {
                let ident = ident_path(src, s.expect_node("identifier"));
                let alias = alias(src, s.try_get_node("alias"), &mut diagnostics);
                let obj = Object::Import { ident, alias };
                module.objects.push(span(obj, s));
            }

            "const" => {
                let ident = expect_ident(src, s.expect_node("identifier"), &mut diagnostics);
                let ty = ty(src, s.expect_node("type"), &mut diagnostics)?;
                let docs = docstrings(src, s);
                let expression = expression(src, s.expect_node("expression"), &mut diagnostics)?;
                let obj = Object::Const {
                    docs,
                    ident,
                    ty,
                    expression,
                };
                module.objects.push(span(obj, s));
            }

            "trait" => {
                let ident = expect_ident(src, s.expect_node("identifier"), &mut diagnostics);
                let docs = docstrings(src, s);
                let mut methods = Vec::new();
                for fun in s.get_list("methods") {
                    methods.push(span(function(src, fun, &mut diagnostics)?, fun));
                }

                let obj = Object::Trait {
                    docs,
                    ident,
                    methods,
                };
                module.objects.push(span(obj, s));
            }

            "impl" => {
                let mut methods = Vec::new();
                for fun in s.get_list("methods") {
                    methods.push(span(function(src, fun, &mut diagnostics)?, fun));
                }
                let generic_parameters =
                    generic_params(src, s.try_get_node("generic parameters"), &mut diagnostics);

                let variant = s.expect_node("type");
                let obj = match variant.get_name() {
                    "trait implementation" => {
                        let ty = ty(src, variant.expect_node("type"), &mut diagnostics)?;
                        let trt = ident_path(src, variant.expect_node("trait"));
                        let for_kw = Keyword(span((), variant.expect_node("kw")));
                        Object::TraitImpl {
                            ty,
                            trt,
                            for_kw,
                            generic_parameters,
                            methods,
                        }
                    }
                    "type implementation" => {
                        let ty = ty(src, variant.expect_node("type"), &mut diagnostics)?;
                        Object::TypeImpl {
                            ty,
                            generic_parameters,
                            methods,
                        }
                    }
                    name => s.ice(&format!("This trait implementation does not exist: {name}")),
                };
                module.objects.push(span(obj, s));
            }

            other => s.ice(&format!("Unhandled top-level item: {}", other)),
        }
    }

    Ok(ModuleOk {
        module,
        diagnostics,
    })
}

fn function(
    src: &str,
    s: &Nodes<'_>,
    diagnostics: &mut Diagnostics,
) -> Result<Function, Span<LoweringError>> {
    let ident = expect_ident(src, s, diagnostics);
    let params = parameters(src, s.expect_node("parameters"), diagnostics)?;
    let return_type = match s.try_get_node("return type") {
        Some(t) => Some(ty(src, t, diagnostics)?),
        None => None,
    };
    let body = body(src, s.expect_node("code body"), diagnostics)?;
    let docs = docstrings(src, s);
    let generics = generic_params(src, s.try_get_node("generic parameters"), diagnostics);
    let obj = Function {
        ident: ident.clone(),
        parameters: params,
        return_type,
        body,
        docs,
        generics,
    };
    Ok(obj)
}

fn generic_params(
    src: &str,
    node: &Option<Nodes<'_>>,
    diagnostics: &mut Diagnostics,
) -> Option<Span<Vec<Span<GenericParameter>>>> {
    let node = match node {
        Some(node) => node,
        None => return None,
    };
    let mut params = Vec::new();
    for param in node.get_list("parameters") {
        let identifier = expect_ident(src, param, diagnostics);
        let constraints = param
            .get_list("constraints")
            .iter()
            .map(|c| ident_path(src, c))
            .collect();
        params.push(span(
            GenericParameter {
                identifier,
                constraints,
            },
            param,
        ));
    }
    Some(span(params, node))
}

fn clause_variant(
    src: &str,
    clause: &Nodes<'_>,
    diagnostics: &mut Diagnostics,
) -> Result<Span<Clauses>, Span<LoweringError>> {
    match clause.get_name() {
        "select" => {
            let ident = expect_ident(src, clause, diagnostics);
            let docs = docstrings(src, clause);
            let mut include = Vec::new();
            let mut exclude = Vec::new();
            let mut optional = Vec::new();
            let foreign = clause
                .try_get_node("foreign")
                .as_ref()
                .map(|n| Keyword(span((), &n)));
            for component in clause.get_list("components") {
                let mutable = component
                    .try_get_node("mutable")
                    .as_ref()
                    .map(|m| span((), m));
                let component_path = ident_path(src, component.expect_node("component"));
                let alias = alias(src, component.try_get_node("alias"), diagnostics);
                match component.try_get_node("modifier") {
                    Some(Nodes::Token(Token {
                        kind: TokenKinds::Token("?"),
                        ..
                    })) => optional.push((component_path, Mutability(mutable), alias)),
                    Some(Nodes::Token(Token {
                        kind: TokenKinds::Token("!"),
                        ..
                    })) => {
                        if mutable.is_none() {
                            exclude.push((component_path, alias))
                        } else {
                            return Err(span(LoweringError::MutableExclusion, component));
                        }
                    }
                    _ => include.push((component_path, Mutability(mutable), alias)),
                }
            }
            Ok(span(
                Clauses::Select(SelectClause {
                    foreign,
                    ident,
                    docs,
                    include,
                    exclude,
                    optional,
                }),
                clause,
            ))
        }
        "action" => {
            let ident = expect_ident(src, clause, diagnostics);
            let docs = docstrings(src, clause);
            let event = clause
                .get_list("event")
                .iter()
                .map(|a| {
                    (
                        ident_path(src, a.expect_node("identifier")),
                        alias(src, a.try_get_node("alias"), diagnostics),
                    )
                })
                .collect();
            let keyword = span((), clause);
            Ok(span(
                Clauses::Action((ActionClause { ident, docs, event }, Keyword(keyword))),
                clause,
            ))
        }
        "restriction" => {
            let expression = expression(src, clause.expect_node("expression"), diagnostics)?;
            Ok(span(
                Clauses::Restriction(RestrictionClause { expression }),
                clause,
            ))
        }
        name => unreachable!("clause {name} unknown"),
    }
}

fn alias(src: &str, node: &Option<Nodes>, diagnostics: &mut Diagnostics) -> Alias {
    Alias(
        node.as_ref()
            .map(|n| span(expect_ident(src, n, diagnostics), n)),
    )
}

fn body(
    src: &str,
    node: &Nodes,
    diagnostics: &mut Diagnostics,
) -> Result<Span<Body>, Span<LoweringError>> {
    Ok(span(
        match node.get_name() {
            "code block" => Body::Block(block(src, node, diagnostics)?),
            "code statement" => Body::Statement(expression(
                src,
                node.expect_node("expression").expect_node("expression"),
                diagnostics,
            )?),
            other => unreachable!("expected code block or statement, got: {other}"),
        },
        node,
    ))
}

fn block(
    src: &str,
    node: &Nodes,
    diagnostics: &mut Diagnostics,
) -> Result<Vec<Span<Statement>>, Span<LoweringError>> {
    let mut statements = Vec::new();

    for stmt_node in node.get_list("statements") {
        let stmt = match stmt_node.get_name() {
            "variable" => {
                let ident = expect_ident(src, stmt_node.expect_node("identifier"), diagnostics);
                let ty = match stmt_node.try_get_node("type") {
                    Some(t) => Some(ty(src, t, diagnostics)?),
                    None => None,
                };
                let expression = match stmt_node.try_get_node("expression").as_ref() {
                    Some(e) => Some(expression(src, e, diagnostics)?),
                    None => None,
                };

                span(
                    Statement::Var {
                        ident,
                        ty,
                        expression,
                    },
                    stmt_node,
                )
            }

            "return" => {
                let expr = expression(src, stmt_node.expect_node("expression"), diagnostics);
                span(Statement::Return { expression: expr? }, stmt_node)
            }

            "loop" => {
                let label = try_label(src, stmt_node, diagnostics);
                let body = body(src, stmt_node.expect_node("code body"), diagnostics)?;
                span(Statement::Loop { label, body }, stmt_node)
            }

            "expression statement" => {
                let expr = expression(src, stmt_node.expect_node("expression"), diagnostics);
                span(Statement::Expr { expression: expr? }, stmt_node)
            }

            "break" => span(
                Statement::Break {
                    label: try_label(src, stmt_node, diagnostics),
                },
                stmt_node,
            ),

            "continue" => span(
                Statement::Continue {
                    label: try_label(src, stmt_node, diagnostics),
                },
                stmt_node,
            ),

            "if" => {
                let condition = expression(src, stmt_node.expect_node("expression"), diagnostics)?;
                let then_block = body(src, stmt_node.expect_node("code body"), diagnostics)?;

                let mut else_if = Vec::new();
                for elif in stmt_node.get_list("else if") {
                    let condition = expression(src, elif.expect_node("expression"), diagnostics)?;
                    let block = body(src, elif.expect_node("code body"), diagnostics)?;
                    else_if.push(span(ElseIf { condition, block }, elif));
                }

                let else_block = match stmt_node.try_get_node("else").as_ref() {
                    Some(e) => Some(span(
                        Else {
                            block: body(src, e.expect_node("code body"), diagnostics)?,
                        },
                        e,
                    )),
                    None => None,
                };

                span(
                    Statement::If {
                        condition,
                        then_block,
                        else_if,
                        else_block,
                    },
                    stmt_node,
                )
            }

            "while" => {
                let label = try_label(src, stmt_node, diagnostics);
                let condition = expression(src, stmt_node.expect_node("expression"), diagnostics)?;
                let body = body(src, stmt_node.expect_node("code body"), diagnostics)?;

                span(
                    Statement::While {
                        label,
                        condition,
                        body,
                    },
                    stmt_node,
                )
            }

            other => stmt_node.ice(&format!("Unhandled statement type: {}", other)),
        };

        statements.push(stmt);
    }

    Ok(statements)
}

#[track_caller]
fn try_label(src: &str, node: &Nodes, diagnostics: &mut Diagnostics) -> Option<Span<SmolStr>> {
    node.try_get_node("label")
        .as_ref()
        .map(|l| expect_ident(src, l, diagnostics))
}

fn expression(
    src: &str,
    node: &Nodes,
    diagnostics: &mut Diagnostics,
) -> Result<Span<Expression>, Span<LoweringError>> {
    let items = expression_items(src, node, diagnostics)?;
    let mut pos = 0;
    let expr = parse_expression_prec(&items, &mut pos, 0);
    match expr.const_reduce(diagnostics) {
        Cow::Owned(reduced_expr) => Ok(Span::new(reduced_expr, expr.location)),
        Cow::Borrowed(_) => Ok(expr),
    }
}

fn parse_expression_prec(
    items: &[Span<ExprItem>],
    pos: &mut usize,
    min_prec: u8,
) -> Span<Expression> {
    let mut lhs = match items[*pos].inner.as_ref() {
        ExprItem::Value(expr) => {
            let v = expr.clone();
            let s = items[*pos].clone().map(|_| v);
            *pos += 1;
            s
        }
        _ => unreachable!("expression must start with value"),
    };

    while *pos < items.len() {
        let op_item = &items[*pos];
        let op = match op_item.inner.as_ref() {
            ExprItem::Operator(op) => op,
            _ => break,
        };
        let spanned_op = op_item.clone().map(|_| *op);

        let prec = op.precedence();
        if prec < min_prec {
            break;
        }

        *pos += 1;
        let next_min_prec = match op.associativity() {
            Associativity::Left => prec + 1,
            Associativity::Right => prec,
        };

        let rhs = parse_expression_prec(items, pos, next_min_prec);

        let loc = lhs.location;
        lhs = Span::new(
            Expression::Binary {
                l: lhs,
                r: rhs,
                op: spanned_op,
            },
            loc,
        );
    }

    lhs
}

fn expression_items(
    src: &str,
    node: &Nodes,
    diagnostics: &mut Diagnostics,
) -> Result<Vec<Span<ExprItem>>, Span<LoweringError>> {
    let mut items = Vec::new();

    let lvalue_node = node.expect_node("lvalue");
    let lvalue = value(src, &lvalue_node, diagnostics)?;
    let loc = lvalue.location;
    items.push(Span::new(ExprItem::Value(Expression::Value(lvalue)), loc));

    for entry in node.get_list("rest") {
        match entry {
            Nodes::Token(_) => {
                let op = operator(entry);
                let loc = op.location;
                items.push(Span::new(ExprItem::Operator(*op), loc));
            }
            Nodes::Node(_) => {
                let v = value(src, entry, diagnostics)?;
                let loc = v.location;
                items.push(Span::new(ExprItem::Value(Expression::Value(v)), loc));
            }
        }
    }

    Ok(items)
}

fn operator(node: &Nodes) -> Span<Operator> {
    let token = node.expect_token();
    let kind = if let TokenKinds::Token(kind) = token.kind {
        kind
    } else {
        node.ice(&format!("Unknown operator: {:?}", token))
    };
    let op = match kind {
        "+" => Operator::Add,
        "-" => Operator::Sub,
        "*" => Operator::Mul,
        "/" => Operator::Div,
        "%" => Operator::Mod,
        "==" => Operator::Eq,
        "!=" => Operator::NEq,
        ">" => Operator::Gr,
        "<" => Operator::Le,
        ">=" => Operator::GrEq,
        "<=" => Operator::LeEq,
        "&&" => Operator::And,
        "||" => Operator::Or,
        "=" => Operator::Assign,
        "+=" => Operator::AddAssign,
        "-=" => Operator::SubAssign,
        "*=" => Operator::MulAssign,
        "/=" => Operator::DivAssign,
        "%=" => Operator::ModAssign,
        other => node.ice(&format!("Unknown operator: {}", other)),
    };

    span_from_token(op, token)
}

#[track_caller]
pub fn expect_ident(src: &str, node: &Nodes, diagnostics: &mut Diagnostics) -> Span<SmolStr> {
    let t = node.expect_node("identifier");
    let ident: Span<SmolStr> = match t {
        Nodes::Node(n) => span_from_node(t.stringify(src).into(), n),
        Nodes::Token(tok) => span_from_token(tok.stringify(src).into(), tok),
    };
    if ident.len() > 30 {
        diagnostics.warns.push(span(
            LoweringWarning::IdentifierTooLong(ident.to_string()),
            node,
        ));
    }
    ident
}

#[track_caller]
fn ident_path(src: &str, node: &Nodes) -> Span<IdentifierPath> {
    let path = node
        .get_list("path")
        .iter()
        .map(|p| span(p.stringify(src).into(), p))
        .collect();

    span(IdentifierPath { path }, node)
}

fn literal(
    src: &str,
    node: &Nodes,
    diagnostics: &mut Diagnostics,
) -> Result<Span<Literal>, Span<LoweringError>> {
    match node {
        Nodes::Node(n) => match n.name {
            "identifier path literal" => {
                let path = node
                    .try_get_node("identifier")
                    .as_ref()
                    .map(|i| ident_path(src, i));
                match node.try_get_node("struct literal").as_ref() {
                    Some(s) => {
                        let mut args = Vec::new();
                        for arg in s.get_list("arguments") {
                            let ident = expect_ident(src, arg, diagnostics);
                            let expr = expression(src, arg.expect_node("expression"), diagnostics)?;
                            args.push(span((ident, expr), arg));
                        }
                        let p = match path {
                            Some(p) => Ok(p),
                            None => Err(Keyword(span((), node))),
                        };
                        return Ok(span(Literal::Structure(p, args), s));
                    }
                    None => {
                        return Ok(match path {
                            Some(p) => {
                                let loc = p.location;
                                Span::new(Literal::Identifier(p), loc)
                            }
                            None => unreachable!(),
                        });
                    }
                }
            }

            "array literal" => {
                let mut elements = Vec::new();
                for e in n.get_list("elements") {
                    elements.push(expression(src, e, diagnostics)?)
                }

                Ok(span(Literal::Array(elements), node))
            }

            "tuple literal" => {
                let mut elements = Vec::new();
                for e in n.get_list("expressions") {
                    elements.push(expression(src, e, diagnostics)?)
                }

                Ok(span(Literal::Tuple(elements), node))
            }

            other => node.ice(&format!("Unhandled literal node: {}", other)),
        },

        Nodes::Token(tok) => match &tok.kind {
            ruparse::lexer::TokenKinds::Complex(kind) => match kind.as_ref() {
                "string" => Ok(span_from_token(
                    Literal::String(string_literal(&tok.stringify(src))),
                    tok,
                )),

                "char" => Ok(span_from_token(
                    Literal::Char(char_literal(&tok.stringify(src)).map_err(|e| span(e, node))?),
                    tok,
                )),

                "numeric" | "float" => Ok(span_from_token(
                    Literal::Number(
                        numeric_literal(&tok.stringify(src)).map_err(|e| span(e, node))?,
                    ),
                    tok,
                )),

                other => panic!("Unhandled token literal kind: {}", other),
            },

            _ => panic!("Unexpected token kind for literal: {:?}", tok.kind),
        },
    }
}

fn value(
    src: &str,
    node: &Nodes,
    diagnostics: &mut Diagnostics,
) -> Result<Span<Value>, Span<LoweringError>> {
    let literal = literal(src, node.expect_node("literal"), diagnostics)?;

    Ok(span(
        Value {
            literal,
            postfix: Vec::new(),
        },
        node,
    ))
}

fn generic_arguments(
    src: &str,
    node: &Option<Nodes>,
    diagnostics: &mut Diagnostics,
) -> Result<Option<Span<Vec<Span<Type>>>>, Span<LoweringError>> {
    match node {
        Some(generics) => {
            let mut parameters = Vec::new();
            for param in generics.get_list("parameters") {
                parameters.push(ty(src, param, diagnostics)?);
            }
            Ok(Some(span(parameters, generics)))
        }
        None => Ok(None),
    }
}

#[track_caller]
fn ty(
    src: &str,
    node: &Nodes,
    diagnostics: &mut Diagnostics,
) -> Result<Span<Type>, Span<LoweringError>> {
    let literal = node.expect_node("literal");
    let prefix = node.expect_node("prefix");
    let refs = span(
        prefix
            .get_list("prefix")
            .iter()
            .map(|n| match n.unwrap_token().kind {
                TokenKinds::Token("&") => 1,
                TokenKinds::Token("&&") => 2,
                _ => unreachable!(),
            })
            .sum(),
        prefix,
    );

    match literal.get_name() {
        "type path" => Ok(span(
            Type {
                literal: span(
                    TypeLiteral::Path(
                        ident_path(src, literal.expect_node("identifier")),
                        generic_arguments(src, literal.try_get_node("generics"), diagnostics)?,
                    ),
                    literal,
                ),
                refs,
            },
            node,
        )),
        "struct type literal" => Ok(span(
            Type {
                literal: span(
                    TypeLiteral::Struct(parameters(src, literal, diagnostics)?),
                    literal,
                ),
                refs,
            },
            node,
        )),
        "array type literal" => {
            let type_ = ty(src, literal.expect_node("type"), diagnostics)?;
            let len = match literal.try_get_node("length") {
                Some(expr) => Some(expression(src, expr, diagnostics)?),
                None => None,
            };
            Ok(span(
                Type {
                    literal: span(TypeLiteral::Array(Box::new(type_), len), literal),
                    refs,
                },
                node,
            ))
        }
        "tuple type literal" => {
            let mut params = Vec::new();
            for param in literal.get_list("parameters") {
                params.push(ty(src, param, diagnostics)?);
            }
            Ok(span(
                Type {
                    literal: span(TypeLiteral::Tuple(params), literal),
                    refs,
                },
                node,
            ))
        }
        "enum type literal" => {
            let mut variants = Vec::new();
            for variant in literal.get_list("variants") {
                let ident = expect_ident(src, variant, diagnostics);
                let expr = match variant.try_get_node("expression") {
                    Some(expr) => Some(expression(src, expr, diagnostics)?),
                    None => None,
                };
                variants.push((ident, expr));
            }
            let repr = match literal.try_get_node("representation") {
                Some(repr) => Some(Box::new(ty(src, repr, diagnostics)?)),
                None => None,
            };
            Ok(span(
                Type {
                    literal: span(TypeLiteral::Enum(repr, variants), literal),
                    refs,
                },
                node,
            ))
        }
        name => panic!("{name}"),
    }
}

fn parameter(
    src: &str,
    node: &Nodes,
    diagnostics: &mut Diagnostics,
) -> Result<Span<Parameter>, Span<LoweringError>> {
    let ident = expect_ident(src, node.expect_node("identifier"), diagnostics);
    let ty = ty(src, node.expect_node("type"), diagnostics)?;
    let docs = docstrings(src, node);
    Ok(span(Parameter { ident, ty, docs }, node))
}

fn parameters(
    src: &str,
    node: &Nodes,
    diagnostics: &mut Diagnostics,
) -> Result<Vec<Span<Parameter>>, Span<LoweringError>> {
    node.get_list("parameters")
        .iter()
        .map(|p| parameter(src, p, diagnostics))
        .collect()
}

#[track_caller]
fn docstrings(src: &str, node: &Nodes) -> Vec<Span<SmolStr>> {
    node.expect_node("docs")
        .get_list("docstr")
        .iter()
        .map(|d| span(d.stringify(src).into(), d))
        .collect()
}

pub fn span<T>(v: T, node: &Nodes) -> Span<T> {
    let index = node.str_idx();
    let len = node.str_last_idx() - index;
    Span::new(v, SpanIndex { index, len })
}
pub fn span_from_node<T>(v: T, node: &Node) -> Span<T> {
    let index = node.location.index;
    let len = node.location.len;
    Span::new(v, SpanIndex { index, len })
}
pub fn span_from_token<T>(v: T, token: &Token) -> Span<T> {
    let index = token.location.index;
    let len = token.location.len;
    Span::new(v, SpanIndex { index, len })
}
