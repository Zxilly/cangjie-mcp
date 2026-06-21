// Some utility functions are shared between the lsp implementation and tests,
// but appear unused when the lsp feature is disabled.
#![allow(dead_code)]

mod resolve;
mod response;
mod types;

pub use types::{
    LspOperation, LspRequest, LspResponse, LspResponseStatus, LspTarget, ResolvedTarget,
    META_WORKING_DIRECTORY,
};

use response::error_response;
#[cfg(feature = "lsp")]
use response::{response_with_data, status_from_count, unsupported_response};

#[cfg(feature = "lsp")]
use resolve::{lsp_unavailable_message, resolve_target_position, validate_request};

#[cfg(feature = "lsp")]
use rmcp::handler::server::router::tool::ToolRouter;

#[cfg(feature = "lsp")]
use crate::mcp_handler::CangjieServer;

#[cfg(feature = "lsp")]
use cangjie_lsp::client::{CangjieClient, DiagnosticsStatus};
#[cfg(feature = "lsp")]
use cangjie_lsp::tools as lsp_tools;

#[cfg(feature = "lsp")]
impl CangjieServer {
    pub(crate) fn lsp_tool_router() -> ToolRouter<Self> {
        ToolRouter::<Self>::new().with_route((Self::lsp_tool_attr(), Self::lsp))
    }
}

pub(crate) async fn execute_lsp_request(
    params: LspRequest,
    #[cfg(feature = "lsp")] lsp_pool: Option<&crate::lsp_pool::LspPool>,
    #[cfg(feature = "lsp")] working_dir: Option<std::path::PathBuf>,
) -> String {
    #[cfg(feature = "lsp")]
    {
        execute_lsp_request_impl(params, lsp_pool, working_dir).await
    }
    #[cfg(not(feature = "lsp"))]
    {
        error_response(
            params.operation,
            "LSP support is not compiled in. Enable the 'lsp' feature.",
        )
    }
}

#[cfg(feature = "lsp")]
async fn execute_lsp_request_impl(
    mut params: LspRequest,
    lsp_pool: Option<&crate::lsp_pool::LspPool>,
    working_dir: Option<std::path::PathBuf>,
) -> String {
    // Normalize MSYS2-style paths (e.g. /c/Users/… → C:/Users/…) from tool inputs
    if let Some(ref mut fp) = params.file_path {
        *fp = cangjie_lsp::utils::normalize_msys2_path(fp);
    }
    let working_dir = working_dir.map(|wd| {
        std::path::PathBuf::from(cangjie_lsp::utils::normalize_msys2_path(
            &wd.to_string_lossy(),
        ))
    });

    if let Err(message) = validate_request(&params) {
        return error_response(params.operation, message);
    }

    // Resolve LSP client: pool mode (daemon) or singleton mode (stdio)
    let pool_client;
    let singleton_guard;

    if let Some(pool) = lsp_pool {
        let workspace = match working_dir {
            Some(wd) => wd,
            None => {
                return error_response(
                    params.operation,
                    "working directory is required in daemon mode for LSP operations. \
                     Pass it via _meta.workingDirectory in the tool call request.",
                );
            }
        };
        match pool.get_or_create(&workspace).await {
            Ok(c) => pool_client = Some(c),
            Err(msg) => return error_response(params.operation, msg),
        }
        singleton_guard = None;
    } else {
        pool_client = None;
        singleton_guard = cangjie_lsp::get_client().await;
        if singleton_guard.as_ref().and_then(|g| g.as_ref()).is_none() {
            return error_response(params.operation, lsp_unavailable_message());
        }
    }

    let client: &CangjieClient = if let Some(ref c) = pool_client {
        c
    } else {
        singleton_guard.as_ref().unwrap().as_ref().unwrap()
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

    macro_rules! lsp_op {
        // positioned — client.method(file, line, char), processor(&result)
        (positioned, $client_method:ident, $processor:ident) => {{
            let position = resolved_position.expect("validated target");
            match client
                .$client_method(
                    file_path.expect("validated file"),
                    position.zero_based_line,
                    position.zero_based_character,
                )
                .await
            {
                Ok(result) => {
                    let data = lsp_tools::$processor(&result);
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
        }};

        // file_only — client.method(file), processor(&result, file)
        (file_only, $client_method:ident, $processor:ident) => {{
            let file = file_path.expect("validated file");
            match client.$client_method(file).await {
                Ok(result) => {
                    let data = lsp_tools::$processor(&result, file);
                    response_with_data(
                        params.operation,
                        status_from_count(data.count),
                        None,
                        &data,
                        None,
                    )
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }};

        // query_only — client.method(query), processor(&result)
        (query_only, $client_method:ident, $processor:ident) => {{
            match client
                .$client_method(params.query.as_deref().unwrap_or_default())
                .await
            {
                Ok(result) => {
                    let data = lsp_tools::$processor(&result);
                    response_with_data(
                        params.operation,
                        status_from_count(data.count),
                        None,
                        &data,
                        None,
                    )
                }
                Err(error) => error_response(params.operation, format!("Error: {error}")),
            }
        }};
    }

    match params.operation {
        LspOperation::Definition => lsp_op!(positioned, definition, process_definition),
        LspOperation::References => lsp_op!(positioned, references, process_references),
        LspOperation::IncomingCalls => lsp_op!(positioned, incoming_calls, process_incoming_calls),
        LspOperation::OutgoingCalls => lsp_op!(positioned, outgoing_calls, process_outgoing_calls),
        LspOperation::TypeSupertypes => {
            lsp_op!(positioned, type_supertypes, process_type_hierarchy)
        }
        LspOperation::TypeSubtypes => lsp_op!(positioned, type_subtypes, process_type_hierarchy),
        LspOperation::DocumentSymbol => lsp_op!(file_only, document_symbol, process_symbols),
        LspOperation::WorkspaceSymbol => {
            lsp_op!(query_only, workspace_symbol, process_workspace_symbols)
        }

        // Special cases — kept manual
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
    }
}
