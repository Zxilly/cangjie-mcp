use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Meta key for passing the client's working directory via `_meta` header.
pub const META_WORKING_DIRECTORY: &str = "workingDirectory";

#[cfg(feature = "lsp")]
use cangjie_lsp::client::SupportedOperation;

#[cfg(feature = "lsp")]
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
}

impl LspOperation {
    #[cfg(feature = "lsp")]
    pub(crate) fn requires_file_path(self) -> bool {
        !matches!(self, Self::WorkspaceSymbol)
    }

    #[cfg(feature = "lsp")]
    pub(crate) fn requires_target(self) -> bool {
        matches!(
            self,
            Self::Definition
                | Self::References
                | Self::Hover
                | Self::IncomingCalls
                | Self::OutgoingCalls
                | Self::TypeSupertypes
                | Self::TypeSubtypes
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

#[cfg(feature = "lsp")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedPosition {
    pub(crate) zero_based_line: u32,
    pub(crate) zero_based_character: u32,
    pub(crate) display: ResolvedTarget,
}
