//! Language server behind the Better Highlights Zed extension: cognitive complexity as inlay
//! hints (plus a warning past the limit), and tagged comments as semantic tokens.

mod comments;
mod complexity;
mod config;
mod languages;

use std::collections::HashMap;
use std::error::Error;
use std::path::Path;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification as _,
    PublishDiagnostics,
};
use lsp_types::request::{InlayHintRequest, Request as _, SemanticTokensFullRequest};
use lsp_types::{
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, InitializeParams, InlayHint, InlayHintLabel,
    InlayHintServerCapabilities, InlayHintTooltip, OneOf, Position, PublishDiagnosticsParams,
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind, Uri,
};

use complexity::FunctionComplexity;
use config::Config;

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();
    let (id, params) = connection.initialize_start()?;
    let params: InitializeParams = serde_json::from_value(params)?;
    let config = Config::from_options(params.initialization_options);
    let result = serde_json::json!({ "capabilities": capabilities(&config) });
    connection.initialize_finish(id, result)?;
    run(&connection, &config)?;
    drop(connection);
    io_threads.join()?;
    Ok(())
}

/// Tagged comments are `comment` tokens with one modifier per tag, so they stay ordinary comments
/// until the user styles them via `semantic_token_rules`.
fn capabilities(config: &Config) -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
            Default::default(),
        ))),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: vec![SemanticTokenType::COMMENT],
                    token_modifiers: config
                        .modifiers()
                        .into_iter()
                        .map(|modifier| SemanticTokenModifier::from(modifier.to_string()))
                        .collect(),
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                ..Default::default()
            },
        )),
        ..Default::default()
    }
}

/// Open documents' full text, keyed by URI string (`Uri` has interior mutability, so it makes a poor key).
type Documents = HashMap<String, String>;

fn run(connection: &Connection, config: &Config) -> Result<(), Box<dyn Error + Sync + Send>> {
    let mut documents = Documents::new();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    return Ok(());
                }
                connection
                    .sender
                    .send(Message::Response(respond(&documents, config, request)))?;
            }
            Message::Notification(notification) => {
                if let Some(uri) = track(&mut documents, notification) {
                    let diagnostics = parse(&documents, &uri)
                        .map(|parsed| diagnostics(&parsed, config))
                        .unwrap_or_default();
                    let params = PublishDiagnosticsParams::new(uri, diagnostics, None);
                    connection
                        .sender
                        .send(Message::Notification(Notification::new(
                            PublishDiagnostics::METHOD.into(),
                            params,
                        )))?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

/// Applies a document open/change/close, returning the URI whose diagnostics are now stale.
fn track(documents: &mut Documents, notification: Notification) -> Option<Uri> {
    let params = notification.params;
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let params = serde_json::from_value::<DidOpenTextDocumentParams>(params).ok()?;
            documents.insert(
                params.text_document.uri.to_string(),
                params.text_document.text,
            );
            Some(params.text_document.uri)
        }
        DidChangeTextDocument::METHOD => {
            // Full sync: the last change holds the whole document.
            let mut params = serde_json::from_value::<DidChangeTextDocumentParams>(params).ok()?;
            documents.insert(
                params.text_document.uri.to_string(),
                params.content_changes.pop()?.text,
            );
            Some(params.text_document.uri)
        }
        DidCloseTextDocument::METHOD => {
            let params = serde_json::from_value::<DidCloseTextDocumentParams>(params).ok()?;
            documents.remove(params.text_document.uri.as_str());
            Some(params.text_document.uri)
        }
        _ => None,
    }
}

