use rmcp::model::CallToolResult;

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

pub fn print_tool_result(result: &CallToolResult) {
    println!("{}", extract_text(result));
}

pub fn print_error(msg: &str) {
    eprintln!("error: {msg}");
}
