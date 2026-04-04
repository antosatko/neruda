use core::panic;
use dashmap::DashMap;
use ir::ast::{
    Alias, Body, Diagnostics, Expression, Function, IdentifierPath, Literal, LoweringError, Module,
    Object, Parameter, Postfix, Statement, Type, Value,
};
use line_index::{LineCol, LineIndex, TextSize};
use parser::{
    grammar::{Token, gen_parser},
    lowering::{ModuleOk, module_named},
};
use ruparse::{Parser, lexer::PreprocessorError, parser::ParseError};
use tower_lsp::{LspService, Server};

use crate::server::Backend;

mod server;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        document_map: DashMap::new(),
        parser: gen_parser(),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
#[derive(Debug, Copy, Clone)]
pub struct Span {
    len: usize,
    line: usize,
    column: usize,
    ty: u8,
}

#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum Types {
    Comment,
    Ident,
    Keyword,
    String,
    Number,
    Operator,
    Type,
    SpecialOperator,
    Label,
}

pub enum IndexErr<'a> {
    Lex(PreprocessorError),
    Parse(ParseError<'a>),
    Lowering(ir::ast::Span<LoweringError>),
}

pub fn index_file<'p, 'src>(
    parser: &'p Parser<'p>,
    src: &'src str,
) -> Result<(Module, Vec<Span>, Diagnostics), IndexErr<'p>>
where
    'p: 'src,
    'src: 'p,
{
    let line_index = LineIndex::new(src);
    let mut spans = Vec::new();

    let tokens = match parser.lexer.lex_utf8(src) {
        Ok(t) => t,
        Err(e) => return Err(IndexErr::Lex(e)),
    };
    index_tokens(&tokens, &mut spans);
    let ast = match parser.parse(&tokens, src) {
        Ok(m) => m,
        Err(e) => return Err(IndexErr::Parse(e)),
    };

    let ModuleOk {
        module,
        diagnostics,
    } = match module_named("", src, ast.entry) {
        Ok(ast) => ast,
        Err(e) => return Err(IndexErr::Lowering(e)),
    };

    module.index(&line_index, &mut spans);

    Ok((module, spans, diagnostics))
}

impl IntoSpan for Token<'_> {
    fn span(&self, ty: Types, _: &LineIndex) -> Span {
        Span {
            len: self.len,
            line: self.location.line - 1,
            column: self.location.column - 1,
            ty: ty as _,
        }
    }
}

fn index_tokens(tokens: &Vec<Token>, spans: &mut Vec<Span>) {
    let line_index = &LineIndex::new("");
    for token in tokens {
        match &token.kind {
            ruparse::lexer::TokenKinds::Complex(t) => match *t {
                "tl docstr" | "docstr" | "comment" => {
                    spans.push(token.span(Types::Comment, line_index))
                }
                "numeric" | "float" => spans.push(token.span(Types::Number, line_index)),
                "char" | "string" => spans.push(token.span(Types::String, line_index)),
                a => panic!("got a: {a}"),
            },
            _ => (),
        }
    }
}

trait IndexedWalk {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>);
}

trait IntoSpan {
    #[must_use]
    fn span(&self, ty: Types, line_index: &LineIndex) -> Span;
    #[must_use]
    fn span_word(&self, ty: Types, line_index: &LineIndex, word: &str) -> Span {
        let mut this = self.span(ty, line_index);
        this.len = word.chars().count();
        this
    }
}

impl<T> IntoSpan for ir::ast::Span<T> {
    fn span(&self, ty: Types, line_index: &LineIndex) -> Span {
        let LineCol { line, col } = line_index.line_col(TextSize::new(self.location.index as _));
        Span {
            len: self.location.len,
            line: line as usize,
            column: col as usize,
            ty: ty as _,
        }
    }
}

/* ===================== IMPLEMENTATIONS ===================== */

impl IndexedWalk for Module {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        self.objects.iter().for_each(|o| o.index(line_index, spans));
    }
}

