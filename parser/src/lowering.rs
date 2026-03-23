use std::collections::HashMap;

use arena::Arena;
use ruparse::{
    lexer::{Token, TokenKinds},
    parser::{Node, Nodes},
};
use smol_str::SmolStr;

use ir::{
    ActionClause, Alias, Associativity, Body, Clauses, Else, ElseIf, ExprItem, Expression,
    Function, IdentifierPath, Keyword, Literal, LoweringError, Module, Mutability, Object,
    Operator, Parameter, RestrictionClause, SelectClause, Span, SpanIndex, Statement, Type,
    TypeLiteral, Value, char_literal, numeric_literal, string_literal,
};

pub fn module_named(
    name: impl Into<SmolStr>,
    src: &str,
    node: Node,
) -> Result<Module, LoweringError> {
    let node = &Nodes::Node(node);
    let mut module = Module {
        name: name.into(),
        docs: docstrings(src, node),
        objects: Arena::new(),
        symbols: HashMap::new(),
    };

    for s in node.get_list("top level statements") {
        match s.get_name() {
            "scheduler" => {
                let ident = expect_ident(src, s);
                let docs = docstrings(src, s);

                let mut resources = None;
                let mut systems = None;
                let mut init = None;

                if let Some(res) = s.try_get_node("resources").as_ref() {
                    let mut resources_vec = Vec::new();
                    for r in res.get_list("resources") {
                        resources_vec.push(value(src, r)?);
                    }
                    resources = Some(span(resources_vec, res));
                }

                if let Some(sys) = s.try_get_node("systems").as_ref() {
                    let mut systems_vec = Vec::new();
                    for p in sys.get_list("systems") {
                        systems_vec.push(ident_path(src, p));
                    }
                    systems = Some(span(systems_vec, sys));
                }

                if let Some(init_node) = s.try_get_node("initialization").as_ref() {
                    let body = body(src, init_node.expect_node("body"))?;
                    init = Some((body, Keyword(span((), init_node))));
                }

                let obj = Object::Scheduler {
                    ident: ident.clone(),
                    resources,
                    systems,
                    init,
                    docs,
                };

                let key = module.objects.push(span(obj, s));
                module.symbols.insert(ident.inner, key);
            }

            "function" => {
                let ident = expect_ident(src, s);
                let params = parameters(src, s.expect_node("parameters"));
                let return_type = s.try_get_node("return type").as_ref().map(|t| ty(src, t));
                let body = body(src, s.expect_node("code body"))?;
                let docs = docstrings(src, s);

                let obj = Object::Function(Function {
                    ident: ident.clone(),
                    parameters: params,
                    return_type,
                    body,
                    docs,
                });

                let key = module.objects.push(span(obj, s));
                module.symbols.insert(ident.inner, key);
            }

            "system" => {
                let ident = expect_ident(src, s);
                let sys_body = body(src, s.expect_node("main body"))?;
                let docs = docstrings(src, s);
                let mut query = Vec::new();
                for clause in s.expect_node("query").get_list("clauses") {
                    query.push(clause_variant(src, clause)?);
                }
                let before = match s.try_get_node("before body").as_ref() {
                    Some(before) => Some(span(body(src, before.expect_node("body"))?, before)),
                    None => None,
                };
                let after = match s.try_get_node("after body").as_ref() {
                    Some(after) => Some(span(body(src, after.expect_node("body"))?, after)),
                    None => None,
                };

                let obj = Object::System {
                    ident: ident.clone(),
                    query,
                    docs,
                    body: sys_body,
                    before,
                    after,
                };

                let key = module.objects.push(span(obj, s));
                module.symbols.insert(ident.inner, key);
            }

            "component" => {
                let ident = expect_ident(src, s);
                let docs = docstrings(src, s);
                let ty = s.try_get_node("type").as_ref().map(|t| ty(src, t));

                let obj = Object::Component {
                    ident: ident.clone(),
                    ty,
                    docs,
                };

                let key = module.objects.push(span(obj, s));
                module.symbols.insert(ident.inner, key);
            }

            "type definition" => {
                let ident = expect_ident(src, s);
                let docs = docstrings(src, s);
                let ty = s.try_get_node("type").as_ref().map(|t| ty(src, t));

                let obj = Object::Type {
                    ident: ident.clone(),
                    ty,
                    docs,
                };

                let key = module.objects.push(span(obj, s));
                module.symbols.insert(ident.inner, key);
            }

            other => s.ice(&format!("Unhandled top-level item: {}", other)),
        }
    }

    Ok(module)
}

