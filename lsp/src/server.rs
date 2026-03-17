use dashmap::DashMap;
use ruparse::Parser;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::{IndexErr, index_file};

pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::COMMENT,      // 0
    SemanticTokenType::VARIABLE,     // 1
    SemanticTokenType::KEYWORD,      // 2
    SemanticTokenType::STRING,       // 3
    SemanticTokenType::NUMBER,       // 4
    SemanticTokenType::OPERATOR,     // 5
    SemanticTokenType::TYPE,         // 6
    SemanticTokenType::DECORATOR,    // 7
    SemanticTokenType::new("label"), // 8
];

pub struct Backend {
    pub(crate) client: Client,
    pub(crate) document_map: DashMap<String, String>,
    pub(crate) parser: Parser<'static>,
}

impl Backend {
    /// Helper to publish diagnostics (errors) to the client
    async fn validate_document(&self, uri: Url, src: &str) {
        let mut diagnostics = Vec::new();

        // Run the indexer/parser to check for errors
        if let Err(e) = index_file(&self.parser, src) {
            let diag = match e {
                IndexErr::Lex(err) => {
                    let mut buf = String::new();
                    let _ = err.write(&mut buf, src, Some("main.nrd"));
                    let location = err.location;
                    Diagnostic::new_simple(
                        Range::new(
                            Position::new(location.line as _, location.column as _),
                            Position::new(location.line as _, location.column as _),
                        ),
                        strip_ansi_escapes::strip_str(&buf),
                    )
                }
                IndexErr::Parse(err) => {
                    let mut buf = String::new();
                    let _ = err.write(&mut buf, src, Some("main.nrd"));
                    let location = err.location;
                    Diagnostic::new_simple(
                        Range::new(
                            Position::new(location.line as _, location.column as _),
                            Position::new(location.line as _, location.column as _),
                        ),
                        strip_ansi_escapes::strip_str(&buf),
                    )
                }
                IndexErr::Idk => {
                    Diagnostic::new_simple(Range::default(), "Internal compiler error".to_string())
                }
            };
            diagnostics.push(diag);
        }

        // Send the list (empty list clears existing errors)

        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
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
                                    language: Some("neruda".to_string()),
                                    scheme: Some("file".to_string()),
                                    pattern: Some("*.nrd".to_string()),
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
            .log_message(MessageType::INFO, "Neruda Server Ready")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.document_map.insert(uri.to_string(), text.clone());
        self.validate_document(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(change) = params.content_changes.into_iter().next() {
            self.document_map
                .insert(uri.to_string(), change.text.clone());
            self.validate_document(uri, &change.text).await;
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri_str = params.text_document.uri.to_string();
        let src = match self.document_map.get(&uri_str) {
            Some(text) => text.clone(),
            None => return Ok(None),
        };

        // If indexing fails, we return an empty token set so the UI doesn't flicker/error out
        let mut spans = match index_file(&self.parser, &src) {
            Ok(s) => s,
            Err(_) => {
                return Ok(Some(
                    SemanticTokensResult::Tokens(SemanticTokens::default()),
                ));
            }
        };

        // Sort for Delta Encoding
        spans.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));

        let mut pre_line = 0;
        let mut pre_start = 0;
        let mut data = Vec::with_capacity(spans.len());

        for span in spans {
            let line = span.line as u32;
            let start = span.column as u32;

            let delta_line = line - pre_line;
            let delta_start = if delta_line == 0 {
                start - pre_start
            } else {
                start
            };

            data.push(SemanticToken {
                delta_line,
                delta_start,
                length: span.len as u32,
                token_type: span.ty as u32,
                token_modifiers_bitset: 0,
            });

            pre_line = line;
            pre_start = start;
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