impl IndexedWalk for ir::ast::Span<Object> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        match &self.inner {
            Object::Scheduler {
                ident,
                resources,
                systems,
                init,
                docs: _,
            } => {
                spans.push(self.span_word(Types::Keyword, line_index, "scheduler"));
                spans.push(ident.span(Types::Ident, line_index));

                if let Some(resources) = resources {
                    spans.push(resources.span_word(Types::Keyword, line_index, "resources"));
                    resources.iter().for_each(|r| r.index(line_index, spans));
                }
                if let Some(systems) = systems {
                    spans.push(systems.span_word(Types::Keyword, line_index, "systems"));
                    for generic in systems.iter().filter_map(|g| g.generics.as_ref()) {
                        for ty in &generic.inner {
                            ty.index(line_index, spans);
                        }
                    }
                }
                if let Some(init) = init {
                    spans.push(init.1.0.span_word(Types::Keyword, line_index, "init"));
                    init.0.index(line_index, spans);
                }
            }
            Object::Function {
                ident,
                parameters,
                return_type,
                body,
                docs: _,
                generics,
            } => {
                spans.push(self.span_word(Types::Keyword, line_index, "function"));
                spans.push(ident.span(Types::Ident, line_index));

                parameters.iter().for_each(|p| p.index(line_index, spans));
                if let Some(ret) = return_type {
                    ret.index(line_index, spans);
                }
                body.index(line_index, spans);
                if let Some(generics) = generics {
                    spans.extend(generics.iter().map(|g| g.span(Types::Type, line_index)));
                }
            }
            Object::Component { ident, ty, docs: _ } => {
                spans.push(self.span_word(Types::Keyword, line_index, "component"));
                spans.push(ident.span(Types::Ident, line_index));
                if let Some(ty) = &ty {
                    ty.index(line_index, spans);
                }
            }
            Object::Type {
                ident,
                ty,
                docs: _,
                generics,
            } => {
                spans.push(self.span_word(Types::Keyword, line_index, "type"));
                spans.push(ident.span(Types::Ident, line_index));
                if let Some(ty) = &ty {
                    ty.index(line_index, spans);
                }
                if let Some(generics) = generics {
                    spans.extend(generics.iter().map(|g| g.span(Types::Type, line_index)));
                }
            }
            Object::System {
                ident,
                docs: _,
                query,
                body,
                after,
                before,
                generics,
            } => {
                spans.push(self.span_word(Types::Keyword, line_index, "system"));
                spans.push(ident.span(Types::Ident, line_index));
                body.index(line_index, spans);
                if let Some(before) = before {
                    spans.push(before.span_word(Types::Keyword, line_index, "before"));
                    before.inner.index(line_index, spans);
                }
                if let Some(after) = after {
                    spans.push(after.span_word(Types::Keyword, line_index, "after"));
                    after.inner.index(line_index, spans);
                }
                if let Some(generics) = generics {
                    spans.extend(generics.iter().map(|g| g.span(Types::Type, line_index)));
                }
                for clause in query {
                    match &clause.inner {
                        ir::ast::Clauses::Select(select) => {
                            spans.push(select.ident.span(Types::Ident, line_index));
                            for (component, mutability, alias) in &select.include {
                                for ident in &component.inner.path {
                                    spans.push(ident.span(Types::Type, line_index));
                                }
                                if let Some(mutability) = &mutability.0 {
                                    spans.push(mutability.span_word(
                                        Types::Keyword,
                                        line_index,
                                        "mut",
                                    ));
                                }
                                if let Alias(Some(alias)) = alias {
                                    spans.push(alias.span_word(Types::Keyword, line_index, "as"));
                                    spans.push(alias.inner.span(Types::Ident, line_index))
                                }
                            }
                            for (component, mutability, alias) in &select.optional {
                                for ident in &component.inner.path {
                                    spans.push(ident.span(Types::Type, line_index));
                                }
                                if let Some(mutability) = &mutability.0 {
                                    spans.push(mutability.span_word(
                                        Types::Keyword,
                                        line_index,
                                        "mut",
                                    ));
                                }
                                if let Alias(Some(alias)) = alias {
                                    spans.push(alias.span_word(Types::Keyword, line_index, "as"));
                                    spans.push(alias.inner.span(Types::Ident, line_index))
                                }
                            }
                            for (component, alias) in &select.exclude {
                                for ident in &component.inner.path {
                                    spans.push(ident.span(Types::Type, line_index));
                                }
                                if let Alias(Some(alias)) = alias {
                                    spans.push(alias.span_word(Types::Keyword, line_index, "as"));
                                    spans.push(alias.inner.span(Types::Ident, line_index))
                                }
                            }
                        }
                        ir::ast::Clauses::Action((action, keyword)) => {
                            spans.push(action.ident.span(Types::Ident, line_index));
                            spans.push(keyword.0.span_word(Types::Keyword, line_index, "on"));
                            for (event_component, alias) in &action.event {
                                if let Alias(Some(alias)) = alias {
                                    spans.push(alias.span_word(Types::Keyword, line_index, "as"));
                                    spans.push(alias.inner.span(Types::Type, line_index))
                                }
                                for ident in &event_component.path {
                                    spans.push(ident.span(Types::Type, line_index));
                                }
                            }
                        }
                        ir::ast::Clauses::Restriction(restriction) => {
                            spans.push(clause.span_word(Types::Keyword, line_index, "where"));
                            restriction.expression.index(line_index, spans);
                        }
                    }
                }
            }
        }
    }
}

