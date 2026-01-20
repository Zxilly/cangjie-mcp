"""FastMCP server for Cangjie documentation."""

from __future__ import annotations

from typing import TYPE_CHECKING

from mcp.server.fastmcp import FastMCP
from mcp.types import ToolAnnotations

from cangjie_mcp.config import IndexKey, Settings
from cangjie_mcp.server import tools
from cangjie_mcp.server.tools import (
    CodeExample,
    GetCodeExamplesInput,
    GetToolUsageInput,
    GetTopicInput,
    ListTopicsInput,
    SearchDocsInput,
    SearchResult,
    ToolContext,
    ToolUsageResult,
    TopicResult,
    TopicsListResult,
)

if TYPE_CHECKING:
    from cangjie_mcp.indexer.document_source import DocumentSource
    from cangjie_mcp.indexer.store import VectorStore


# =============================================================================
# Server Instructions
# =============================================================================

SERVER_INSTRUCTIONS = """
Cangjie Documentation Server - Semantic search and retrieval for Cangjie programming language.

## About Cangjie
Cangjie is a modern programming language developed by Huawei for building native applications
on HarmonyOS. File extension: `.cj`. Features: strong static typing with type inference,
pattern matching, first-class functions, built-in concurrency, and seamless C/C++ interop.

## Syntax Reference

### 1. Variables

Declaration syntax: `modifier name: Type = value`

Mutability modifiers:
- `let` - immutable variable (can only be assigned once at initialization)
- `var` - mutable variable (can be reassigned)

IMPORTANT: `let` does NOT support variable shadowing like Rust. Cannot redeclare same name in same scope.

Visibility modifiers:
- `private` - visible only within class definition
- `internal` - visible in current package and subpackages (DEFAULT)
- `protected` - visible in current module and subclasses
- `public` - visible everywhere

Static modifier: `static` - affects member storage and reference

Example:
```
main() {
    let a: Int64 = 20      // immutable
    var b: Int64 = 12      // mutable
    b = 23                 // OK: var can be reassigned
    println("${a}${b}")
}
```

### 2. Basic Types

Signed integers: `Int8`, `Int16`, `Int32`, `Int64`, `IntNative`
Unsigned integers: `UInt8`, `UInt16`, `UInt32`, `UInt64`, `UIntNative`
Floating point: `Float16`, `Float32`, `Float64`
Boolean: `true`, `false`

Character (Rune):
- Represents Unicode characters: `let a: Rune = r'a'`
- Escape sequences: `r'\\n'`, `r'\\t'`, `r'\\\\'`
- Unicode literals: `r'\\u{4f60}'` (Chinese character)
- Convert to UInt32: `UInt32(rune)`
- Convert from int: `Rune(num)` (must be valid Unicode range)

String:
- Basic: `"Hello Cangjie"`
- Interpolation: `"Count: ${count * 2}"` (uses `${}` NOT `{}`)
- To rune array: `"hello".toRuneArray()`
- Raw multiline (no escaping): `#"raw \\n stays as backslash-n"#`
- Multi-hash raw: `##"can contain #"##`

Tuple:
- Type: `(T1, T2, ..., TN)` (minimum 2 elements)
- Access by index: `tuple[0]`, `tuple[1]`
- Example: `var t = (true, 42); println(t[0])`

Array: `Array<T>` - element type can be any type

Range:
- Type: `Range<T>` with `start`, `end`, `step` (step is Int64, cannot be 0)
- Inclusive: `1..=10` (1 to 10 including 10)
- Exclusive: `1..10` (1 to 9)

Unit:
- Single value: `()`
- Only supports: assignment, equality, inequality
- Does NOT support other operations

### 3. Control Flow

IMPORTANT: Parentheses around conditions are REQUIRED (unlike some languages).

if expression:
```
if (condition) {
    branch1
} else {
    branch2
}
```

Pattern matching in if (let pattern):
```
let opt = Some(3)
if (let Some(value) <- opt) {
    println("Got ${value}")
}
```

while expression:
```
while (condition) { body }
do { body } while (condition)
```

for-in expression:
```
for (item in collection) { body }
for (i in 1..=100) { sum += i }        // range iteration
for ((x, y) in tupleArray) { ... }     // tuple destructuring
```

Jump control:
- `break` and `continue` are supported
- NO labeled jumps (cannot break/continue to outer loop)
- NO `goto` statement

### 4. Functions

Functions are first-class citizens (can be passed, returned, assigned).

Function type syntax: `(ParamTypes) -> ReturnType`

Definition:
```
func add(a: Int64, b: Int64): Int64 {
    return a + b
}

// Implicit return (last expression)
func add(a: Int64, b: Int64): Int64 {
    a + b
}
```

Named parameters with default values:
```
func greet(name!: String = "World") {
    println("Hello ${name}")
}
greet()                    // uses default
greet(name: "Alice")       // named argument
```

IMPORTANT: Function parameters are immutable (`let`) by default.

Lambda expressions:
```
// Full syntax
{ param1: Type1, param2: Type2 => expression }

// Examples
let f = { a: Int64, b: Int64 => a + b }
var display = { => println("Hello") }           // no params
var sum: (Int64, Int64) -> Int64 = { a, b => a + b }  // type inference

// Immediate invocation
let result = { => 123 }()    // result = 123
```

### 5. Enum Types

Definition (constructors separated by `|`):
```
enum RGBColor {
    | Red | Green | Blue
}

enum Color {
    | Red(UInt8)
    | Green(UInt8)
    | Blue(UInt8)
}
```

Constructors: parameterless `Name` or with params `Name(p1, p2, ...)`

### 6. Pattern Matching

match expression:
```
match (value) {
    case pattern1 => result1
    case pattern2 => result2
    case _ => default_result    // wildcard
}
```

IMPORTANT: case does NOT need braces `{}`, just `=>` followed by expression.

Pattern types:

1. Constant pattern: integers, floats, chars, bools, strings, Unit
   - String interpolation NOT supported in patterns

2. Wildcard pattern: `_` matches anything

3. Binding pattern: identifier captures matched value
   ```
   case n => "value is ${n}"
   ```

4. Tuple pattern:
   ```
   case ("Alice", age) => "Alice is ${age}"
   case (_, _) => "unknown"
   ```

5. Type pattern:
   ```
   case x: SomeType => x.method()
   ```

6. Enum pattern:
   ```
   case Year(n) => "${n * 12} months"
   case TimeUnit.Month(n) => "${n} months"   // qualified name
   ```

7. Multiple values: `case 2 | 3 | 4 => "small"`

Nested patterns allowed in tuples and enums:
```
case (SetTimeUnit(Year(year)), _) => println("Year: ${year}")
```

### 7. Option Type

Definition:
```
enum Option<T> {
    | Some(T)   // has value
    | None      // no value
}
```

Common operations:
- Unwrap with default: `opt ?? defaultValue`
- Check: `opt.isSome()`, `opt.isNone()`
- Pattern match: `if (let Some(v) <- opt) { ... }`
- Early return: `opt ?? return errorValue`
- Throw on None: `opt ?? throw Exception("error")`

### 8. Class Types

Definition:
```
class ClassName {
    // member variables
    // member properties
    // static initializers
    // constructors (init)
    // member functions
    // operator functions
}
```

Example:
```
class Rectangle {
    let width: Int64
    let height: Int64

    public init(width: Int64, height: Int64) {
        this.width = width
        this.height = height
    }

    public func area(): Int64 {
        width * height
    }
}

let rect = Rectangle(10, 20)
let h = rect.height    // h = 20
```

IMPORTANT: Member default visibility is `internal`, NOT `private` or `public`.

### 9. Collections

All use `add()` to add, `[]` to modify, `remove()` to delete.

- `Array<T>`: fixed size, modify with `[]`
  - Literal: `let arr: Array<String> = ["A", "B", "C"]`

- `ArrayList<T>`: dynamic, frequent add/remove
  - `list.add(item)`, `list.remove(at: index)`

- `HashSet<T>`: unique elements only

- `HashMap<K, V>`: key-value mapping
  - Literal: `let map = HashMap(("A", 1), ("B", 2))`

### 10. Packages

Directory structure determines package hierarchy:
```
src/
  demo/
    sub/
      a.cj    // package demo.sub
    b.cj      // package demo
  main.cj     // package demo
```

Declaration: `package demo.subpackage`

Import:
```
import std.math.*                           // all from package
import package1.foo                         // single item
import {package1.foo, package2.Bar}         // multiple items
```

### 11. Unit Testing

Test macros:
- `@Test` - marks class or function as test
- `@TestCase` - marks method as test case
- `@Fail("message")` - explicit failure

Assertions:
- `@Assert(left, right)` - equality check, stops on failure
- `@Assert(condition)` - condition check, stops on failure
- `@Expect(left, right)` - equality check, continues on failure
- `@Expect(condition)` - condition check, continues on failure

Example:
```
@Test
class CalculatorTest {
    @TestCase
    func testAdd() {
        let result = add(2, 3)
        @Assert(result, 5)
    }
}
```

## Critical Pitfalls (Common Errors)

1. `let` does NOT support shadowing - cannot redeclare same variable name in same scope
2. Condition parentheses REQUIRED: `if (x)` not `if x`
3. String interpolation uses `${}` not `{}`
4. Class members default to `internal`, NOT `private`
5. Function parameters are immutable (`let`) by default
6. match case does NOT need braces: `case 1 => expr` not `case 1 => { expr }`
7. NO `goto`, NO labeled break/continue
8. `Unit` type only supports assignment and equality operations
9. Rune must be in valid Unicode range when converting from integer
10. Raw strings (`#"..."#`) do NOT process escape sequences
11. `Duration` and `sleep` are in `std.core` (no import needed)

## Available Tools

- `cangjie_search_docs`: Semantic search across documentation with pagination
- `cangjie_get_topic`: Get complete documentation for a specific topic
- `cangjie_list_topics`: List available documentation topics by category
- `cangjie_get_code_examples`: Get code examples for a language feature
- `cangjie_get_tool_usage`: Get usage information for Cangjie CLI tools (cjc, cjpm, cjfmt)

## Recommended Workflow

1. Use `cangjie_list_topics` to discover available categories and topics
2. Use `cangjie_search_docs` for semantic queries about concepts
3. Use `cangjie_get_topic` to retrieve full documentation for known topics
4. Use `cangjie_get_code_examples` to find practical code examples
5. Use `cangjie_get_tool_usage` for CLI tool documentation
""".strip()


