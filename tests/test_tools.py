"""Tests for server/tools.py TypedDict types and helper functions."""

from cangjie_mcp.server.tools import (
    CodeExample,
    SearchResult,
    ToolExample,
    ToolUsageResult,
    TopicResult,
)


class TestSearchResult:
    """Tests for SearchResult TypedDict."""

    def test_create_result(self) -> None:
        """Test creating SearchResult."""
        result: SearchResult = {
            "content": "Functions are defined using func keyword.",
            "score": 0.95,
            "file_path": "/docs/syntax/functions.md",
            "category": "syntax",
            "topic": "functions",
            "title": "Functions",
        }
        assert result["content"] == "Functions are defined using func keyword."
        assert result["score"] == 0.95
        assert result["file_path"] == "/docs/syntax/functions.md"
        assert result["category"] == "syntax"
        assert result["topic"] == "functions"
        assert result["title"] == "Functions"

    def test_empty_fields(self) -> None:
        """Test with empty string fields."""
        result: SearchResult = {
            "content": "Some content",
            "score": 0.5,
            "file_path": "",
            "category": "",
            "topic": "",
            "title": "",
        }
        assert result["content"] == "Some content"
        assert result["file_path"] == ""


class TestTopicResult:
    """Tests for TopicResult TypedDict."""

    def test_create_result(self) -> None:
        """Test creating TopicResult."""
        result: TopicResult = {
            "content": "# Hello World\n\nThis is the content.",
            "file_path": "/docs/basics/hello.md",
            "category": "basics",
            "topic": "hello",
            "title": "Hello World",
        }
        assert result["content"].startswith("# Hello World")
        assert result["file_path"] == "/docs/basics/hello.md"
        assert result["category"] == "basics"


class TestCodeExample:
    """Tests for CodeExample TypedDict."""

    def test_create_example(self) -> None:
        """Test creating CodeExample."""
        example: CodeExample = {
            "language": "cangjie",
            "code": "func main() { println(\"Hello\") }",
            "context": "Basic hello world example",
            "source_topic": "hello_world",
            "source_file": "/docs/basics/hello_world.md",
        }
        assert example["language"] == "cangjie"
        assert "println" in example["code"]
        assert example["source_topic"] == "hello_world"

    def test_bash_example(self) -> None:
        """Test with bash code."""
        example: CodeExample = {
            "language": "bash",
            "code": "cjc build main.cj",
            "context": "Compile a Cangjie file",
            "source_topic": "cjc",
            "source_file": "/docs/tools/cjc.md",
        }
        assert example["language"] == "bash"
        assert "cjc" in example["code"]


class TestToolExample:
    """Tests for ToolExample TypedDict."""

    def test_create_example(self) -> None:
        """Test creating ToolExample."""
        example: ToolExample = {
            "code": "cjpm build",
            "context": "Build the project",
        }
        assert example["code"] == "cjpm build"
        assert example["context"] == "Build the project"


class TestToolUsageResult:
    """Tests for ToolUsageResult TypedDict."""

    def test_create_result(self) -> None:
        """Test creating ToolUsageResult."""
        examples: list[ToolExample] = [
            {"code": "cjpm build", "context": "Build"},
            {"code": "cjpm test", "context": "Test"},
        ]
        result: ToolUsageResult = {
            "tool_name": "cjpm",
            "content": "CJPM is the Cangjie package manager.",
            "examples": examples,
        }
        assert result["tool_name"] == "cjpm"
        assert "package manager" in result["content"]
        assert len(result["examples"]) == 2
        assert result["examples"][0]["code"] == "cjpm build"

    def test_empty_examples(self) -> None:
        """Test with no examples."""
        result: ToolUsageResult = {
            "tool_name": "cjfmt",
            "content": "Code formatter for Cangjie.",
            "examples": [],
        }
        assert result["tool_name"] == "cjfmt"
        assert result["examples"] == []
