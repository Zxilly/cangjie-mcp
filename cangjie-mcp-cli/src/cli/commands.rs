use cangjie_server::lsp_tools::{LspOperation, LspRequest, LspTarget};
use rmcp::model::{CallToolRequestParams, Meta};
use serde_json::{json, Map, Value};

use super::{Commands, LspCommand};

fn make_params(name: &str, arguments: Value) -> CallToolRequestParams {
    let args = match arguments {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    CallToolRequestParams::new(name.to_string()).with_arguments(args)
}

fn build_lsp_target(
    symbol: &Option<String>,
    line: &Option<u32>,
    character: &Option<u32>,
) -> Option<LspTarget> {
    if let Some(sym) = symbol {
        Some(LspTarget::Symbol {
            symbol: sym.clone(),
            line_hint: None,
        })
    } else if let (Some(&l), Some(&c)) = (line.as_ref(), character.as_ref()) {
        Some(LspTarget::Position {
            line: l,
            character: c,
        })
    } else {
        None
    }
}

fn lsp_command_to_request(cmd: &LspCommand) -> LspRequest {
    match cmd {
        LspCommand::Definition {
            file,
            symbol,
            line,
            character,
        } => LspRequest {
            operation: LspOperation::Definition,
            file_path: Some(file.clone()),
            target: build_lsp_target(symbol, line, character),
            query: None,
            new_name: None,
        },
        LspCommand::References {
            file,
            symbol,
            line,
            character,
        } => LspRequest {
            operation: LspOperation::References,
            file_path: Some(file.clone()),
            target: build_lsp_target(symbol, line, character),
            query: None,
            new_name: None,
        },
        LspCommand::Hover {
            file,
            symbol,
            line,
            character,
        } => LspRequest {
            operation: LspOperation::Hover,
            file_path: Some(file.clone()),
            target: build_lsp_target(symbol, line, character),
            query: None,
            new_name: None,
        },
        LspCommand::Symbols { file } => LspRequest {
            operation: LspOperation::DocumentSymbol,
            file_path: Some(file.clone()),
            target: None,
            query: None,
            new_name: None,
        },
        LspCommand::Diagnostics { file } => LspRequest {
            operation: LspOperation::Diagnostics,
            file_path: Some(file.clone()),
            target: None,
            query: None,
            new_name: None,
        },
        LspCommand::WorkspaceSymbol { query } => LspRequest {
            operation: LspOperation::WorkspaceSymbol,
            file_path: None,
            target: None,
            query: Some(query.clone()),
            new_name: None,
        },
        LspCommand::Completion {
            file,
            line,
            character,
        } => LspRequest {
            operation: LspOperation::Completion,
            file_path: Some(file.clone()),
            target: Some(LspTarget::Position {
                line: *line,
                character: *character,
            }),
            query: None,
            new_name: None,
        },
        LspCommand::Rename {
            file,
            symbol,
            new_name,
        } => LspRequest {
            operation: LspOperation::Rename,
            file_path: Some(file.clone()),
            target: Some(LspTarget::Symbol {
                symbol: symbol.clone(),
                line_hint: None,
            }),
            query: None,
            new_name: Some(new_name.clone()),
        },
        LspCommand::IncomingCalls {
            file,
            symbol,
            line,
            character,
        } => LspRequest {
            operation: LspOperation::IncomingCalls,
            file_path: Some(file.clone()),
            target: build_lsp_target(symbol, line, character),
            query: None,
            new_name: None,
        },
        LspCommand::OutgoingCalls {
            file,
            symbol,
            line,
            character,
        } => LspRequest {
            operation: LspOperation::OutgoingCalls,
            file_path: Some(file.clone()),
            target: build_lsp_target(symbol, line, character),
            query: None,
            new_name: None,
        },
        LspCommand::TypeSupertypes {
            file,
            symbol,
            line,
            character,
        } => LspRequest {
            operation: LspOperation::TypeSupertypes,
            file_path: Some(file.clone()),
            target: build_lsp_target(symbol, line, character),
            query: None,
            new_name: None,
        },
        LspCommand::TypeSubtypes {
            file,
            symbol,
            line,
            character,
        } => LspRequest {
            operation: LspOperation::TypeSubtypes,
            file_path: Some(file.clone()),
            target: build_lsp_target(symbol, line, character),
            query: None,
            new_name: None,
        },
    }
}

pub fn command_to_tool_call(cmd: &Commands) -> Option<CallToolRequestParams> {
    match cmd {
        Commands::Query {
            query,
            category,
            top_k,
            offset,
            package,
        } => {
            let mut args = json!({
                "query": query,
                "top_k": top_k,
                "offset": offset,
            });
            if let Some(cat) = category {
                args["category"] = json!(cat);
            }
            if let Some(pkg) = package {
                args["package"] = json!(pkg);
            }
            Some(make_params("cangjie_search_docs", args))
        }
        Commands::Topic {
            name,
            category,
            offset,
            max_length,
        } => {
            let mut args = json!({
                "topic": name,
                "offset": offset,
                "max_length": max_length,
            });
            if let Some(cat) = category {
                args["category"] = json!(cat);
            }
            Some(make_params("cangjie_get_topic", args))
        }
        Commands::Topics { category } => {
            let mut args = json!({});
            if let Some(cat) = category {
                args["category"] = json!(cat);
            }
            Some(make_params("cangjie_list_topics", args))
        }
        Commands::Lsp { operation } => {
            let request = lsp_command_to_request(operation);
            let args = serde_json::to_value(&request).unwrap_or_default();
            let mut params = make_params("cangjie_lsp", args);
            // Pass working directory via _meta (header-style, not visible in tool schema)
            if let Ok(cwd) = std::env::current_dir() {
                let mut meta = Meta::new();
                meta.0.insert(
                    cangjie_server::lsp_tools::META_WORKING_DIRECTORY.to_string(),
                    json!(cwd.to_string_lossy()),
                );
                params.meta = Some(meta);
            }
            Some(params)
        }
        Commands::Serve | Commands::Index | Commands::Daemon { .. } | Commands::Config { .. } => {
            None
        }
    }
}