# =============================================================================
# Tool Registration
# =============================================================================


def _register_tools(mcp: FastMCP, ctx: ToolContext) -> None:
    """Register all MCP tools with the server.

    This function is shared between create_mcp_server and create_mcp_server_with_store
    to avoid code duplication.

    Args:
        mcp: FastMCP server instance
        ctx: Tool context with store and loader
    """

    @mcp.tool(
        name="cangjie_search_docs",
        annotations=ToolAnnotations(
            title="Search Cangjie Documentation",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    def cangjie_search_docs(params: SearchDocsInput) -> SearchResult:
        """Search Cangjie documentation using semantic search.

        Performs vector similarity search across all indexed documentation.
        Returns matching sections ranked by relevance with pagination support.

        Args:
            params: Search parameters including:
                - query (str): Search query describing what you're looking for
                - category (str | None): Optional category filter (e.g., 'cjpm', 'syntax')
                - top_k (int): Number of results to return (default: 5, max: 20)
                - offset (int): Pagination offset (default: 0)

        Returns:
            SearchResult containing:
                - items: List of matching documents with content, score, and metadata
                - total: Estimated total matches
                - count: Number of items in this response
                - offset: Current pagination offset
                - has_more: Whether more results are available
                - next_offset: Next offset for pagination (or None)

        Examples:
            - Query: "how to define a class" -> Returns class definition docs
            - Query: "pattern matching syntax" -> Returns pattern matching docs
            - Query: "async programming" with category="stdlib" -> Filters to stdlib
        """
        return tools.search_docs(ctx, params)

    @mcp.tool(
        name="cangjie_get_topic",
        annotations=ToolAnnotations(
            title="Get Documentation Topic",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    def cangjie_get_topic(params: GetTopicInput) -> TopicResult | str:
        """Get complete documentation for a specific topic.

        Retrieves the full content of a documentation file by topic name.
        Use cangjie_list_topics first to discover available topic names.

        Args:
            params: Input parameters including:
                - topic (str): Topic name (file name without .md extension)
                - category (str | None): Optional category to narrow search

        Returns:
            TopicResult with full document content and metadata, or error string if not found.
            TopicResult contains:
                - content: Full markdown content of the document
                - file_path: Path to the source file
                - category: Document category
                - topic: Topic name
                - title: Document title

        Examples:
            - topic="classes" -> Returns full class documentation
            - topic="pattern-matching", category="syntax" -> Specific category lookup
        """
        result = tools.get_topic(ctx, params)
        return result if result else f"Topic '{params.topic}' not found"

    @mcp.tool(
        name="cangjie_list_topics",
        annotations=ToolAnnotations(
            title="List Documentation Topics",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    def cangjie_list_topics(params: ListTopicsInput) -> TopicsListResult:
        """List available documentation topics organized by category.

        Returns all documentation topics, optionally filtered by category.
        Use this to discover topic names for use with cangjie_get_topic.

        Args:
            params: Input parameters including:
                - category (str | None): Optional category filter

        Returns:
            TopicsListResult containing:
                - categories: Dict mapping category names to lists of topic names
                - total_categories: Number of categories
                - total_topics: Total number of topics across all categories

        Examples:
            - No params -> Returns all categories and their topics
            - category="cjpm" -> Returns only cjpm-related topics
        """
        return tools.list_topics(ctx, params)

    @mcp.tool(
        name="cangjie_get_code_examples",
        annotations=ToolAnnotations(
            title="Get Code Examples",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    def cangjie_get_code_examples(params: GetCodeExamplesInput) -> list[CodeExample]:
        """Get code examples for a specific Cangjie language feature.

        Searches documentation for code blocks related to a feature.
        Returns extracted code examples with their surrounding context.

        Args:
            params: Input parameters including:
                - feature (str): Feature to find examples for
                - top_k (int): Number of documents to search (default: 3)

        Returns:
            List of CodeExample objects, each containing:
                - language: Programming language of the code block
                - code: The actual code content
                - context: Surrounding text providing context
                - source_topic: Topic where the example was found
                - source_file: Source file path

        Examples:
            - feature="pattern matching" -> Pattern matching code examples
            - feature="generics" -> Generic type usage examples
            - feature="async/await" -> Async programming examples
        """
        return tools.get_code_examples(ctx, params)

    @mcp.tool(
        name="cangjie_get_tool_usage",
        annotations=ToolAnnotations(
            title="Get Tool Usage",
            readOnlyHint=True,
            destructiveHint=False,
            idempotentHint=True,
            openWorldHint=False,
        ),
    )
    def cangjie_get_tool_usage(params: GetToolUsageInput) -> ToolUsageResult | str:
        """Get usage information for Cangjie development tools.

        Searches for documentation about Cangjie CLI tools including
        compiler, package manager, formatter, and other utilities.

        Args:
            params: Input parameters including:
                - tool_name (str): Name of the tool (e.g., 'cjc', 'cjpm', 'cjfmt')

        Returns:
            ToolUsageResult with documentation and examples, or error string if not found.
            ToolUsageResult contains:
                - tool_name: Name of the tool
                - content: Combined documentation content
                - examples: List of shell command examples with context

        Examples:
            - tool_name="cjc" -> Compiler usage and options
            - tool_name="cjpm" -> Package manager commands
            - tool_name="cjfmt" -> Code formatter usage
        """
        result = tools.get_tool_usage(ctx, params)
        return result if result else f"No usage information found for tool '{params.tool_name}'"


# =============================================================================
# Server Factory Functions
# =============================================================================


def create_mcp_server(settings: Settings) -> FastMCP:
    """Create and configure the MCP server.

    Creates a FastMCP server with all Cangjie documentation tools registered.
    The VectorStore is initialized from settings.

    Args:
        settings: Application settings including paths and embedding config

    Returns:
        Configured FastMCP instance ready to serve requests
    """
    mcp = FastMCP(
        name="cangjie_mcp",
        instructions=SERVER_INSTRUCTIONS,
    )

    ctx = tools.create_tool_context(settings)
    _register_tools(mcp, ctx)

    return mcp


def create_mcp_server_with_store(
    settings: Settings,
    store: VectorStore,
    key: IndexKey | None = None,
    document_source: DocumentSource | None = None,
) -> FastMCP:
    """Create and configure the MCP server with a pre-loaded VectorStore.

    This is used by the HTTP server to create MCP instances for each index,
    where the VectorStore is already loaded by MultiIndexStore.

    Args:
        settings: Application settings
        store: Pre-loaded VectorStore instance
        key: Optional IndexKey for naming the server instance
        document_source: Optional DocumentSource for reading documentation

    Returns:
        Configured FastMCP instance
    """
    server_name = f"cangjie_mcp_{key.version}_{key.lang}" if key else "cangjie_mcp"

    instructions = SERVER_INSTRUCTIONS
    if key:
        instructions = f"Index: {key.version} ({key.lang})\n\n{instructions}"

    mcp = FastMCP(
        name=server_name,
        instructions=instructions,
    )

    ctx = tools.create_tool_context(settings, store=store, document_source=document_source)
    _register_tools(mcp, ctx)

    return mcp
