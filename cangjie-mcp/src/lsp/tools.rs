use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Diagnostic, DiagnosticSeverity,
    DocumentSymbol, DocumentSymbolResponse, GotoDefinitionResponse, Hover, HoverContents, Location,
    LocationLink, MarkedString, SymbolKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::lsp::utils::uri_to_path;

// -- Output types ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocationResult {
    pub file_path: String,
    pub line: u32,
    pub character: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_character: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DefinitionResult {
    pub locations: Vec<LocationResult>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReferencesResult {
    pub locations: Vec<LocationResult>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HoverOutput {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<LocationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SymbolOutput {
    pub name: String,
    pub kind: String,
    pub line: u32,
    pub character: u32,
    pub end_line: u32,
    pub end_character: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<SymbolOutput>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SymbolsResult {
    pub symbols: Vec<SymbolOutput>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticOutput {
    pub message: String,
    pub severity: String,
    pub line: u32,
    pub character: u32,
    pub end_line: u32,
    pub end_character: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticsResult {
    pub diagnostics: Vec<DiagnosticOutput>,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub hint_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionOutput {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionResult {
    pub items: Vec<CompletionOutput>,
    pub count: usize,
}

// -- Helpers -----------------------------------------------------------------

fn severity_name(severity: Option<DiagnosticSeverity>) -> &'static str {
    match severity {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warning",
        Some(DiagnosticSeverity::INFORMATION) => "information",
        Some(DiagnosticSeverity::HINT) => "hint",
        _ => "unknown",
    }
}

fn symbol_kind_name(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::FILE => "file",
        SymbolKind::MODULE => "module",
        SymbolKind::NAMESPACE => "namespace",
        SymbolKind::PACKAGE => "package",
        SymbolKind::CLASS => "class",
        SymbolKind::METHOD => "method",
        SymbolKind::PROPERTY => "property",
        SymbolKind::FIELD => "field",
        SymbolKind::CONSTRUCTOR => "constructor",
        SymbolKind::ENUM => "enum",
        SymbolKind::INTERFACE => "interface",
        SymbolKind::FUNCTION => "function",
        SymbolKind::VARIABLE => "variable",
        SymbolKind::CONSTANT => "constant",
        SymbolKind::STRING => "string",
        SymbolKind::NUMBER => "number",
        SymbolKind::BOOLEAN => "boolean",
        SymbolKind::ARRAY => "array",
        SymbolKind::OBJECT => "object",
        SymbolKind::KEY => "key",
        SymbolKind::NULL => "null",
        SymbolKind::ENUM_MEMBER => "enum member",
        SymbolKind::STRUCT => "struct",
        SymbolKind::EVENT => "event",
        SymbolKind::OPERATOR => "operator",
        SymbolKind::TYPE_PARAMETER => "type parameter",
        _ => "unknown",
    }
}

fn completion_kind_name(kind: CompletionItemKind) -> &'static str {
    match kind {
        CompletionItemKind::TEXT => "text",
        CompletionItemKind::METHOD => "method",
        CompletionItemKind::FUNCTION => "function",
        CompletionItemKind::CONSTRUCTOR => "constructor",
        CompletionItemKind::FIELD => "field",
        CompletionItemKind::VARIABLE => "variable",
        CompletionItemKind::CLASS => "class",
        CompletionItemKind::INTERFACE => "interface",
        CompletionItemKind::MODULE => "module",
        CompletionItemKind::PROPERTY => "property",
        CompletionItemKind::UNIT => "unit",
        CompletionItemKind::VALUE => "value",
        CompletionItemKind::ENUM => "enum",
        CompletionItemKind::KEYWORD => "keyword",
        CompletionItemKind::SNIPPET => "snippet",
        CompletionItemKind::COLOR => "color",
        CompletionItemKind::FILE => "file",
        CompletionItemKind::REFERENCE => "reference",
        CompletionItemKind::FOLDER => "folder",
        CompletionItemKind::ENUM_MEMBER => "enum member",
        CompletionItemKind::CONSTANT => "constant",
        CompletionItemKind::STRUCT => "struct",
        CompletionItemKind::EVENT => "event",
        CompletionItemKind::OPERATOR => "operator",
        CompletionItemKind::TYPE_PARAMETER => "type parameter",
        _ => "unknown",
    }
}

fn validate_file_path(file_path: &str) -> Option<String> {
    let path = std::path::Path::new(file_path);
    if !path.exists() {
        return Some(format!("File not found: {file_path}"));
    }
    if !path.is_file() {
        return Some(format!("Not a file: {file_path}"));
    }
    if path.extension().and_then(|e| e.to_str()) != Some("cj") {
        return Some(format!(
            "Not a Cangjie file (expected .cj extension): {file_path}"
        ));
    }
    None
}

// -- Conversion helpers ------------------------------------------------------

fn location_to_result(loc: &Location) -> LocationResult {
    LocationResult {
        file_path: uri_to_path(loc.uri.as_str()).to_string_lossy().to_string(),
        line: loc.range.start.line + 1,
        character: loc.range.start.character + 1,
        end_line: Some(loc.range.end.line + 1),
        end_character: Some(loc.range.end.character + 1),
    }
}

fn location_link_to_result(link: &LocationLink) -> LocationResult {
    LocationResult {
        file_path: uri_to_path(link.target_uri.as_str())
            .to_string_lossy()
            .to_string(),
        line: link.target_range.start.line + 1,
        character: link.target_range.start.character + 1,
        end_line: Some(link.target_range.end.line + 1),
        end_character: Some(link.target_range.end.character + 1),
    }
}

fn convert_document_symbol(sym: &DocumentSymbol) -> SymbolOutput {
    let children = sym
        .children
        .as_ref()
        .map(|kids| kids.iter().map(convert_document_symbol).collect());

    SymbolOutput {
        name: sym.name.clone(),
        kind: symbol_kind_name(sym.kind).to_string(),
        line: sym.range.start.line + 1,
        character: sym.range.start.character + 1,
        end_line: sym.range.end.line + 1,
        end_character: sym.range.end.character + 1,
        children,
    }
}

fn extract_documentation(doc: &lsp_types::Documentation) -> String {
    match doc {
        lsp_types::Documentation::String(s) => s.clone(),
        lsp_types::Documentation::MarkupContent(mc) => mc.value.clone(),
    }
}

fn extract_diagnostic_code(code: &lsp_types::NumberOrString) -> String {
    match code {
        lsp_types::NumberOrString::Number(n) => n.to_string(),
        lsp_types::NumberOrString::String(s) => s.clone(),
    }
}

// -- Tool execution functions ------------------------------------------------

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

pub fn process_hover(result: &Value, file_path: &str) -> String {
    let hover: Option<Hover> = serde_json::from_value(result.clone()).ok();

    let hover = match hover {
        Some(h) => h,
        None => {
            let output = HoverOutput {
                content: "No hover information available".to_string(),
                range: None,
            };
            return serde_json::to_string_pretty(&output).unwrap();
        }
    };

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

    let output = HoverOutput { content, range };
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

pub fn process_completion(result: &Value) -> CompletionResult {
    let response: Option<CompletionResponse> = serde_json::from_value(result.clone()).ok();

    let completion_items: Vec<CompletionItem> = match response {
        Some(CompletionResponse::Array(items)) => items,
        Some(CompletionResponse::List(list)) => list.items,
        None => Vec::new(),
    };

    let items: Vec<CompletionOutput> = completion_items
        .iter()
        .map(|item| CompletionOutput {
            label: item.label.clone(),
            kind: item.kind.map(|k| completion_kind_name(k).to_string()),
            detail: item.detail.clone(),
            documentation: item.documentation.as_ref().map(extract_documentation),
            insert_text: item.insert_text.clone(),
        })
        .collect();

    let count = items.len();
    CompletionResult { items, count }
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
    fn test_process_completion() {
        let result = json!({
            "isIncomplete": false,
            "items": [
                {
                    "label": "println",
                    "kind": 3,
                    "detail": "func println(msg: String)",
                    "insertText": "println($1)"
                },
                {
                    "label": "print",
                    "kind": 3
                }
            ]
        });
        let comp = process_completion(&result);
        assert_eq!(comp.count, 2);
        assert_eq!(comp.items[0].label, "println");
        assert_eq!(comp.items[0].kind, Some("function".to_string()));
        assert_eq!(comp.items[0].insert_text, Some("println($1)".to_string()));
        assert_eq!(comp.items[1].label, "print");
    }

    #[test]
    fn test_process_completion_flat_array() {
        let result = json!([
            {"label": "item1", "kind": 6},
            {"label": "item2", "kind": 7}
        ]);
        let comp = process_completion(&result);
        assert_eq!(comp.count, 2);
        assert_eq!(comp.items[0].kind, Some("variable".to_string()));
        assert_eq!(comp.items[1].kind, Some("class".to_string()));
    }

    #[test]
    fn test_severity_name_all_variants() {
        assert_eq!(severity_name(Some(DiagnosticSeverity::ERROR)), "error");
        assert_eq!(severity_name(Some(DiagnosticSeverity::WARNING)), "warning");
        assert_eq!(
            severity_name(Some(DiagnosticSeverity::INFORMATION)),
            "information"
        );
        assert_eq!(severity_name(Some(DiagnosticSeverity::HINT)), "hint");
        assert_eq!(severity_name(None), "unknown");
    }

    #[test]
    fn test_symbol_kind_name_coverage() {
        assert_eq!(symbol_kind_name(SymbolKind::FILE), "file");
        assert_eq!(symbol_kind_name(SymbolKind::MODULE), "module");
        assert_eq!(symbol_kind_name(SymbolKind::NAMESPACE), "namespace");
        assert_eq!(symbol_kind_name(SymbolKind::PACKAGE), "package");
        assert_eq!(symbol_kind_name(SymbolKind::CLASS), "class");
        assert_eq!(symbol_kind_name(SymbolKind::METHOD), "method");
        assert_eq!(symbol_kind_name(SymbolKind::PROPERTY), "property");
        assert_eq!(symbol_kind_name(SymbolKind::FIELD), "field");
        assert_eq!(symbol_kind_name(SymbolKind::CONSTRUCTOR), "constructor");
        assert_eq!(symbol_kind_name(SymbolKind::ENUM), "enum");
        assert_eq!(symbol_kind_name(SymbolKind::INTERFACE), "interface");
        assert_eq!(symbol_kind_name(SymbolKind::FUNCTION), "function");
        assert_eq!(symbol_kind_name(SymbolKind::VARIABLE), "variable");
        assert_eq!(symbol_kind_name(SymbolKind::CONSTANT), "constant");
        assert_eq!(symbol_kind_name(SymbolKind::STRING), "string");
        assert_eq!(symbol_kind_name(SymbolKind::NUMBER), "number");
        assert_eq!(symbol_kind_name(SymbolKind::BOOLEAN), "boolean");
        assert_eq!(symbol_kind_name(SymbolKind::ARRAY), "array");
        assert_eq!(symbol_kind_name(SymbolKind::OBJECT), "object");
        assert_eq!(symbol_kind_name(SymbolKind::KEY), "key");
        assert_eq!(symbol_kind_name(SymbolKind::NULL), "null");
        assert_eq!(symbol_kind_name(SymbolKind::ENUM_MEMBER), "enum member");
        assert_eq!(symbol_kind_name(SymbolKind::STRUCT), "struct");
        assert_eq!(symbol_kind_name(SymbolKind::EVENT), "event");
        assert_eq!(symbol_kind_name(SymbolKind::OPERATOR), "operator");
        assert_eq!(
            symbol_kind_name(SymbolKind::TYPE_PARAMETER),
            "type parameter"
        );
        // SymbolKind is a newtype with private field, so we can't construct
        // an unknown variant for testing the default branch.
    }

    #[test]
    fn test_completion_kind_name_coverage() {
        assert_eq!(completion_kind_name(CompletionItemKind::TEXT), "text");
        assert_eq!(completion_kind_name(CompletionItemKind::METHOD), "method");
        assert_eq!(
            completion_kind_name(CompletionItemKind::FUNCTION),
            "function"
        );
        assert_eq!(
            completion_kind_name(CompletionItemKind::CONSTRUCTOR),
            "constructor"
        );
        assert_eq!(completion_kind_name(CompletionItemKind::FIELD), "field");
        assert_eq!(
            completion_kind_name(CompletionItemKind::VARIABLE),
            "variable"
        );
        assert_eq!(completion_kind_name(CompletionItemKind::CLASS), "class");
        assert_eq!(
            completion_kind_name(CompletionItemKind::INTERFACE),
            "interface"
        );
        assert_eq!(completion_kind_name(CompletionItemKind::MODULE), "module");
        assert_eq!(
            completion_kind_name(CompletionItemKind::PROPERTY),
            "property"
        );
        assert_eq!(completion_kind_name(CompletionItemKind::UNIT), "unit");
        assert_eq!(completion_kind_name(CompletionItemKind::VALUE), "value");
        assert_eq!(completion_kind_name(CompletionItemKind::ENUM), "enum");
        assert_eq!(completion_kind_name(CompletionItemKind::KEYWORD), "keyword");
        assert_eq!(completion_kind_name(CompletionItemKind::SNIPPET), "snippet");
        assert_eq!(completion_kind_name(CompletionItemKind::COLOR), "color");
        assert_eq!(completion_kind_name(CompletionItemKind::FILE), "file");
        assert_eq!(
            completion_kind_name(CompletionItemKind::REFERENCE),
            "reference"
        );
        assert_eq!(completion_kind_name(CompletionItemKind::FOLDER), "folder");
        assert_eq!(
            completion_kind_name(CompletionItemKind::ENUM_MEMBER),
            "enum member"
        );
        assert_eq!(
            completion_kind_name(CompletionItemKind::CONSTANT),
            "constant"
        );
        assert_eq!(completion_kind_name(CompletionItemKind::STRUCT), "struct");
        assert_eq!(completion_kind_name(CompletionItemKind::EVENT), "event");
        assert_eq!(
            completion_kind_name(CompletionItemKind::OPERATOR),
            "operator"
        );
        assert_eq!(
            completion_kind_name(CompletionItemKind::TYPE_PARAMETER),
            "type parameter"
        );
        // CompletionItemKind is a newtype with private field, so we can't
        // construct an unknown variant for testing the default branch.
    }

    #[test]
    fn test_validate_file_path_nonexistent() {
        let result = validate_file_path("/nonexistent/path/file.cj");
        assert!(result.is_some());
        assert!(result.unwrap().contains("File not found"));
    }

    #[test]
    fn test_validate_file_path_not_cj_extension() {
        // Use a file that exists but isn't .cj
        let result = validate_file_path(env!("CARGO_MANIFEST_DIR"));
        assert!(result.is_some());
        // A directory is "not a file"
        assert!(result.unwrap().contains("Not a file"));
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
    fn test_process_references_empty() {
        let refs = process_references(&json!([]));
        assert_eq!(refs.count, 0);
        assert!(refs.locations.is_empty());
    }

    #[test]
    fn test_process_completion_empty() {
        let comp = process_completion(&json!(null));
        assert_eq!(comp.count, 0);
        assert!(comp.items.is_empty());
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

    #[test]
    fn test_process_symbols_empty() {
        let syms = process_symbols(&json!(null), "test.cj");
        assert_eq!(syms.count, 0);
    }

    #[test]
    fn test_process_completion_with_documentation() {
        let result = json!({
            "isIncomplete": false,
            "items": [{
                "label": "myFunc",
                "kind": 3,
                "documentation": {"kind": "markdown", "value": "# My Function\nDoes things."}
            }]
        });
        let comp = process_completion(&result);
        assert_eq!(comp.count, 1);
        assert_eq!(
            comp.items[0].documentation,
            Some("# My Function\nDoes things.".to_string())
        );
    }

    #[test]
    fn test_process_completion_with_string_documentation() {
        let result = json!({
            "isIncomplete": false,
            "items": [{
                "label": "myFunc",
                "kind": 3,
                "documentation": "Plain text docs"
            }]
        });
        let comp = process_completion(&result);
        assert_eq!(
            comp.items[0].documentation,
            Some("Plain text docs".to_string())
        );
    }

    #[test]
    fn test_get_validate_error_delegates() {
        // get_validate_error is a thin wrapper around validate_file_path
        let result = get_validate_error("/nonexistent/file.cj");
        assert!(result.is_some());
        assert!(result.unwrap().contains("File not found"));
    }
}
