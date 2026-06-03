use std::{fmt::Write, ops::Deref};

use annotate_snippets::{AnnotationKind, Group, Level, Renderer, Snippet, renderer::DecorStyle};

use crate::const_stage::{Context, Diagnostic, Error, Errors, Warnings};

const TERM_WIDTH: usize = 60;

impl Error {
    pub fn write(&self, w: &mut impl Write, ctx: &Context) -> std::fmt::Result {
        let (id, header, snippet) = self.inner.id_header_snippet_report(ctx);
        let span = self.span.index..self.span.index + self.span.len;
        let src = &ctx.types.modules.get_unchecked(&self.module).ast.src;
        let mut snippet = Snippet::source(src.as_ref())
            .annotation(AnnotationKind::Primary.span(span).label(snippet))
            // .annotation(
            //     AnnotationKind::Visible
            //         .span(self.location.index - 5..self.location.index + self.location.len),
            // )
            .fold(true);
        if let Some(file) = &ctx.types.modules.get_unchecked(&self.module).ast.path {
            snippet = snippet.path(file.to_str());
        }
        let report = Group::with_title(
            Level::ERROR
                .with_name("semantic error")
                .primary_title(header)
                .id(id),
        )
        .element(snippet);
        let render = Renderer::styled()
            .decor_style(DecorStyle::Unicode)
            .term_width(TERM_WIDTH)
            .render(&[report]);
        write!(w, "{render}")
    }

    pub fn print(&self, ctx: &Context) -> std::fmt::Result {
        let mut msg = String::new();
        self.write(&mut msg, ctx)?;
        println!("{msg}");
        Ok(())
    }
}

impl Errors {
    pub fn id_header_snippet_report(&self, ctx: &Context) -> (&'static str, String, String) {
        match self {
            Self::IllegalType(obj) => {
                let ident = obj.ident(ctx);
                (
                    "400",
                    format!("Use of type {ident} is illegal here"),
                    format!("Got {ident}"),
                )
            }
            Self::NonConstraintType(obj) => {
                let ident = obj.ident(ctx);
                (
                    "401",
                    format!("Type {ident} must be a trait type"),
                    format!("Got {ident}"),
                )
            }
            Self::CouldNotSubstituteType(ty) => (
                "402",
                format!("Could not substitute for type {}", ty.stringify(&ctx.types),),
                format!("Use of generic argument is illegal"),
            ),
            Self::TypeMismatch { expected, got } => {
                let expected = expected.stringify(&ctx.types);
                let got = got.stringify(&ctx.types);
                (
                    "403",
                    format!("Type mismatch, expected: {expected}, got {got}"),
                    format!("Got {got}"),
                )
            }
            Errors::TypeNotFound(smol_strs) => (
                "404",
                format!("Type '{}' not found", smol_strs.join("::")),
                format!("Unknown"),
            ),
            Errors::ObjectNotFound(smol_strs) => (
                "405",
                format!("Object '{}' not found", smol_strs.join("::")),
                format!("Unknown"),
            ),
            Errors::NotConst => (
                "406",
                format!("Could not evaluate in constant context"),
                format!("not constant"),
            ),
            Errors::CanNotApplyConst { op, left, right } => (
                "407",
                format!(
                    "Operator {op} can not be applied for types {}, {} in const context.",
                    left.type_of().stringify(&ctx.types),
                    right.type_of().stringify(&ctx.types)
                ),
                format!("Not applicable"),
            ),
            Errors::EvalModule(path, _) => (
                "408",
                format!("Could not evaluate module '{}'", path.join("::")),
                format!("Module evaluation"),
            ),
            Errors::UndefinedSelf => (
                "409",
                format!("Self is not defined in this context"),
                format!("Self undefined"),
            ),
            Errors::NonPrimitiveType { got } => (
                "410",
                format!(
                    "Context only allows primitive types, got: {}",
                    got.stringify(&ctx.types)
                ),
                format!("Non primitive type"),
            ),
            Errors::ExpectedNumericConst { got } => (
                "411",
                format!("Expected a numeric constant, got: {}", got.stringify()),
                format!("Expected numeric"),
            ),
            Errors::DuplicateIdentifier(ident) => (
                "412",
                format!("Identifier '{ident}' defined multiple times"),
                format!("Duplicate identifier"),
            ),
            Errors::GenericArityMismatch { expected, found } => (
                "413",
                format!("Expected {expected} generic argument(s), found {found}"),
                format!("Generic arity mismatch"),
            ),
            Errors::FailedTypeInfer => (
                "414",
                format!("Could not automatically infer type"),
                format!("Failed type infer"),
            ),
            Errors::UndefinedAutostep(ty) => (
                "415",
                format!(
                    "Type '{}' does not implement autostep or is not constant",
                    ty.stringify(&ctx.types)
                ),
                format!("Failed autostep"),
            ),
            Errors::FailedImplicitCast { from, to } => (
                "416",
                format!(
                    "Could not implicitly cast from '{}' to '{}'",
                    from.stringify(&ctx.types),
                    to.stringify(&ctx.types),
                ),
                format!("Failed implicit cast"),
            ),
            Errors::ArrayElementCountMismatch {
                expected: (exp_type, exp_size),
                got: (got_type, Some(got_size)),
            } => (
                "417",
                format!(
                    "Type '{}' expected to have {exp_size} element(s) to match type '{}', instead got {got_size}",
                    got_type.stringify(&ctx.types),
                    exp_type.stringify(&ctx.types),
                ),
                format!("Array element count mismatch"),
            ),
            Errors::ArrayElementCountMismatch {
                expected: (exp_type, exp_size),
                got: (got_type, None),
            } => (
                "418",
                format!(
                    "Unsized type '{}' expected to have {exp_size} element(s) to match type '{}'",
                    got_type.stringify(&ctx.types),
                    exp_type.stringify(&ctx.types),
                ),
                format!("Array element count mismatch"),
            ),
            Errors::UndefinedDefault(ty) => (
                "419",
                format!(
                    "Could not construct a default value for type '{}'",
                    ty.stringify(&ctx.types)
                ),
                format!("Undefined default value"),
            ),
            Errors::ExpectedOptionalResource { ident } => (
                "420",
                format!("Resource flag '{ident}' expected to be nullable",),
                format!("Expected nullable resource"),
            ),
        }
    }
}

