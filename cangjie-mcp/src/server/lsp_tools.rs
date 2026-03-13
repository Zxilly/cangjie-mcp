use anyhow::Result;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::lsp::client::{CangjieClient, DiagnosticsStatus, SupportedOperation};
use crate::lsp::tools as lsp_tools;
use crate::lsp::tools::{SymbolOutput, SymbolsResult};
use crate::server::tools::CangjieServer;

impl From<LspOperation> for SupportedOperation {
    fn from(op: LspOperation) -> Self {
        match op {
            LspOperation::Definition => SupportedOperation::Definition,
            LspOperation::References => SupportedOperation::References,
            LspOperation::Hover => SupportedOperation::Hover,
            LspOperation::DocumentSymbol => SupportedOperation::DocumentSymbol,
            LspOperation::Diagnostics => SupportedOperation::Diagnostics,
            LspOperation::WorkspaceSymbol => SupportedOperation::WorkspaceSymbol,
            LspOperation::IncomingCalls => SupportedOperation::IncomingCalls,
            LspOperation::OutgoingCalls => SupportedOperation::OutgoingCalls,
            LspOperation::TypeSupertypes => SupportedOperation::TypeSupertypes,
            LspOperation::TypeSubtypes => SupportedOperation::TypeSubtypes,
            LspOperation::Rename => SupportedOperation::Rename,
            LspOperation::Completion => SupportedOperation::Completion,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LspOperation {
    Definition,
    References,
    Hover,
    DocumentSymbol,
    Diagnostics,
    WorkspaceSymbol,
    IncomingCalls,
    OutgoingCalls,
    TypeSupertypes,
    TypeSubtypes,
    Rename,
    Completion,
}

impl LspOperation {
    fn requires_file_path(self) -> bool {
        !matches!(self, Self::WorkspaceSymbol)
    }

    fn requires_target(self) -> bool {
        matches!(
            self,
            Self::Definition
                | Self::References
                | Self::Hover
                | Self::IncomingCalls
                | Self::OutgoingCalls
                | Self::TypeSupertypes
                | Self::TypeSubtypes
                | Self::Rename
                | Self::Completion
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LspTarget {
    Position {
        line: u32,
        character: u32,
    },
    Symbol {
        symbol: String,
        #[serde(default)]
        line_hint: Option<u32>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
pub struct LspRequest {
    pub operation: LspOperation,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub target: Option<LspTarget>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub new_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub file_path: String,
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LspResponseStatus {
    Ok,
    Empty,
    Unsupported,
    Timeout,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct LspResponse {
    pub operation: LspOperation,
    pub status: LspResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_target: Option<ResolvedTarget>,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedPosition {
    zero_based_line: u32,
    zero_based_character: u32,
    display: ResolvedTarget,
}

fn empty_data() -> Value {
    json!({})
}

fn status_from_count(count: usize) -> LspResponseStatus {
    if count == 0 {
        LspResponseStatus::Empty
    } else {
        LspResponseStatus::Ok
    }
}

fn serialize_response(response: &LspResponse) -> String {
    serde_json::to_string_pretty(response).unwrap_or_else(|e| {
        format!("{{\"status\":\"error\",\"message\":\"Serialization error: {e}\"}}")
    })
}

fn response_with_data<T: Serialize>(
    operation: LspOperation,
    status: LspResponseStatus,
    resolved_target: Option<ResolvedTarget>,
    data: &T,
    message: Option<String>,
) -> String {
    let data = serde_json::to_value(data).unwrap_or_else(|_| empty_data());
    serialize_response(&LspResponse {
        operation,
        status,
        resolved_target,
        data,
        message,
    })
}

fn status_response(
    operation: LspOperation,
    status: LspResponseStatus,
    message: impl Into<String>,
) -> String {
    serialize_response(&LspResponse {
        operation,
        status,
        resolved_target: None,
        data: empty_data(),
        message: Some(message.into()),
    })
}

fn error_response(operation: LspOperation, message: impl Into<String>) -> String {
    status_response(operation, LspResponseStatus::Error, message)
}

fn unsupported_response(operation: LspOperation, message: impl Into<String>) -> String {
    status_response(operation, LspResponseStatus::Unsupported, message)
}

fn validate_request(params: &LspRequest) -> Result<(), String> {
    if params.operation.requires_file_path() {
        let Some(file_path) = params.file_path.as_deref() else {
            return Err("file_path is required for this operation".to_string());
        };
        if let Some(err) = lsp_tools::get_validate_error(file_path) {
            return Err(err);
        }
    }

    if params.operation.requires_target() && params.target.is_none() {
        return Err("target is required for this operation".to_string());
    }

    if matches!(params.operation, LspOperation::WorkspaceSymbol)
        && params
            .query
            .as_deref()
            .is_none_or(|query| query.trim().is_empty())
    {
        return Err("query is required for workspace_symbol".to_string());
    }

    if matches!(params.operation, LspOperation::Rename)
        && params
            .new_name
            .as_deref()
            .is_none_or(|new_name| new_name.trim().is_empty())
    {
        return Err("new_name is required for rename".to_string());
    }

    if matches!(params.operation, LspOperation::Completion)
        && !matches!(params.target, Some(LspTarget::Position { .. }))
    {
        return Err("completion requires target.kind=position".to_string());
    }

    Ok(())
}

fn collect_symbol_matches(symbols: &[SymbolOutput], name: &str, out: &mut Vec<(u32, u32)>) {
    for symbol in symbols {
        let is_match = symbol.name == name
            || (symbol.name.starts_with(name)
                && symbol.name.as_bytes().get(name.len()) == Some(&b'('));
        if is_match {
            out.push((symbol.line, symbol.character));
        }
        if let Some(children) = symbol.children.as_deref() {
            collect_symbol_matches(children, name, out);
        }
    }
}

fn select_symbol_match(
    symbols: &SymbolsResult,
    symbol: &str,
    line_hint: Option<u32>,
    file_path: &str,
) -> Result<(u32, u32), String> {
    let mut matches = Vec::new();
    collect_symbol_matches(&symbols.symbols, symbol, &mut matches);

    if matches.is_empty() {
        let available: Vec<String> = symbols
            .symbols
            .iter()
            .map(|item| item.name.clone())
            .collect();
        return Err(format!(
            "Symbol '{}' not found in {}. Available: {:?}",
            symbol, file_path, available
        ));
    }

    if matches.len() == 1 {
        return Ok(matches[0]);
    }

    if let Some(line_hint) = line_hint {
        return Ok(*matches
            .iter()
            .min_by_key(|(line, _)| (*line as i64 - line_hint as i64).unsigned_abs())
            .expect("matches is not empty"));
    }

    Err(format!(
        "Symbol '{}' appears {} times (lines: {:?}). Provide target.line_hint to disambiguate.",
        symbol,
        matches.len(),
        matches.iter().map(|(line, _)| *line).collect::<Vec<_>>()
    ))
}

async fn resolve_target_position(
    client: &CangjieClient,
    file_path: &str,
    target: &LspTarget,
) -> Result<ResolvedPosition, String> {
    match target {
        LspTarget::Position { line, character } => {
            if *line == 0 || *character == 0 {
                return Err(
                    "target.line and target.character must be 1-based positive integers"
                        .to_string(),
                );
            }
            Ok(ResolvedPosition {
                zero_based_line: line - 1,
                zero_based_character: character - 1,
                display: ResolvedTarget {
                    file_path: file_path.to_string(),
                    line: *line,
                    character: *character,
                },
            })
        }
        LspTarget::Symbol { symbol, line_hint } => {
            let result = client
                .document_symbol(file_path)
                .await
                .map_err(|e| format!("Failed to get symbols: {e}"))?;
            let symbols = lsp_tools::process_symbols(&result, file_path);
            let (line, character) = select_symbol_match(&symbols, symbol, *line_hint, file_path)?;
            Ok(ResolvedPosition {
                zero_based_line: line - 1,
                zero_based_character: character - 1,
                display: ResolvedTarget {
                    file_path: file_path.to_string(),
                    line,
                    character,
                },
            })
        }
    }
}

fn lsp_unavailable_message() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    if crate::lsp::detect_settings(Some(cwd)).is_none() {
        return "LSP is not available: CANGJIE_HOME is not configured. Set CANGJIE_HOME (and optionally CANGJIE_PATH) in environment variables.".to_string();
    }

    "LSP is not available: client is not initialized or failed to start. Check startup logs for 'LSP startup' and 'Failed to initialize LSP client'.".to_string()
}

impl CangjieServer {
    pub(crate) fn lsp_tool_router() -> ToolRouter<Self> {
        ToolRouter::<Self>::new().with_route((Self::lsp_tool_attr(), Self::lsp))
    }
}

pub(crate) async fn execute_lsp_request(params: LspRequest) -> String {
    if let Err(message) = validate_request(&params) {
        return error_response(params.operation, message);
    }

    let guard = crate::lsp::get_client().await;
    let client = match guard {
        Some(ref inner) => match inner.as_ref() {
            Some(client) => client,
            None => return error_response(params.operation, lsp_unavailable_message()),
        },
        None => return error_response(params.operation, lsp_unavailable_message()),
    };

    if !client.supports(params.operation.into()) {
        return unsupported_response(
            params.operation,
            format!(
                "LSP server does not advertise support for {:?}",
                params.operation
            ),
        );
    }

    let file_path = params.file_path.as_deref();
    let mut resolved_target = None;

    let resolved_position =
        if let (Some(file_path), Some(target)) = (file_path, params.target.as_ref()) {
            match resolve_target_position(client, file_path, target).await {
                Ok(position) => {
                    resolved_target = Some(position.display.clone());
                    Some(position)
                }
                Err(message) => return error_response(params.operation, message),
            }
        } else {
            None
        };

    match params.operation {
        LspOperation::Definition => {
            let position = resolved_position.expect("validated target");
            match client
                .definition(
                    file_path.expect("validated file"),
                    position.zero_based_line,
                    position.zero_based_character,
                )
                .await
            {
                Ok(result) => {
                    let data = lsp_tools::process_definition(&result);
                    response_with_data(
                        params.operation,
                        status_from_count(data.count),
                        resolved_target,
                        &data,
                        None,
                    )
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }
        LspOperation::References => {
            let position = resolved_position.expect("validated target");
            match client
                .references(
                    file_path.expect("validated file"),
                    position.zero_based_line,
                    position.zero_based_character,
                )
                .await
            {
                Ok(result) => {
                    let data = lsp_tools::process_references(&result);
                    response_with_data(
                        params.operation,
                        status_from_count(data.count),
                        resolved_target,
                        &data,
                        None,
                    )
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }
        LspOperation::Hover => {
            let position = resolved_position.expect("validated target");
            match client
                .hover(
                    file_path.expect("validated file"),
                    position.zero_based_line,
                    position.zero_based_character,
                )
                .await
            {
                Ok(result) => {
                    match lsp_tools::parse_hover(&result, file_path.expect("validated file")) {
                        None => response_with_data(
                            params.operation,
                            LspResponseStatus::Empty,
                            resolved_target,
                            &lsp_tools::HoverOutput::default(),
                            None,
                        ),
                        Some(data) => response_with_data(
                            params.operation,
                            LspResponseStatus::Ok,
                            resolved_target,
                            &data,
                            None,
                        ),
                    }
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }
        LspOperation::DocumentSymbol => match client
            .document_symbol(file_path.expect("validated file"))
            .await
        {
            Ok(result) => {
                let data = lsp_tools::process_symbols(&result, file_path.expect("validated file"));
                response_with_data(
                    params.operation,
                    status_from_count(data.count),
                    None,
                    &data,
                    None,
                )
            }
            Err(error) => error_response(params.operation, format!("Error: {error}")),
        },
        LspOperation::Diagnostics => match client
            .get_diagnostics(file_path.expect("validated file"))
            .await
        {
            Ok(result) => {
                let data = lsp_tools::process_diagnostics(&result.diagnostics);
                let status = match result.status {
                    DiagnosticsStatus::Timeout => LspResponseStatus::Timeout,
                    DiagnosticsStatus::Ready if data.diagnostics.is_empty() => {
                        LspResponseStatus::Empty
                    }
                    DiagnosticsStatus::Ready => LspResponseStatus::Ok,
                };
                let message = if matches!(result.status, DiagnosticsStatus::Timeout) {
                    Some("Timed out waiting for fresh diagnostics; returning the latest available diagnostics.".to_string())
                } else {
                    None
                };
                response_with_data(params.operation, status, None, &data, message)
            }
            Err(error) => error_response(params.operation, format!("Error: {error}")),
        },
        LspOperation::WorkspaceSymbol => match client
            .workspace_symbol(params.query.as_deref().unwrap_or_default())
            .await
        {
            Ok(result) => {
                let data = lsp_tools::process_workspace_symbols(&result);
                response_with_data(
                    params.operation,
                    status_from_count(data.count),
                    None,
                    &data,
                    None,
                )
            }
            Err(error) => error_response(params.operation, format!("Error: {error}")),
        },
        LspOperation::IncomingCalls => {
            let position = resolved_position.expect("validated target");
            match client
                .incoming_calls(
                    file_path.expect("validated file"),
                    position.zero_based_line,
                    position.zero_based_character,
                )
                .await
            {
                Ok(result) => {
                    let data = lsp_tools::process_incoming_calls(&result);
                    response_with_data(
                        params.operation,
                        status_from_count(data.count),
                        resolved_target,
                        &data,
                        None,
                    )
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }
        LspOperation::OutgoingCalls => {
            let position = resolved_position.expect("validated target");
            match client
                .outgoing_calls(
                    file_path.expect("validated file"),
                    position.zero_based_line,
                    position.zero_based_character,
                )
                .await
            {
                Ok(result) => {
                    let data = lsp_tools::process_outgoing_calls(&result);
                    response_with_data(
                        params.operation,
                        status_from_count(data.count),
                        resolved_target,
                        &data,
                        None,
                    )
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }
        LspOperation::TypeSupertypes => {
            let position = resolved_position.expect("validated target");
            match client
                .type_supertypes(
                    file_path.expect("validated file"),
                    position.zero_based_line,
                    position.zero_based_character,
                )
                .await
            {
                Ok(result) => {
                    let data = lsp_tools::process_type_hierarchy(&result);
                    response_with_data(
                        params.operation,
                        status_from_count(data.count),
                        resolved_target,
                        &data,
                        None,
                    )
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }
        LspOperation::TypeSubtypes => {
            let position = resolved_position.expect("validated target");
            match client
                .type_subtypes(
                    file_path.expect("validated file"),
                    position.zero_based_line,
                    position.zero_based_character,
                )
                .await
            {
                Ok(result) => {
                    let data = lsp_tools::process_type_hierarchy(&result);
                    response_with_data(
                        params.operation,
                        status_from_count(data.count),
                        resolved_target,
                        &data,
                        None,
                    )
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }
        LspOperation::Rename => {
            let position = resolved_position.expect("validated target");
            match client
                .rename(
                    file_path.expect("validated file"),
                    position.zero_based_line,
                    position.zero_based_character,
                    params.new_name.as_deref().unwrap_or_default(),
                )
                .await
            {
                Ok(result) => {
                    let data = lsp_tools::process_rename(&result);
                    response_with_data(
                        params.operation,
                        status_from_count(data.edit_count),
                        resolved_target,
                        &data,
                        None,
                    )
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }
        LspOperation::Completion => {
            let position = resolved_position.expect("validated target");
            match client
                .completion(
                    file_path.expect("validated file"),
                    position.zero_based_line,
                    position.zero_based_character,
                )
                .await
            {
                Ok(result) => {
                    let data = lsp_tools::process_completion(&result);
                    response_with_data(
                        params.operation,
                        status_from_count(data.count),
                        resolved_target,
                        &data,
                        None,
                    )
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_symbols() -> SymbolsResult {
        SymbolsResult {
            symbols: vec![
                SymbolOutput {
                    name: "main".to_string(),
                    kind: "function".to_string(),
                    line: 4,
                    character: 3,
                    end_line: 6,
                    end_character: 1,
                    children: None,
                },
                SymbolOutput {
                    name: "Widget".to_string(),
                    kind: "class".to_string(),
                    line: 10,
                    character: 1,
                    end_line: 20,
                    end_character: 1,
                    children: Some(vec![
                        SymbolOutput {
                            name: "render".to_string(),
                            kind: "method".to_string(),
                            line: 12,
                            character: 5,
                            end_line: 14,
                            end_character: 1,
                            children: None,
                        },
                        SymbolOutput {
                            name: "render".to_string(),
                            kind: "method".to_string(),
                            line: 16,
                            character: 5,
                            end_line: 18,
                            end_character: 1,
                            children: None,
                        },
                    ]),
                },
            ],
            count: 2,
        }
    }

    #[test]
    fn test_validate_workspace_symbol_requires_query() {
        let params = LspRequest {
            operation: LspOperation::WorkspaceSymbol,
            file_path: None,
            target: None,
            query: None,
            new_name: None,
        };
        assert!(validate_request(&params).is_err());
    }

    #[test]
    fn test_validate_completion_requires_position_target() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let file_path = temp_dir.path().join("main.cj");
        std::fs::write(&file_path, "main() {}").unwrap();
        let params = LspRequest {
            operation: LspOperation::Completion,
            file_path: Some(file_path.to_string_lossy().to_string()),
            target: Some(LspTarget::Symbol {
                symbol: "main".to_string(),
                line_hint: None,
            }),
            query: None,
            new_name: None,
        };
        assert_eq!(
            validate_request(&params).unwrap_err(),
            "completion requires target.kind=position"
        );
    }

    #[test]
    fn test_select_symbol_match_unique() {
        let result = select_symbol_match(&sample_symbols(), "main", None, "/tmp/main.cj").unwrap();
        assert_eq!(result, (4, 3));
    }

    #[test]
    fn test_select_symbol_match_with_line_hint() {
        let result =
            select_symbol_match(&sample_symbols(), "render", Some(15), "/tmp/main.cj").unwrap();
        assert_eq!(result, (16, 5));
    }

    #[test]
    fn test_select_symbol_match_requires_disambiguation() {
        let error =
            select_symbol_match(&sample_symbols(), "render", None, "/tmp/main.cj").unwrap_err();
        assert!(error.contains("Provide target.line_hint"));
    }

    #[test]
    fn test_response_serialization_uses_timeout_status() {
        let serialized = response_with_data(
            LspOperation::Diagnostics,
            LspResponseStatus::Timeout,
            None,
            &json!({ "diagnostics": [] }),
            Some("timeout".to_string()),
        );
        let parsed: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed["status"], "timeout");
    }
}
