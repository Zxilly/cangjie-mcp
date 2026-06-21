use serde_json::Value;

use crate::types::{
    CallHierarchyIncomingCall, CallHierarchyOutgoingCall, Diagnostic, DiagnosticSeverity,
    DocumentSymbolResponse, GotoDefinitionResponse, Hover, HoverContents, Location, MarkedString,
    TypeHierarchyItem,
};

use crate::utils::uri_to_path;

use super::convert::{
    convert_call_hierarchy_item, convert_document_symbol, convert_type_hierarchy_item,
    extract_diagnostic_code, location_link_to_result, location_to_result, severity_name,
    symbol_kind_name, symbol_kind_name_by_number, validate_file_path,
};
use super::types::{
    DefinitionResult, DiagnosticOutput, DiagnosticsResult, HoverOutput, IncomingCallOutput,
    IncomingCallsResult, LocationResult, OutgoingCallOutput, OutgoingCallsResult, ReferencesResult,
    SymbolOutput, SymbolsResult, TypeHierarchyItemOutput, TypeHierarchyResult,
    WorkspaceSymbolOutput, WorkspaceSymbolResult,
};

pub fn process_definition(result: &Value) -> DefinitionResult {
    let response: Option<GotoDefinitionResponse> = serde_json::from_value(result.clone()).ok();

    let locations = match response {
        Some(GotoDefinitionResponse::Scalar(loc)) => vec![location_to_result(&loc)],
        Some(GotoDefinitionResponse::Array(locs)) => locs.iter().map(location_to_result).collect(),
        Some(GotoDefinitionResponse::Link(links)) => {
            links.iter().map(location_link_to_result).collect()
        }
        None => Vec::new(),
    };

    let count = locations.len();
    DefinitionResult { locations, count }
}

pub fn process_references(result: &Value) -> ReferencesResult {
    let locations: Vec<Location> = serde_json::from_value(result.clone()).unwrap_or_default();
    let locations: Vec<LocationResult> = locations.iter().map(location_to_result).collect();
    let count = locations.len();
    ReferencesResult { locations, count }
}

pub fn parse_hover(result: &Value, file_path: &str) -> Option<HoverOutput> {
    let hover: Hover = serde_json::from_value(result.clone()).ok()?;

    let content = match hover.contents {
        HoverContents::Scalar(marked) => match marked {
            MarkedString::String(s) => s,
            MarkedString::LanguageString(ls) => ls.value,
        },
        HoverContents::Markup(mc) => mc.value,
        HoverContents::Array(arr) => {
            let parts: Vec<String> = arr
                .into_iter()
                .map(|item| match item {
                    MarkedString::String(s) => s,
                    MarkedString::LanguageString(ls) => ls.value,
                })
                .collect();
            parts.join("\n\n")
        }
    };

    let range = hover.range.map(|r| LocationResult {
        file_path: file_path.to_string(),
        line: r.start.line + 1,
        character: r.start.character + 1,
        end_line: Some(r.end.line + 1),
        end_character: Some(r.end.character + 1),
    });

    Some(HoverOutput { content, range })
}

pub fn process_hover(result: &Value, file_path: &str) -> String {
    let output = parse_hover(result, file_path).unwrap_or_else(|| HoverOutput {
        content: "No hover information available".to_string(),
        range: None,
    });
    serde_json::to_string_pretty(&output).unwrap_or_else(|e| format!("Serialization error: {e}"))
}

pub fn process_symbols(result: &Value, _file_path: &str) -> SymbolsResult {
    let response: Option<DocumentSymbolResponse> = serde_json::from_value(result.clone()).ok();

    let symbols = match response {
        Some(DocumentSymbolResponse::Flat(sym_infos)) => sym_infos
            .iter()
            .map(|si| SymbolOutput {
                name: si.name.clone(),
                kind: symbol_kind_name(si.kind).to_string(),
                line: si.location.range.start.line + 1,
                character: si.location.range.start.character + 1,
                end_line: si.location.range.end.line + 1,
                end_character: si.location.range.end.character + 1,
                children: None,
            })
            .collect(),
        Some(DocumentSymbolResponse::Nested(doc_syms)) => {
            doc_syms.iter().map(convert_document_symbol).collect()
        }
        None => Vec::new(),
    };

    let count = symbols.len();
    SymbolsResult { symbols, count }
}

