use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};

use dashmap::DashMap;
use ir::const_stage::Context;
use line_index::{LineIndex, TextSize};
use parser::parse_directory;
use ruparse::Parser;
use tower_lsp::{Client, LanguageServer, jsonrpc::Result, lsp_types::*};

use crate::index_file;

pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::COMMENT,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::TYPE,
    SemanticTokenType::DECORATOR,
    SemanticTokenType::new("label"),
];

pub struct Backend {
    pub(crate) client: Client,
    pub(crate) document_map: DashMap<String, String>,
    pub(crate) parser: Parser<'static>,
    pub(crate) workspace_root: DashMap<(), PathBuf>,
}

impl Backend {
    fn workspace_root(&self) -> Option<PathBuf> {
        self.workspace_root.get(&()).map(|v| v.clone())
    }

    fn file_text(&self, path: &Path) -> Option<String> {
        let uri = Url::from_file_path(path).ok()?.to_string();

        if let Some(open) = self.document_map.get(&uri) {
            Some(open.clone())
        } else {
            std::fs::read_to_string(path).ok()
        }
    }

    fn range_from_span(&self, src: &str, index: usize, len: usize) -> Range {
        let line_index = LineIndex::new(src);

        let start = line_index.line_col(TextSize::new(index as u32));

        let end = line_index.line_col(TextSize::new((index + len) as u32));

        Range::new(
            Position::new(start.line, start.col),
            Position::new(end.line, end.col),
        )
    }

