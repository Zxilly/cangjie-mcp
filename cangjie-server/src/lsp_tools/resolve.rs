#[cfg(feature = "lsp")]
use cangjie_lsp::client::CangjieClient;
#[cfg(feature = "lsp")]
use cangjie_lsp::tools as lsp_tools;
#[cfg(feature = "lsp")]
use cangjie_lsp::tools::{SymbolOutput, SymbolsResult};

#[cfg(feature = "lsp")]
use super::types::{LspOperation, LspRequest, LspTarget, ResolvedPosition, ResolvedTarget};

#[cfg(feature = "lsp")]
pub(crate) fn validate_request(params: &LspRequest) -> Result<(), String> {
    let op_name = format!("{:?}", params.operation).to_lowercase();

    // Phase 1: required parameters present
    if params.operation.requires_file_path() && params.file_path.is_none() {
        return Err(format!(
            "file_path is required for {op_name}. Provide an absolute path to a .cj file, e.g. {{\"file_path\": \"/path/to/file.cj\"}}"
        ));
    }

    if params.operation.requires_target() && params.target.is_none() {
        return Err(format!(
            "target is required for {op_name}. Use {{\"kind\": \"symbol\", \"symbol\": \"name\"}} or {{\"kind\": \"position\", \"line\": 1, \"character\": 1}}"
        ));
    }

    if matches!(params.operation, LspOperation::WorkspaceSymbol)
        && params
            .query
            .as_deref()
            .is_none_or(|query| query.trim().is_empty())
    {
        return Err(
            "query is required for workspace_symbol. Provide a symbol name to search, e.g. {\"query\": \"MyClass\"}"
                .to_string(),
        );
    }

    // Phase 2: validate parameter values (e.g. file exists on disk)
    if let Some(file_path) = params.file_path.as_deref() {
        if params.operation.requires_file_path() {
            if let Some(err) = lsp_tools::get_validate_error(file_path) {
                return Err(err);
            }
        }
    }

    Ok(())
}

#[cfg(feature = "lsp")]
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

#[cfg(feature = "lsp")]
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

#[cfg(feature = "lsp")]
pub(crate) async fn resolve_target_position(
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

#[cfg(feature = "lsp")]
pub(crate) fn lsp_unavailable_message() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    if cangjie_lsp::detect_settings(Some(cwd)).is_none() {
        return "LSP is not available: CANGJIE_HOME is not configured. Set CANGJIE_HOME (and optionally CANGJIE_PATH) in environment variables.".to_string();
    }

    "LSP is not available: client is not initialized or failed to start. Check startup logs for 'LSP startup' and 'Failed to initialize LSP client'.".to_string()
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "lsp")]
    use super::*;

    #[cfg(feature = "lsp")]
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

    #[cfg(feature = "lsp")]
    #[test]
    fn test_validate_workspace_symbol_requires_query() {
        let params = LspRequest {
            operation: LspOperation::WorkspaceSymbol,
            file_path: None,
            target: None,
            query: None,
        };
        assert!(validate_request(&params).is_err());
    }

    #[cfg(feature = "lsp")]
    #[test]
    fn test_select_symbol_match_unique() {
        let result = select_symbol_match(&sample_symbols(), "main", None, "/tmp/main.cj").unwrap();
        assert_eq!(result, (4, 3));
    }

    #[cfg(feature = "lsp")]
    #[test]
    fn test_select_symbol_match_with_line_hint() {
        let result =
            select_symbol_match(&sample_symbols(), "render", Some(15), "/tmp/main.cj").unwrap();
        assert_eq!(result, (16, 5));
    }

    #[cfg(feature = "lsp")]
    #[test]
    fn test_select_symbol_match_requires_disambiguation() {
        let error =
            select_symbol_match(&sample_symbols(), "render", None, "/tmp/main.cj").unwrap_err();
        assert!(error.contains("Provide target.line_hint"));
    }
}