pub fn process_diagnostics(diags: &[Value]) -> DiagnosticsResult {
    let typed_diags: Vec<Diagnostic> = diags
        .iter()
        .filter_map(|v| serde_json::from_value(v.clone()).ok())
        .collect();

    let mut diagnostics = Vec::new();
    let mut error_count = 0;
    let mut warning_count = 0;
    let mut info_count = 0;
    let mut hint_count = 0;

    for diag in &typed_diags {
        match diag.severity {
            Some(DiagnosticSeverity::ERROR) => error_count += 1,
            Some(DiagnosticSeverity::WARNING) => warning_count += 1,
            Some(DiagnosticSeverity::INFORMATION) => info_count += 1,
            Some(DiagnosticSeverity::HINT) => hint_count += 1,
            _ => {}
        }

        diagnostics.push(DiagnosticOutput {
            message: diag.message.clone(),
            severity: severity_name(diag.severity).to_string(),
            line: diag.range.start.line + 1,
            character: diag.range.start.character + 1,
            end_line: diag.range.end.line + 1,
            end_character: diag.range.end.character + 1,
            code: diag.code.as_ref().map(extract_diagnostic_code),
            source: diag.source.clone(),
        });
    }

    DiagnosticsResult {
        diagnostics,
        error_count,
        warning_count,
        info_count,
        hint_count,
    }
}

pub fn process_workspace_symbols(result: &Value) -> WorkspaceSymbolResult {
    // workspace/symbol can return SymbolInformation[] or WorkspaceSymbol[]
    let empty = [];
    let arr: &[Value] = result.as_array().map(Vec::as_slice).unwrap_or(&empty);
    let symbols: Vec<WorkspaceSymbolOutput> = arr
        .iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_string();
            let kind_num = item.get("kind")?.as_u64()? as u32;
            let kind = symbol_kind_name_by_number(kind_num).to_string();
            let container_name = item
                .get("containerName")
                .and_then(|v| v.as_str())
                .map(String::from);

            // SymbolInformation has location.uri + location.range
            let (file_path, line, character) = if let Some(loc) = item.get("location") {
                let uri = loc.get("uri")?.as_str()?;
                let range = loc.get("range")?;
                let start = range.get("start")?;
                (
                    uri_to_path(uri).to_string_lossy().to_string(),
                    start.get("line")?.as_u64()? as u32 + 1,
                    start.get("character")?.as_u64()? as u32 + 1,
                )
            } else {
                // WorkspaceSymbol has uri + range directly
                let uri = item.get("uri")?.as_str()?;
                let range = item.get("range")?;
                let start = range.get("start")?;
                (
                    uri_to_path(uri).to_string_lossy().to_string(),
                    start.get("line")?.as_u64()? as u32 + 1,
                    start.get("character")?.as_u64()? as u32 + 1,
                )
            };

            Some(WorkspaceSymbolOutput {
                name,
                kind,
                file_path,
                line,
                character,
                container_name,
            })
        })
        .collect();

    let count = symbols.len();
    WorkspaceSymbolResult { symbols, count }
}

pub fn process_incoming_calls(result: &Value) -> IncomingCallsResult {
    let calls: Vec<CallHierarchyIncomingCall> =
        serde_json::from_value(result.clone()).unwrap_or_default();
    let calls: Vec<IncomingCallOutput> = calls
        .iter()
        .map(|call| {
            let call_sites = call
                .from_ranges
                .iter()
                .map(|range| LocationResult {
                    file_path: uri_to_path(call.from.uri.as_str())
                        .to_string_lossy()
                        .to_string(),
                    line: range.start.line + 1,
                    character: range.start.character + 1,
                    end_line: Some(range.end.line + 1),
                    end_character: Some(range.end.character + 1),
                })
                .collect();
            IncomingCallOutput {
                from: convert_call_hierarchy_item(&call.from),
                call_sites,
            }
        })
        .collect();
    let count = calls.len();
    IncomingCallsResult { calls, count }
}

