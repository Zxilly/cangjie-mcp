// Re-export lsp_types symbols used across the LSP module.

// Used by client.rs
pub use lsp_types::{
    CallHierarchyIncomingCallsParams, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    ClientCapabilities, ClientInfo, CompletionParams, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbolParams, GotoDefinitionParams, HoverParams,
    InitializeParams, InitializeResult, InitializedParams, Position, ReferenceContext,
    ReferenceParams, RenameParams, ServerCapabilities, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams, TraceValue,
    TypeHierarchyPrepareParams, TypeHierarchySubtypesParams, TypeHierarchySupertypesParams, Uri,
    VersionedTextDocumentIdentifier, WorkDoneProgressParams, WorkspaceFolder,
    WorkspaceSymbolParams,
};

// Used by tools.rs
pub use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, CompletionItem,
    CompletionResponse, Diagnostic, DiagnosticSeverity, DocumentSymbol, DocumentSymbolResponse,
    GotoDefinitionResponse, Hover, HoverContents, Location, LocationLink, MarkedString,
    NumberOrString, SymbolKind, TypeHierarchyItem, WorkspaceEdit,
};
