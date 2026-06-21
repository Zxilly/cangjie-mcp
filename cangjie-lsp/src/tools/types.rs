use serde::{Deserialize, Serialize};

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
pub struct WorkspaceSymbolOutput {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line: u32,
    pub character: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceSymbolResult {
    pub symbols: Vec<WorkspaceSymbolOutput>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CallHierarchyItemOutput {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line: u32,
    pub character: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IncomingCallOutput {
    pub from: CallHierarchyItemOutput,
    pub call_sites: Vec<LocationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IncomingCallsResult {
    pub calls: Vec<IncomingCallOutput>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutgoingCallOutput {
    pub to: CallHierarchyItemOutput,
    pub call_sites: Vec<LocationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutgoingCallsResult {
    pub calls: Vec<OutgoingCallOutput>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TypeHierarchyItemOutput {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line: u32,
    pub character: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TypeHierarchyResult {
    pub items: Vec<TypeHierarchyItemOutput>,
    pub count: usize,
}
