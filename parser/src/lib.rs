use std::{
    collections::HashMap,
    ffi::OsString,
    fmt::Write,
    fs::{self},
    path::{Path, PathBuf},
};

const TERM_WIDTH: usize = 60;

use annotate_snippets::{AnnotationKind, Group, Level, Renderer, Snippet, renderer::DecorStyle};
use ir::ast::{LoweringError, Span};
use ruparse::{Parser, lexer::PreprocessorError, parser::ParseError};
use smol_str::SmolStr;

use crate::{grammar::gen_parser, lowering::ModuleOk};

pub mod grammar;
pub mod lowering;

#[derive(Debug)]
pub enum AnyParseErr<'parser> {
    Preprocessor(PreprocessorError),
    Parse(ParseError<'parser>),
    Io(std::io::Error),
    NonUtf8FileName(OsString),
    ModuleRoot,
    Lowering(Span<LoweringError>),
}

pub fn parse_source<'a>(
    name: &str,
    src: &'a str,
    parser: Option<&'a Parser>,
    path: Option<PathBuf>,
) -> Result<ModuleOk, AnyParseErr<'a>> {
    let parser = match parser {
        Some(p) => p,
        None => Box::leak(Box::new(gen_parser())),
    };

    let tokens = parser
        .lexer
        .lex_utf8(src)
        .map_err(AnyParseErr::Preprocessor)?;

    let ast = parser.parse(&tokens, src).map_err(AnyParseErr::Parse)?;

    lowering::module_named(name, src, ast.entry, path).map_err(AnyParseErr::Lowering)
}

pub fn parse_directory<'a, F>(
    path: &Path,
    parser: Option<&'a Parser>,
    mut on_error: F,
) -> Result<HashMap<Vec<SmolStr>, ModuleOk>, AnyParseErr<'a>>
where
    F: FnMut(&str, &Path, AnyParseErr<'_>),
{
    let parser = match parser {
        Some(p) => p,
        None => Box::leak(Box::new(gen_parser())),
    };
    let mut result = HashMap::new();

    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().to_str().is_some_and(|s| s.ends_with(".nrd")))
    {
        let content = fs::read_to_string(entry.path()).map_err(AnyParseErr::Io)?;
        let pathbuf = entry.clone().into_path();

        match entry.path().strip_prefix(path).unwrap().to_str() {
            Some(name_str) => {
                let name_key = name_str.strip_suffix(".nrd").unwrap_or(name_str);
                let mut parts: Vec<SmolStr> = Path::new(name_key)
                    .iter()
                    .map(|p| SmolStr::new(p.to_str().unwrap()))
                    .collect();
                match parse_source(name_key, &content, Some(parser), Some(pathbuf)) {
                    Ok(module) => {
                        if parts.last().is_some_and(|last| last.as_str() == "mod") {
                            let _ = parts.pop();
                            if parts.is_empty() {
                                on_error(&content, entry.path(), AnyParseErr::ModuleRoot);
                                continue;
                            }
                        }
                        result.insert(parts, module);
                    }
                    Err(e) => {
                        on_error(&content, entry.path(), e);
                    }
                }
            }
            None => on_error(
                &content,
                entry.path(),
                AnyParseErr::NonUtf8FileName(entry.path().as_os_str().to_os_string()),
            ),
        };
    }

    Ok(result)
}

impl<'a> AnyParseErr<'a> {
    pub fn write(
        &self,
        w: &mut impl Write,
        txt: &'a str,
        filepath: Option<&Path>,
    ) -> std::fmt::Result {
        match self {
            AnyParseErr::Preprocessor(preprocessor_error) => {
                preprocessor_error.write(w, txt, filepath)
            }
            AnyParseErr::Parse(parse_error) => parse_error.write(w, txt, filepath),
            AnyParseErr::Io(error) => write!(w, "{error}"),
            AnyParseErr::NonUtf8FileName(os_string) => {
                write!(w, "File name '{os_string:?}' is not a valid UTF-8")
            }
            AnyParseErr::ModuleRoot => {
                write!(w, "File 'mod.nrd' not allowed in the root")
            }
            AnyParseErr::Lowering(span) => write_lowering_err(span, w, txt, filepath),
        }
    }

    pub fn print(&self, txt: &'a str, filepath: Option<&Path>) -> std::fmt::Result {
        let mut msg = String::new();
        self.write(&mut msg, txt, filepath)?;
        println!("{msg}");
        Ok(())
    }
}

fn write_lowering_err(
    err: &Span<LoweringError>,
    w: &mut impl Write,
    txt: &str,
    filename: Option<&Path>,
) -> std::fmt::Result {
    let (header, id, explained) = err.info();

    let span = err.location.index..err.location.index + err.location.len;
    let mut snippet = Snippet::source(txt)
        .annotation(
            AnnotationKind::Primary
                .span(span)
                .label(format!("{:?}", explained)),
        )
        .fold(true);
    if let Some(file) = filename {
        snippet = snippet.path(file.to_str());
    }
    let report = Group::with_title(
        Level::ERROR
            .with_name("syntax error")
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