/// Answers the requests we advertise, and rejects the rest.
fn respond(documents: &Documents, config: &Config, request: Request) -> Response {
    match request.method.as_str() {
        InlayHintRequest::METHOD => reply::<InlayHintRequest>(request, |params| {
            parse(documents, &params.text_document.uri)
                .map(|parsed| inlay_hints(&parsed, params.range, config))
        }),
        SemanticTokensFullRequest::METHOD => {
            reply::<SemanticTokensFullRequest>(request, |params| {
                parse(documents, &params.text_document.uri)
                    .map(|parsed| semantic_tokens(&parsed, config))
            })
        }
        _ => Response::new_err(request.id, ErrorCode::MethodNotFound as i32, request.method),
    }
}

/// Decodes `request`'s params as `R::Params` and answers with `handle`'s result.
fn reply<R: lsp_types::request::Request>(
    request: Request,
    handle: impl FnOnce(R::Params) -> R::Result,
) -> Response {
    match serde_json::from_value(request.params) {
        Ok(params) => Response::new_ok(request.id, handle(params)),
        Err(error) => Response::new_err(
            request.id,
            ErrorCode::InvalidParams as i32,
            error.to_string(),
        ),
    }
}

struct Parsed<'a> {
    source: &'a str,
    tree: tree_sitter::Tree,
    language: languages::Language,
}

fn parse<'a>(documents: &'a Documents, uri: &Uri) -> Option<Parsed<'a>> {
    let source = documents.get(uri.as_str())?;
    let language = languages::for_path(Path::new(uri.path().as_str()))?;
    let tree = language.parse(source)?;
    Some(Parsed {
        source,
        tree,
        language,
    })
}

/// A `complexity N/limit` hint at the end of the signature line of each function scoring at
/// least `hint_threshold`, with the explanation as its hover tooltip. In functions over the
/// limit, each hotspot also gets a `+N complexity` hint at the end of its first line.
fn inlay_hints(parsed: &Parsed, range: lsp_types::Range, config: &Config) -> Vec<InlayHint> {
    let limit = config.complexity_limit;
    let lines: Vec<&str> = parsed.source.lines().collect();
    let hint = |row: usize, label: String, tooltip: String| {
        let end = lines.get(row).map_or(0, |line| line.trim_end().len());
        InlayHint {
            position: Position::new(row as u32, utf16_column(&lines, row, end)),
            label: InlayHintLabel::String(label),
            kind: None,
            text_edits: None,
            tooltip: Some(InlayHintTooltip::String(tooltip)),
            padding_left: Some(true),
            padding_right: None,
            data: None,
        }
    };
    let mut hints = Vec::new();
    for function in complexity::functions(&parsed.tree, parsed.source, &parsed.language) {
        let score = function.score();
        if score >= config.hint_threshold {
            let row = function.anchor.start_point.row;
            let tooltip = explain(&function, parsed.source, limit);
            hints.push(hint(row, format!("complexity {score}/{limit}"), tooltip));
        }
        if score <= limit {
            continue;
        }
        for hotspot in function.hotspots() {
            let (label, from_nesting) = match hotspot.nesting {
                0 => (format!("+{} complexity", hotspot.cost), String::new()),
                nesting => (
                    format!("+{} complexity ({nesting} from nesting)", hotspot.cost),
                    format!(", {nesting} of it from nesting"),
                ),
            };
            let tooltip = format!(
                "This block adds {} to its function's complexity of {score} (limit {limit}){from_nesting}. \
                 Moving it into its own function takes that cost out of this one.",
                hotspot.cost
            );
            hints.push(hint(hotspot.range.start_point.row, label, tooltip));
        }
    }
    hints.retain(|hint| (range.start.line..=range.end.line).contains(&hint.position.line));
    hints
}