    fn diagnostic(
        &self,
        src: &str,
        index: usize,
        len: usize,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Diagnostic {
        Diagnostic {
            range: self.range_from_span(src, index, len),
            severity: Some(severity),
            source: Some("neruda".into()),
            message: message.into(),
            ..Default::default()
        }
    }

    async fn validate_workspace(&self) {
        let Some(root) = self.workspace_root() else {
            return;
        };

        let modules = match parse_directory(&root, None, |_src, _path, _err| {}) {
            Ok(v) => v,
            Err(_) => return,
        };

        let mut diagnostics: HashMap<Url, Vec<Diagnostic>> = HashMap::new();

        /*
         * Lowering diagnostics
         */

        for (_module_path, module_ok) in &modules {
            let Some(src) = self.file_text(module_ok.module.path.as_ref().unwrap().as_path())
            else {
                continue;
            };

            let Ok(uri) = Url::from_file_path(module_ok.module.path.as_ref().unwrap().as_path())
            else {
                continue;
            };

            let file_diags = diagnostics.entry(uri).or_default();

            for warn in &module_ok.diagnostics.warns {
                file_diags.push(self.diagnostic(
                    &src,
                    warn.location.index,
                    warn.location.len,
                    DiagnosticSeverity::WARNING,
                    format!("{}", warn.inner),
                ));
            }

            for info in &module_ok.diagnostics.diagnostics {
                file_diags.push(self.diagnostic(
                    &src,
                    info.location.index,
                    info.location.len,
                    DiagnosticSeverity::INFORMATION,
                    format!("{}", info.inner),
                ));
            }
        }

        return;

        /*
         * IR
         */

        let ir_ctx = Context::from_ast(HashMap::from_iter(
            modules
                .iter()
                .map(|(k, v)| (k.clone(), Arc::new(v.module.clone()))),
        ));

        let ir_ctx = match ir_ctx {
            Ok(v) => v,

            Err((ir_ctx, err)) => {
                let module = ir_ctx.types.modules.get_unchecked(&err.module);
                if let Some(path) = &module.ast.path {
                    if let Some(src) = self.file_text(&path) {
                        if let Ok(uri) = Url::from_file_path(&path) {
                            diagnostics.entry(uri).or_default().push(self.diagnostic(
                                &src,
                                err.span.index,
                                err.span.len,
                                DiagnosticSeverity::ERROR,
                                format!("{:?}", err.inner),
                            ));
                        }
                    }
                }

                self.publish_diagnostics(diagnostics).await;

                return;
            }
        };

        /*
         * IR warnings
         */

        for warn in &ir_ctx.diagnostics.warnings {
            let module = ir_ctx.types.modules.get_unchecked(&warn.module);
            let Some(path) = &module.ast.path else {
                continue;
            };

            let Some(src) = self.file_text(&path) else {
                continue;
            };

            let Ok(uri) = Url::from_file_path(&path) else {
                continue;
            };

            diagnostics.entry(uri).or_default().push(self.diagnostic(
                &src,
                warn.span.index,
                warn.span.len,
                DiagnosticSeverity::WARNING,
                format!("{:?}", warn.inner),
            ));
        }

        /*
         * Publish
         */

        self.publish_diagnostics(diagnostics).await;
    }

    async fn publish_diagnostics(&self, diagnostics: HashMap<Url, Vec<Diagnostic>>) {
        let mut published = HashSet::new();

        for (uri, diags) in diagnostics {
            published.insert(uri.clone());

            self.client.publish_diagnostics(uri, diags, None).await;
        }

        /*
         * Clear stale diagnostics
         */

        for entry in self.document_map.iter() {
            let Ok(uri) = Url::parse(entry.key()) else {
                continue;
            };

            if !published.contains(&uri) {
                self.client.publish_diagnostics(uri, vec![], None).await;
            }
        }
    }

    async fn publish_ice(&self, uri: Option<Url>, message: impl Into<String>) {
        let diagnostic = Diagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 1)),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("neruda".into()),
            message: format!("Internal Compiler Error (ICE)\n\n{}", message.into()),
            ..Default::default()
        };
        if let Some(uri) = uri {
            self.client
                .publish_diagnostics(uri, vec![diagnostic], None)
                .await;
        }
        self.client
            .show_message(MessageType::ERROR, "Neruda compiler crashed (ICE)")
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(folder) = params.workspace_folders.as_ref().and_then(|v| v.first()) {
            if let Ok(path) = folder.uri.to_file_path() {
                self.workspace_root.insert((), path);
            }
        } else if let Some(root) = params.root_uri {
            if let Ok(path) = root.to_file_path() {
                self.workspace_root.insert((), path);
            }
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),

                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(
                        SemanticTokensRegistrationOptions {
                            text_document_registration_options: TextDocumentRegistrationOptions {
                                document_selector: Some(vec![DocumentFilter {
                                    language: Some("neruda".into()),

                                    scheme: Some("file".into()),

                                    pattern: Some("*.nrd".into()),
                                }]),
                            },

                            semantic_tokens_options: SemanticTokensOptions {
                                legend: SemanticTokensLegend {
                                    token_types: TOKEN_TYPES.to_vec(),

                                    token_modifiers: vec![],
                                },

                                full: Some(SemanticTokensFullOptions::Bool(true)),

                                ..Default::default()
                            },

                            static_registration_options: StaticRegistrationOptions::default(),
                        },
                    ),
                ),

                ..Default::default()
            },

            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Neruda language server initialized")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;

        self.document_map.insert(uri.to_string(), text);

        self.validate_workspace().await
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        if let Some(change) = params.content_changes.into_iter().next() {
            self.document_map.insert(uri.to_string(), change.text);

            self.validate_workspace().await
        }
    }

    async fn did_save(&self, _: DidSaveTextDocumentParams) {
        self.validate_workspace().await
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.document_map.remove(params.text_document.uri.as_str());

        self.validate_workspace().await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri_str = params.text_document.uri.to_string();

        let Some(src) = self.document_map.get(&uri_str).map(|v| v.clone()) else {
            return Ok(None);
        };

        let (_, mut spans, _) =
            match index_file(&self.parser, &src, Some(PathBuf::from(uri_str.clone()))) {
                Ok(v) => v,

                Err(_) => {
                    return Ok(Some(
                        SemanticTokensResult::Tokens(SemanticTokens::default()),
                    ));
                }
            };

        spans.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));

        let mut prev_line = 0u32;
        let mut prev_col = 0u32;

        let mut data = Vec::with_capacity(spans.len());

        for span in spans {
            let line = span.line as u32;
            let col = span.column as u32;

            let delta_line = line - prev_line;

            let delta_start = if delta_line == 0 { col - prev_col } else { col };

            data.push(SemanticToken {
                delta_line,
                delta_start,
                length: span.len as u32,
                token_type: span.ty as u32,
                token_modifiers_bitset: 0,
            });

            prev_line = line;
            prev_col = col;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}
