use rmcp::model::CallToolResult;
use serde_json::Value;

use super::Commands;

/// Extract raw text from a CallToolResult.
fn extract_text(result: &CallToolResult) -> String {
    let mut out = String::new();
    for content in &result.content {
        if let rmcp::model::RawContent::Text(ref text) = content.raw {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&text.text);
        }
    }
    out
}

/// Format the tool result based on the command that produced it.
pub fn print_tool_result(result: &CallToolResult, cmd: &Commands) {
    let text = extract_text(result);

    match cmd {
        Commands::Topic { .. } => print_topic(&text),
        Commands::Topics { .. } => print_topics_list(&text),
        _ => {
            // search_docs already returns Markdown; lsp returns JSON (useful for tools)
            println!("{text}");
        }
    }
}

pub fn print_error(msg: &str) {
    eprintln!("error: {msg}");
}

/// Format get_topic output: extract the doc content from JSON wrapper and print as text.
fn print_topic(text: &str) {
    let Ok(val) = serde_json::from_str::<Value>(text) else {
        // Not JSON (e.g., error message) — print as-is
        println!("{text}");
        return;
    };

    if let Some(title) = val.get("title").and_then(Value::as_str) {
        println!("# {title}");
    }

    // Print category/topic metadata as a compact line
    let category = val.get("category").and_then(Value::as_str);
    let topic = val.get("topic").and_then(Value::as_str);
    if let (Some(cat), Some(top)) = (category, topic) {
        println!("[{cat}/{top}]\n");
    }

    if let Some(content) = val.get("content").and_then(Value::as_str) {
        println!("{content}");
    }
}

/// Format list_topics output: convert JSON categories map to readable text.
fn print_topics_list(text: &str) {
    let Ok(val) = serde_json::from_str::<Value>(text) else {
        println!("{text}");
        return;
    };

    // Handle error case
    if let Some(err) = val.get("error").and_then(Value::as_str) {
        eprintln!("error: {err}");
        if let Some(cats) = val.get("available_categories").and_then(Value::as_array) {
            let names: Vec<&str> = cats.iter().filter_map(Value::as_str).collect();
            if !names.is_empty() {
                eprintln!("Available categories: {}", names.join(", "));
            }
        }
        return;
    }

    let total_cats = val
        .get("total_categories")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_topics = val.get("total_topics").and_then(Value::as_u64).unwrap_or(0);
    println!("{total_topics} topics in {total_cats} categories:\n");

    if let Some(categories) = val.get("categories").and_then(Value::as_object) {
        let mut cats: Vec<_> = categories.iter().collect();
        cats.sort_by_key(|(k, _)| k.as_str());

        for (cat, topics) in cats {
            println!("## {cat}");
            if let Some(topics) = topics.as_array() {
                for topic in topics {
                    let name = topic.get("name").and_then(Value::as_str).unwrap_or("?");
                    let title = topic.get("title").and_then(Value::as_str).unwrap_or("");
                    if title.is_empty() {
                        println!("  - {name}");
                    } else {
                        println!("  - {name}: {title}");
                    }
                }
            }
            println!();
        }
    }
}