impl IndexedWalk for ir::ast::Span<Body> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        match &self.inner {
            Body::Block(stmts) => stmts.iter().for_each(|s| s.index(line_index, spans)),
            Body::Statement(expr) => {
                spans.push(self.span_word(Types::SpecialOperator, line_index, "=>"));
                expr.index(line_index, spans)
            }
        }
    }
}

impl IndexedWalk for ir::ast::Span<Statement> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        match &self.inner {
            Statement::Var {
                ident,
                ty,
                expression,
            } => {
                spans.push(self.span_word(Types::Keyword, line_index, "var"));
                spans.push(ident.span(Types::Ident, line_index));
                if let Some(t) = ty {
                    t.index(line_index, spans);
                }
                if let Some(e) = expression {
                    e.index(line_index, spans);
                }
            }
            Statement::If {
                condition,
                then_block,
                else_if,
                else_block,
            } => {
                spans.push(self.span_word(Types::Keyword, line_index, "if"));
                condition.index(line_index, spans);
                then_block.index(line_index, spans);

                for elif in else_if {
                    spans.push(elif.span_word(Types::Keyword, line_index, "else if"));
                    elif.inner.condition.index(line_index, spans);
                    elif.inner.block.index(line_index, spans);
                }
                if let Some(eb) = else_block {
                    spans.push(eb.span_word(Types::Keyword, line_index, "else"));
                    eb.inner.block.index(line_index, spans);
                }
            }
            Statement::While {
                label,
                condition,
                body,
            } => {
                spans.push(self.span_word(Types::Keyword, line_index, "while"));
                if let Some(l) = label {
                    spans.push(l.span(Types::Label, line_index)); // Highlight as Label
                }
                condition.index(line_index, spans);
                body.index(line_index, spans);
            }
            Statement::Loop { label, body } => {
                spans.push(self.span_word(Types::Keyword, line_index, "loop"));
                if let Some(l) = label {
                    spans.push(l.span(Types::Label, line_index)); // Highlight as Label
                }
                body.index(line_index, spans);
            }
            Statement::Break { label } => {
                spans.push(self.span_word(Types::Keyword, line_index, "break"));
                if let Some(l) = label {
                    spans.push(l.span(Types::Label, line_index));
                }
            }
            Statement::Continue { label } => {
                spans.push(self.span_word(Types::Keyword, line_index, "continue"));
                if let Some(l) = label {
                    spans.push(l.span(Types::Label, line_index));
                }
            }
            Statement::Return { expression } => {
                spans.push(self.span_word(Types::Keyword, line_index, "return"));
                expression.index(line_index, spans);
            }
            Statement::Expr { expression } => expression.index(line_index, spans),
        }
    }
}

