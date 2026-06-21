use crate::types::{
    CallHierarchyItem, DiagnosticSeverity, DocumentSymbol, Location, LocationLink, NumberOrString,
    SymbolKind, TypeHierarchyItem,
};

use crate::utils::uri_to_path;

use super::types::{
    CallHierarchyItemOutput, LocationResult, SymbolOutput, TypeHierarchyItemOutput,
};

pub(super) fn severity_name(severity: Option<DiagnosticSeverity>) -> &'static str {
    match severity {
        Some(DiagnosticSeverity::ERROR) => "error",
        Some(DiagnosticSeverity::WARNING) => "warning",
        Some(DiagnosticSeverity::INFORMATION) => "information",
        Some(DiagnosticSeverity::HINT) => "hint",
        _ => "unknown",
    }
}

pub(super) fn symbol_kind_name(kind: SymbolKind) -> &'static str {
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

pub(super) fn symbol_kind_name_by_number(kind: u32) -> &'static str {
    match kind {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        15 => "string",
        16 => "number",
        17 => "boolean",
        18 => "array",
        19 => "object",
        20 => "key",
        21 => "null",
        22 => "enum member",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type parameter",
        _ => "unknown",
    }
}

pub(super) fn validate_file_path(file_path: &str) -> Option<String> {
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

pub(super) fn location_to_result(loc: &Location) -> LocationResult {
    LocationResult {
        file_path: uri_to_path(loc.uri.as_str()).to_string_lossy().to_string(),
        line: loc.range.start.line + 1,
        character: loc.range.start.character + 1,
        end_line: Some(loc.range.end.line + 1),
        end_character: Some(loc.range.end.character + 1),
    }
}

pub(super) fn location_link_to_result(link: &LocationLink) -> LocationResult {
    LocationResult {
        file_path: uri_to_path(link.target_uri.as_str())
            .to_string_lossy()
            .to_string(),
        line: link.target_selection_range.start.line + 1,
        character: link.target_selection_range.start.character + 1,
        end_line: Some(link.target_range.end.line + 1),
        end_character: Some(link.target_range.end.character + 1),
    }
}

pub(super) fn convert_document_symbol(sym: &DocumentSymbol) -> SymbolOutput {
    let children = sym
        .children
        .as_ref()
        .map(|kids| kids.iter().map(convert_document_symbol).collect());

    SymbolOutput {
        name: sym.name.clone(),
        kind: symbol_kind_name(sym.kind).to_string(),
        line: sym.selection_range.start.line + 1,
        character: sym.selection_range.start.character + 1,
        end_line: sym.range.end.line + 1,
        end_character: sym.range.end.character + 1,
        children,
    }
}

pub(super) fn extract_diagnostic_code(code: &NumberOrString) -> String {
    match code {
        NumberOrString::Number(n) => n.to_string(),
        NumberOrString::String(s) => s.clone(),
    }
}

pub(super) fn convert_call_hierarchy_item(item: &CallHierarchyItem) -> CallHierarchyItemOutput {
    CallHierarchyItemOutput {
        name: item.name.clone(),
        kind: symbol_kind_name(item.kind).to_string(),
        file_path: uri_to_path(item.uri.as_str()).to_string_lossy().to_string(),
        line: item.selection_range.start.line + 1,
        character: item.selection_range.start.character + 1,
        detail: item.detail.clone(),
    }
}

pub(super) fn convert_type_hierarchy_item(item: &TypeHierarchyItem) -> TypeHierarchyItemOutput {
    TypeHierarchyItemOutput {
        name: item.name.clone(),
        kind: symbol_kind_name(item.kind).to_string(),
        file_path: uri_to_path(item.uri.as_str()).to_string_lossy().to_string(),
        line: item.selection_range.start.line + 1,
        character: item.selection_range.start.character + 1,
        detail: item.detail.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_file_path_nonexistent() {
        let result = validate_file_path("/nonexistent/path/file.cj");
        assert!(result.is_some());
        assert!(result.unwrap().contains("File not found"));
    }

    #[test]
    fn test_validate_file_path_not_cj_extension() {
        let result = validate_file_path(env!("CARGO_MANIFEST_DIR"));
        assert!(result.is_some());
        // A directory is "not a file"
        assert!(result.unwrap().contains("Not a file"));
    }
}