pub fn process_outgoing_calls(result: &Value) -> OutgoingCallsResult {
    let calls: Vec<CallHierarchyOutgoingCall> =
        serde_json::from_value(result.clone()).unwrap_or_default();
    let calls: Vec<OutgoingCallOutput> = calls
        .iter()
        .map(|call| {
            let call_sites = call
                .from_ranges
                .iter()
                .map(|range| LocationResult {
                    file_path: uri_to_path(call.to.uri.as_str())
                        .to_string_lossy()
                        .to_string(),
                    line: range.start.line + 1,
                    character: range.start.character + 1,
                    end_line: Some(range.end.line + 1),
                    end_character: Some(range.end.character + 1),
                })
                .collect();
            OutgoingCallOutput {
                to: convert_call_hierarchy_item(&call.to),
                call_sites,
            }
        })
        .collect();
    let count = calls.len();
    OutgoingCallsResult { calls, count }
}

pub fn process_type_hierarchy(result: &Value) -> TypeHierarchyResult {
    let items: Vec<TypeHierarchyItem> = serde_json::from_value(result.clone()).unwrap_or_default();
    let items: Vec<TypeHierarchyItemOutput> =
        items.iter().map(convert_type_hierarchy_item).collect();
    let count = items.len();
    TypeHierarchyResult { items, count }
}