impl IndexedWalk for ir::ast::Span<Parameter> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        spans.push(self.inner.ident.span(Types::Ident, line_index));
        self.inner.ty.index(line_index, spans);
        spans.extend(self.docs.iter().map(|d| d.span(Types::Comment, line_index)));
    }
}

impl IndexedWalk for ir::ast::Span<Type> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        let Type { literal, generics } = &self.inner;
        match &literal.inner {
            ir::ast::TypeLiteral::Path(identifier_path) => {
                for ident in &identifier_path.path {
                    spans.push(ident.span(Types::Type, line_index));
                }
            }
            ir::ast::TypeLiteral::Struct(paramers) => {
                spans.push(self.literal.span_word(Types::Keyword, line_index, "struct"));
                for param in &paramers.0 {
                    param.index(line_index, spans);
                }
                if let Some(generics) = &paramers.1 {
                    for generic in &generics.inner {
                        spans.push(generic.span(Types::Type, line_index));
                    }
                }
            }
            ir::ast::TypeLiteral::Array(ty, len) => {
                ty.index(line_index, spans);
                let _: Option<usize> = *len;
            }
            ir::ast::TypeLiteral::Tuple(params) => {
                for ty in params {
                    ty.index(line_index, spans);
                }
            }
        }
        if let Some(generics) = generics {
            for generic in &generics.inner {
                generic.index(line_index, spans);
            }
        }
    }
}

impl IndexedWalk for ir::ast::Span<IdentifierPath> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        for ident in &self.inner.path {
            spans.push(ident.span(Types::Ident, line_index));
        }
    }
}

impl IndexedWalk for ir::ast::Span<Expression> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        match &self.inner {
            Expression::Value(v) => v.index(line_index, spans),
            Expression::Binary { l, r, op } => {
                l.index(line_index, spans);
                spans.push(op.span(Types::Operator, line_index));
                r.index(line_index, spans);
            }
        }
    }
}

impl IndexedWalk for ir::ast::Span<Box<Expression>> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        let inner_span = ir::ast::Span {
            location: self.location,
            inner: *self.inner.clone(),
        };
        inner_span.index(line_index, spans);
    }
}

// Kept one consistent Value implementation and fixed the missing literal call
impl IndexedWalk for Value {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        self.literal.index(line_index, spans); // Fixed missing call
        for p in &self.postfix {
            p.index(line_index, spans);
        }
    }
}

impl IndexedWalk for ir::ast::Span<Value> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        self.inner.index(line_index, spans);
    }
}

impl IndexedWalk for ir::ast::Span<Postfix> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        match &self.inner {
            Postfix::Field(ident) => spans.push(ident.span(Types::Ident, line_index)),
            Postfix::Call(args) => args.iter().for_each(|a| a.index(line_index, spans)),
            Postfix::Index(expr) => expr.index(line_index, spans),
            Postfix::Refs(_) | Postfix::Derefs(_) => {
                spans.push(self.span(Types::Operator, line_index));
            }
        }
    }
}

impl IndexedWalk for ir::ast::Span<Literal> {
    fn index(&self, line_index: &LineIndex, spans: &mut Vec<Span>) {
        match &self.inner {
            Literal::Identifier(path) => {
                for ident in &path.path {
                    spans.push(ident.span(Types::Ident, line_index));
                }
            }
            Literal::Array(exprs) | Literal::Tuple(exprs) => {
                exprs.iter().for_each(|e| e.index(line_index, spans));
            }
            Literal::Structure(path, args) => {
                match path {
                    Ok(p) => {
                        for ident in &p.path {
                            spans.push(ident.span(Types::Ident, line_index));
                        }
                    }
                    Err(kw) => spans.push(kw.0.span_word(Types::Keyword, line_index, "struct")),
                }
                for arg in args {
                    spans.push(arg.0.span(Types::Ident, line_index));
                    arg.1.index(line_index, spans);
                }
            }
            Literal::Number(_) | Literal::String(_) | Literal::Char(_) => (),
        }
    }
}
