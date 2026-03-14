const DOCS_PROMPT: &str = include_str!("prompts/docs.txt");
const LSP_PROMPT: &str = include_str!("prompts/lsp.txt");

pub fn get_prompt(lsp_enabled: bool) -> String {
    if lsp_enabled {
        format!("{DOCS_PROMPT}\n\n---\n\n{LSP_PROMPT}")
    } else {
        DOCS_PROMPT.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_prompt_without_lsp() {
        let result = get_prompt(false);
        assert_eq!(result, DOCS_PROMPT);
        assert!(!result.contains(LSP_PROMPT));
    }

    #[test]
    fn test_get_prompt_with_lsp() {
        let result = get_prompt(true);
        assert!(result.contains(DOCS_PROMPT));
        assert!(result.contains(LSP_PROMPT));
        assert!(result.contains("---"));
        let expected = format!("{DOCS_PROMPT}\n\n---\n\n{LSP_PROMPT}");
        assert_eq!(result, expected);
    }
}
