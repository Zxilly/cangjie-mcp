// Re-export lsp_types symbols used across the LSP module.

// Used by client.rs
pub use lsp_types::{
    CallHierarchyIncomingCallsParams, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    ClientCapabilities, ClientInfo, CompletionItem, CompletionParams, CompletionResponse,
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbolParams,
    GotoDefinitionParams, HoverParams, InitializeParams, InitializeResult, InitializedParams,
    OneOf, Position, ReferenceContext, ReferenceParams, RenameParams, ServerCapabilities,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, TraceValue, TypeHierarchyPrepareParams,
    TypeHierarchySubtypesParams, TypeHierarchySupertypesParams, Uri,
    VersionedTextDocumentIdentifier, WorkDoneProgressParams, WorkspaceFolder,
    WorkspaceSymbolParams,
};

// Used by tools.rs
pub use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, Diagnostic,
    DiagnosticSeverity, DocumentSymbol, DocumentSymbolResponse, GotoDefinitionResponse, Hover,
    HoverContents, Location, LocationLink, MarkedString, NumberOrString, SymbolKind,
    TypeHierarchyItem, WorkspaceEdit,
};
