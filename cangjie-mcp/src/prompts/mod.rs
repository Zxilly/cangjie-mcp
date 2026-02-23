const DOCS_PROMPT: &str = include_str!("docs.txt");
const LSP_PROMPT: &str = include_str!("lsp.txt");

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
        // Should NOT contain LSP_PROMPT content
        assert!(!result.contains(LSP_PROMPT));
    }

    #[test]
    fn test_get_prompt_with_lsp() {
        let result = get_prompt(true);
        // Should contain both prompts joined by separator
        assert!(result.contains(DOCS_PROMPT));
        assert!(result.contains(LSP_PROMPT));
        assert!(result.contains("---"));
        // Verify the exact format
        let expected = format!("{DOCS_PROMPT}\n\n---\n\n{LSP_PROMPT}");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_prompts_not_empty() {
        assert!(!DOCS_PROMPT.is_empty(), "DOCS_PROMPT should not be empty");
        assert!(!LSP_PROMPT.is_empty(), "LSP_PROMPT should not be empty");
    }
}
