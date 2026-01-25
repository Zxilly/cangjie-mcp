"""Extract API signatures and stdlib info from Cangjie documentation."""

from __future__ import annotations

import re
from typing import TypedDict

# Pattern to extract package from import statements
IMPORT_PATTERN = re.compile(r"import\s+(std(?:\.\w+)+)")

# Pattern to extract type declarations (class, struct, interface, enum)
TYPE_PATTERN = re.compile(
    r"(?:public\s+)?(?:open\s+)?(?:abstract\s+)?"
    r"(?:class|struct|interface|enum)\s+(\w+)"
)

# Pattern to extract method signatures
# func name<T>(params): ReturnType
METHOD_PATTERN = re.compile(
    r"(?:public\s+)?(?:static\s+)?func\s+(\w+)\s*"
    r"(?:<[^>]+>)?\s*\(([^)]*)\)\s*(?::\s*([^\{\n]+))?"
)


class MethodSignature(TypedDict):
    """Method signature extracted from code."""

    name: str
    signature: str
    params: str
    return_type: str | None


class StdlibInfo(TypedDict):
    """Stdlib information extracted from document."""

    is_stdlib: bool
    packages: list[str]
    type_names: list[str]
    method_names: list[MethodSignature]


def extract_stdlib_info(content: str) -> StdlibInfo:
    """Dynamically extract stdlib info from document content.

    Scans document content for `import std.*` statements and extracts
    package names, type declarations, and method signatures.

    Args:
        content: Markdown document content

    Returns:
        StdlibInfo with keys:
            - is_stdlib: Whether this document is stdlib-related
            - packages: List of package names (e.g., ["std.collection", "std.fs"])
            - type_names: List of type names defined in the document
            - method_names: List of method signature dicts
    """
    # Find all std.* imports
    packages: set[str] = set()
    for match in IMPORT_PATTERN.finditer(content):
        packages.add(match.group(1))  # e.g., "std.collection"

    # Check if this is stdlib documentation
    is_stdlib = bool(packages) or "import std." in content

    # Extract type declarations from code blocks
    type_names = extract_type_declarations(content)
    method_names = extract_method_signatures(content)

    return StdlibInfo(
        is_stdlib=is_stdlib,
        packages=list(packages),
        type_names=type_names,
        method_names=method_names,
    )


def extract_type_declarations(content: str) -> list[str]:
    """Extract class/struct/interface/enum names from code blocks.

    Args:
        content: Markdown document content

    Returns:
        List of unique type names found in the document
    """
    return list(set(TYPE_PATTERN.findall(content)))


def extract_method_signatures(content: str) -> list[MethodSignature]:
    """Extract method signatures from code blocks.

    Args:
        content: Markdown document content

    Returns:
        List of MethodSignature dicts with keys:
            - name: Method name
            - signature: Full signature string
            - params: Parameter string
            - return_type: Return type or None
    """
    methods: list[MethodSignature] = []
    seen_names: set[str] = set()

    for match in METHOD_PATTERN.finditer(content):
        name, params, return_type = match.groups()

        # Avoid duplicate method names
        if name in seen_names:
            continue
        seen_names.add(name)

        methods.append(
            MethodSignature(
                name=name,
                signature=match.group(0).strip(),
                params=params.strip() if params else "",
                return_type=return_type.strip() if return_type else None,
            )
        )

    return methods