/// A warning on the name of each function scoring above the limit.
fn diagnostics(parsed: &Parsed, config: &Config) -> Vec<Diagnostic> {
    let lines: Vec<&str> = parsed.source.lines().collect();
    let position = |point: tree_sitter::Point| {
        Position::new(
            point.row as u32,
            utf16_column(&lines, point.row, point.column),
        )
    };
    complexity::functions(&parsed.tree, parsed.source, &parsed.language)
        .into_iter()
        .filter(|function| function.score() > config.complexity_limit)
        .map(|function| {
            let start = function.anchor.start_point;
            // Lambdas anchor on their parameters, which may span lines; underline the first one only.
            let end = match function.anchor.end_point {
                end if end.row == start.row => end,
                _ => tree_sitter::Point::new(start.row, usize::MAX),
            };
            Diagnostic {
                range: lsp_types::Range::new(position(start), position(end)),
                severity: Some(DiagnosticSeverity::WARNING),
                source: Some("better-highlights".into()),
                message: explain(&function, parsed.source, config.complexity_limit),
                ..Default::default()
            }
        })
        .collect()
}

/// Plain-text explanation of a score: the costliest blocks first, then how scoring works.
/// Zed shows diagnostics as plain text, so no Markdown.
fn explain(function: &FunctionComplexity, source: &str, limit: u32) -> String {
    const SHOWN: usize = 5;
    let hotspots = function.hotspots();
    let mut text = format!("Cognitive complexity {} (limit {limit}).", function.score());
    if hotspots.is_empty() {
        return text;
    }
    text.push_str(" Biggest contributors:");
    for hotspot in hotspots.iter().take(SHOWN) {
        let line = source[hotspot.range.start_byte..hotspot.range.end_byte]
            .lines()
            .next()
            .unwrap_or_default()
            .trim();
        text.push_str(&format!(
            "\n• {} from line {}: {}",
            hotspot.cost,
            hotspot.range.start_point.row + 1,
            shorten(line)
        ));
        if hotspot.nesting > 0 {
            text.push_str(&format!(" ({} of it from nesting)", hotspot.nesting));
        }
    }
    let rest = &hotspots[hotspots.len().min(SHOWN)..];
    if !rest.is_empty() {
        let cost: u32 = rest.iter().map(|hotspot| hotspot.cost).sum();
        text.push_str(&format!(
            "\n• {cost} more from {} smaller spots",
            rest.len()
        ));
    }
    text.push_str(
        "\n\nEach if, loop, switch, catch and ternary costs 1 plus how deeply it is nested. \
         Each else, else if and run of && or || costs 1. \
         Moving a block into its own function, or replacing it with an early return, removes its nesting cost.",
    );
    text
}

fn shorten(line: &str) -> String {
    const MAX: usize = 40;
    match line.char_indices().nth(MAX) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    }
}

fn semantic_tokens(parsed: &Parsed, config: &Config) -> SemanticTokensResult {
    let lines: Vec<&str> = parsed.source.lines().collect();
    let modifiers = config.modifiers();
    let mut previous = (0, 0);
    let data = comments::tagged_lines(&parsed.tree, parsed.source, &parsed.language, &config.tags)
        .into_iter()
        // Tags whose modifier didn't fit in the legend are skipped.
        .filter_map(|tagged| {
            let bit = modifiers
                .iter()
                .position(|modifier| *modifier == tagged.tag.modifier)?;
            Some((tagged, bit))
        })
        .map(|(tagged, bit)| {
            let start = utf16_column(&lines, tagged.row, tagged.start);
            let length = utf16_column(&lines, tagged.row, tagged.end) - start;
            let row = tagged.row as u32;
            let (previous_row, previous_start) = previous;
            previous = (row, start);
            SemanticToken {
                delta_line: row - previous_row,
                delta_start: if row == previous_row {
                    start - previous_start
                } else {
                    start
                },
                length,
                token_type: 0,
                token_modifiers_bitset: 1 << bit,
            }
        })
        .collect();
    SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    })
}

/// The UTF-16 column (what LSP positions count) of byte `column` on `row`, clamped to the line.
fn utf16_column(lines: &[&str], row: usize, column: usize) -> u32 {
    let line = lines.get(row).copied().unwrap_or_default();
    line[..column.min(line.len())].encode_utf16().count() as u32
}