fn clause_variant(src: &str, clause: &Nodes<'_>) -> Result<Span<Clauses>, LoweringError> {
    match clause.get_name() {
        "select" => {
            let ident = expect_ident(src, clause);
            let docs = docstrings(src, clause);
            let mut include = Vec::new();
            let mut exclude = Vec::new();
            let mut optional = Vec::new();
            for component in clause.get_list("components") {
                let mutable = component
                    .try_get_node("mutable")
                    .as_ref()
                    .map(|m| span((), m));
                let component_path = ident_path(src, component.expect_node("component"));
                let alias = alias(src, component.try_get_node("alias").as_ref());
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
                            return Err(LoweringError::MutableExclusion);
                        }
                    }
                    _ => include.push((component_path, Mutability(mutable), alias)),
                }
            }
            Ok(span(
                Clauses::Select(SelectClause {
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
            let ident = expect_ident(src, clause);
            let docs = docstrings(src, clause);
            let event = clause
                .get_list("event")
                .iter()
                .map(|a| {
                    (
                        ident_path(src, a.expect_node("identifier")),
                        alias(src, a.try_get_node("alias").as_ref()),
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
            let expression = expression(src, clause.expect_node("expression"))?;
            Ok(span(
                Clauses::Restriction(RestrictionClause { expression }),
                clause,
            ))
        }
        name => unreachable!("clause {name} unknown"),
    }
}

fn alias(src: &str, node: Option<&Nodes>) -> Alias {
    Alias(node.map(|n| span(expect_ident(src, n), n)))
}

fn body(src: &str, node: &Nodes) -> Result<Span<Body>, LoweringError> {
    Ok(span(
        match node.get_name() {
            "code block" => Body::Block(block(src, node)?),
            "code statement" => Body::Statement(expression(
                src,
                node.expect_node("expression").expect_node("expression"),
            )?),
            other => unreachable!("expected code block or statement, got: {other}"),
        },
        node,
    ))
}

fn block(src: &str, node: &Nodes) -> Result<Vec<Span<Statement>>, LoweringError> {
    let mut statements = Vec::new();

    for stmt_node in node.get_list("statements") {
        let stmt = match stmt_node.get_name() {
            "variable" => {
                let ident = expect_ident(src, stmt_node.expect_node("identifier"));
                let ty = stmt_node.try_get_node("type").as_ref().map(|t| ty(src, t));
                let expression = match stmt_node.try_get_node("expression").as_ref() {
                    Some(e) => Some(expression(src, e)?),
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
                let expr = expression(src, stmt_node.expect_node("expression"));
                span(Statement::Return { expression: expr? }, stmt_node)
            }

            "loop" => {
                let label = try_label(src, stmt_node);
                let body = body(src, stmt_node.expect_node("code body"))?;
                span(Statement::Loop { label, body }, stmt_node)
            }

            "expression statement" => {
                let expr = expression(src, stmt_node.expect_node("expression"));
                span(Statement::Expr { expression: expr? }, stmt_node)
            }

            "break" => span(
                Statement::Break {
                    label: try_label(src, stmt_node),
                },
                stmt_node,
            ),

            "continue" => span(
                Statement::Continue {
                    label: try_label(src, stmt_node),
                },
                stmt_node,
            ),

            "if" => {
                let condition = expression(src, stmt_node.expect_node("expression"))?;
                let then_block = body(src, stmt_node.expect_node("code body"))?;

                let mut else_if = Vec::new();
                for elif in stmt_node.get_list("else if") {
                    let condition = expression(src, elif.expect_node("expression"))?;
                    let block = body(src, elif.expect_node("code body"))?;
                    else_if.push(span(ElseIf { condition, block }, elif));
                }

                let else_block = match stmt_node.try_get_node("else").as_ref() {
                    Some(e) => Some(span(
                        Else {
                            block: body(src, e.expect_node("code body"))?,
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
                let label = try_label(src, stmt_node);
                let condition = expression(src, stmt_node.expect_node("expression"))?;
                let body = body(src, stmt_node.expect_node("code body"))?;

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
fn try_label(src: &str, node: &Nodes) -> Option<Span<SmolStr>> {
    node.try_get_node("label")
        .as_ref()
        .map(|l| expect_ident(src, l))
}

fn expression(src: &str, node: &Nodes) -> Result<Span<Expression>, LoweringError> {
    let items = expression_items(src, node)?;
    let mut pos = 0;
    Ok(parse_expression_prec(&items, &mut pos, 0))
}

fn parse_expression_prec(
    items: &[Span<ExprItem>],
    pos: &mut usize,
    min_prec: u8,
) -> Span<Expression> {
    let mut lhs = match &items[*pos].inner {
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
        let op = match op_item.inner {
            ExprItem::Operator(op) => op,
            _ => break,
        };
        let spanned_op = op_item.clone().map(|_| op);

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
                l: lhs.map(Box::new),
                r: rhs.map(Box::new),
                op: spanned_op,
            },
            loc,
        );
    }

    lhs
}

fn expression_items(src: &str, node: &Nodes) -> Result<Vec<Span<ExprItem>>, LoweringError> {
    let mut items = Vec::new();

    let lvalue_node = node.expect_node("lvalue");
    let lvalue = value(src, &lvalue_node)?;
    items.push(lvalue.map(|v| ExprItem::Value(Expression::Value(v))));

    for entry in node.get_list("rest") {
        match entry {
            Nodes::Token(_) => {
                let op = operator(entry);
                items.push(op.map(ExprItem::Operator));
            }
            Nodes::Node(_) => {
                let v = value(src, entry)?;
                items.push(v.map(|v| ExprItem::Value(Expression::Value(v))));
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
pub fn expect_ident(src: &str, node: &Nodes) -> Span<SmolStr> {
    let t = node.expect_node("identifier");
    match t {
        Nodes::Node(n) => span_from_node(t.stringify(src).into(), n),
        Nodes::Token(tok) => span_from_token(tok.stringify(src).into(), tok),
    }
}

fn ident_path(src: &str, node: &Nodes) -> Span<IdentifierPath> {
    let path = node
        .get_list("path")
        .iter()
        .map(|p| span(p.stringify(src).into(), p))
        .collect();

    span(IdentifierPath { path }, node)
}

fn literal(src: &str, node: &Nodes) -> Result<Span<Literal>, LoweringError> {
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
                            let ident = expect_ident(src, arg);
                            let expr = expression(src, arg.expect_node("expression"))?;
                            args.push(span((ident, expr), arg));
                        }
                        let p = match path {
                            Some(p) => Ok(p),
                            None => Err(Keyword(span((), node))),
                        };
                        return Ok(span(Literal::Structure(p, args), s));
                    }
                    None => return Ok(path.unwrap().map(Literal::Identifier)),
                }
            }

            "array literal" => {
                let mut elements = Vec::new();
                for e in n.get_list("elements") {
                    elements.push(expression(src, e)?)
                }

                Ok(span(Literal::Array(elements), node))
            }

            "tuple literal" => {
                let mut elements = Vec::new();
                for e in n.get_list("expressions") {
                    elements.push(expression(src, e)?)
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
                    Literal::Char(char_literal(&tok.stringify(src))?),
                    tok,
                )),

                "numeric" | "float" => Ok(span_from_token(
                    Literal::Number(numeric_literal(&tok.stringify(src))?),
                    tok,
                )),

                other => panic!("Unhandled token literal kind: {}", other),
            },

            _ => panic!("Unexpected token kind for literal: {:?}", tok.kind),
        },
    }
}

fn value(src: &str, node: &Nodes) -> Result<Span<Value>, LoweringError> {
    let literal = literal(src, node.expect_node("literal"))?;

    Ok(span(
        Value {
            literal,
            postfix: Vec::new(),
        },
        node,
    ))
}

fn ty(src: &str, node: &Nodes) -> Span<Type> {
    let literal = node.expect_node("literal");
    match literal.get_name() {
        "identifier path" => span(
            Type {
                literal: span(TypeLiteral::Path(ident_path(src, literal).inner), literal),
            },
            node,
        ),
        "struct type literal" => span(
            Type {
                literal: span(TypeLiteral::Struct(parameters(src, literal)), literal),
            },
            node,
        ),
        "array type literal" => {
            let type_ = ty(src, literal.expect_node("type"));
            assert!(literal.try_get_node("length").is_none(), "working on it :)");
            span(
                Type {
                    literal: span(TypeLiteral::Array(Box::new(type_), None), literal),
                },
                node,
            )
        }
        name => panic!("{name}"),
    }
}

fn parameter(src: &str, node: &Nodes) -> Span<Parameter> {
    let ident = expect_ident(src, node.expect_node("identifier"));
    let ty = ty(src, node.expect_node("type"));
    let docs = docstrings(src, node);
    span(Parameter { ident, ty, docs }, node)
}

fn parameters(src: &str, node: &Nodes) -> Vec<Span<Parameter>> {
    node.get_list("parameters")
        .iter()
        .map(|p| parameter(src, p))
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