pub fn get_validate_error(file_path: &str) -> Option<String> {
    validate_file_path(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_process_definition_array() {
        let result = json!([{
            "uri": "file:///test/main.cj",
            "range": {
                "start": {"line": 10, "character": 5},
                "end": {"line": 10, "character": 15}
            }
        }]);
        let def = process_definition(&result);
        assert_eq!(def.count, 1);
        assert_eq!(def.locations[0].line, 11);
        assert_eq!(def.locations[0].character, 6);
    }

    #[test]
    fn test_process_definition_single_object() {
        let result = json!({
            "uri": "file:///test/main.cj",
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 0, "character": 10}
            }
        });
        let def = process_definition(&result);
        assert_eq!(def.count, 1);
    }

    #[test]
    fn test_process_definition_location_link() {
        let result = json!([{
            "targetUri": "file:///test/lib.cj",
            "targetRange": {
                "start": {"line": 5, "character": 0},
                "end": {"line": 5, "character": 20}
            },
            "targetSelectionRange": {
                "start": {"line": 5, "character": 0},
                "end": {"line": 5, "character": 20}
            }
        }]);
        let def = process_definition(&result);
        assert_eq!(def.count, 1);
        assert_eq!(def.locations[0].line, 6);
    }

    #[test]
    fn test_process_definition_empty() {
        let def = process_definition(&json!(null));
        assert_eq!(def.count, 0);
    }

    #[test]
    fn test_process_references() {
        let result = json!([
            {
                "uri": "file:///a.cj",
                "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 5}}
            },
            {
                "uri": "file:///b.cj",
                "range": {"start": {"line": 2, "character": 0}, "end": {"line": 2, "character": 5}}
            }
        ]);
        let refs = process_references(&result);
        assert_eq!(refs.count, 2);
    }

    #[test]
    fn test_process_hover_with_content() {
        let result = json!({
            "contents": {"kind": "markdown", "value": "func main()"},
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 0, "character": 10}
            }
        });
        let output = process_hover(&result, "test.cj");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["content"], "func main()");
    }

    #[test]
    fn test_process_hover_null() {
        let output = process_hover(&json!(null), "test.cj");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed["content"].as_str().unwrap().contains("No hover"));
    }

    #[test]
    fn test_process_hover_string_contents() {
        let result = json!({"contents": "Simple hover text"});
        let output = process_hover(&result, "test.cj");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["content"], "Simple hover text");
    }

    #[test]
    fn test_process_symbols() {
        let result = json!([{
            "name": "main",
            "kind": 12,
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 5, "character": 1}
            },
            "selectionRange": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 0, "character": 4}
            }
        }]);
        let syms = process_symbols(&result, "test.cj");
        assert_eq!(syms.count, 1);
        assert_eq!(syms.symbols[0].name, "main");
        assert_eq!(syms.symbols[0].kind, "function");
    }

    #[test]
    fn test_process_symbols_with_children() {
        let result = json!([{
            "name": "MyClass",
            "kind": 5,
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 10, "character": 1}
            },
            "selectionRange": {
                "start": {"line": 0, "character": 6},
                "end": {"line": 0, "character": 13}
            },
            "children": [{
                "name": "method",
                "kind": 6,
                "range": {
                    "start": {"line": 2, "character": 4},
                    "end": {"line": 4, "character": 5}
                },
                "selectionRange": {
                    "start": {"line": 2, "character": 4},
                    "end": {"line": 2, "character": 10}
                }
            }]
        }]);
        let syms = process_symbols(&result, "test.cj");
        assert_eq!(syms.symbols[0].name, "MyClass");
        assert_eq!(syms.symbols[0].kind, "class");
        let children = syms.symbols[0].children.as_ref().unwrap();
        assert_eq!(children[0].name, "method");
        assert_eq!(children[0].kind, "method");
    }

    #[test]
    fn test_process_diagnostics() {
        let diags = vec![
            json!({
                "message": "Type mismatch",
                "severity": 1,
                "range": {
                    "start": {"line": 5, "character": 10},
                    "end": {"line": 5, "character": 20}
                },
                "code": "E001",
                "source": "cangjie"
            }),
            json!({
                "message": "Unused variable",
                "severity": 2,
                "range": {
                    "start": {"line": 3, "character": 0},
                    "end": {"line": 3, "character": 5}
                }
            }),
        ];
        let result = process_diagnostics(&diags);
        assert_eq!(result.error_count, 1);
        assert_eq!(result.warning_count, 1);
        assert_eq!(result.diagnostics.len(), 2);
        assert_eq!(result.diagnostics[0].severity, "error");
        assert_eq!(result.diagnostics[0].code, Some("E001".to_string()));
        assert_eq!(result.diagnostics[1].severity, "warning");
    }

    #[test]
    fn test_process_hover_array_contents() {
        let result = json!({
            "contents": [
                "First part",
                {"language": "cangjie", "value": "func foo()"}
            ]
        });
        let output = process_hover(&result, "test.cj");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let content = parsed["content"].as_str().unwrap();
        assert!(content.contains("First part"));
        assert!(content.contains("func foo()"));
    }

    #[test]
    fn test_process_diagnostics_info_and_hint() {
        let diags = vec![
            json!({
                "message": "Info diagnostic",
                "severity": 3,
                "range": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 0, "character": 5}
                }
            }),
            json!({
                "message": "Hint diagnostic",
                "severity": 4,
                "range": {
                    "start": {"line": 1, "character": 0},
                    "end": {"line": 1, "character": 5}
                }
            }),
        ];
        let result = process_diagnostics(&diags);
        assert_eq!(result.info_count, 1);
        assert_eq!(result.hint_count, 1);
        assert_eq!(result.diagnostics[0].severity, "information");
        assert_eq!(result.diagnostics[1].severity, "hint");
    }

    #[test]
    fn test_process_diagnostics_with_number_code() {
        let diags = vec![json!({
            "message": "Error",
            "severity": 1,
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 0, "character": 1}
            },
            "code": 42
        })];
        let result = process_diagnostics(&diags);
        assert_eq!(result.diagnostics[0].code, Some("42".to_string()));
    }

    #[test]
    fn test_process_symbols_flat_response() {
        let result = json!([{
            "name": "globalVar",
            "kind": 13,
            "location": {
                "uri": "file:///test.cj",
                "range": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 0, "character": 10}
                }
            }
        }]);
        let syms = process_symbols(&result, "test.cj");
        // SymbolInformation has a `location` field → Flat variant
        assert_eq!(syms.count, 1);
        assert_eq!(syms.symbols[0].name, "globalVar");
        assert_eq!(syms.symbols[0].kind, "variable");
    }
}