impl Diagnostic<Warnings> {
    pub fn write(&self, w: &mut impl Write, ctx: &Context) -> std::fmt::Result {
        let (id, header, snippet) = self.inner.id_header_snippet_report(ctx);
        let span = self.span.index..self.span.index + self.span.len;
        let src = &ctx.types.modules.get_unchecked(&self.module).ast.src;
        let mut snippet = Snippet::source(src.as_ref())
            .annotation(AnnotationKind::Primary.span(span).label(snippet))
            // .annotation(
            //     AnnotationKind::Visible
            //         .span(self.location.index - 5..self.location.index + self.location.len),
            // )
            .fold(true);
        if let Some(file) = &ctx.types.modules.get_unchecked(&self.module).ast.path {
            snippet = snippet.path(file.to_str());
        }
        let report = Group::with_title(
            Level::WARNING
                .with_name("semantic warning")
                .primary_title(header)
                .id(id),
        )
        .element(snippet);
        let render = Renderer::styled()
            .decor_style(DecorStyle::Unicode)
            .term_width(TERM_WIDTH)
            .render(&[report]);
        write!(w, "{render}")
    }

    pub fn print(&self, ctx: &Context) -> std::fmt::Result {
        let mut msg = String::new();
        self.write(&mut msg, ctx)?;
        println!("{msg}");
        Ok(())
    }
}

impl Warnings {
    pub fn id_header_snippet_report(&self, ctx: &Context) -> (&'static str, String, String) {
        match self {
            Self::VariableUnused { function, var } => (
                "500",
                format!(
                    "Variable '{}' is never used",
                    ctx.objects
                        .functions
                        .get_unchecked(function)
                        .data
                        .ir
                        .get_done()
                        .variables
                        .get_unchecked(var)
                        .identifier
                        .deref()
                ),
                format!("Unused variable"),
            ),
        }
    }
}
