"""Server prompts for MCP instructions."""

from importlib.resources import files


def _load(name: str) -> str:
    """Load prompt from text file."""
    return files(__package__).joinpath(f"{name}.txt").read_text(encoding="utf-8").strip()


def get_prompt(*, lsp_enabled: bool = False) -> str:
    """Get server prompt, optionally including LSP instructions.

    Args:
        lsp_enabled: Whether LSP tools are available

    Returns:
        Combined prompt string
    """
    prompt = _load("docs")
    if lsp_enabled:
        prompt += f"\n\n---\n\n{_load('lsp')}"
    return prompt
