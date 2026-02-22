const DOCS_PROMPT: &str = include_str!("docs.txt");
const LSP_PROMPT: &str = include_str!("lsp.txt");

pub fn get_prompt(lsp_enabled: bool) -> String {
    if lsp_enabled {
        format!("{DOCS_PROMPT}\n\n---\n\n{LSP_PROMPT}")
    } else {
        DOCS_PROMPT.to_string()
    }
}
