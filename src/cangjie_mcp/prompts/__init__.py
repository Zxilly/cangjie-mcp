"""Server prompts for MCP instructions."""

from importlib.resources import files


def _load(name: str) -> str:
    """Load prompt from text file."""
    return files(__package__).joinpath(f"{name}.txt").read_text(encoding="utf-8").strip()


def get_docs_prompt() -> str:
    """Get documentation server prompt."""
    return _load("docs")


def get_lsp_prompt() -> str:
    """Get LSP server prompt."""
    return _load("lsp")


def get_prompt() -> str:
    """Get server prompt."""
    return f"""{get_docs_prompt()}

---

{get_lsp_prompt()}"""
