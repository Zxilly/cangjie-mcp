---
name: cangjie
description: Use when working with Cangjie (.cj) source files, asking about Cangjie language features or syntax, needing Cangjie API documentation, or performing code intelligence operations on Cangjie code. Cangjie is Huawei's programming language for HarmonyOS native development.
---

# Cangjie Language Assistant

## About Cangjie

Cangjie (仓颉) is a modern programming language developed by Huawei for building native applications on HarmonyOS. File extension: `.cj`.

**Core Features:**
- Strong static typing with powerful type inference
- Multi-paradigm: functional, imperative, and object-oriented
- Pattern matching with algebraic data types (enum + match)
- First-class functions and closures
- Lightweight user-mode threads (native coroutines via `spawn`)
- Seamless C/C++ interop via FFI
- Compile-time metaprogramming (macros)

### Key Differences from Other Languages

**vs Rust:**

| Feature | Rust | Cangjie |
|---------|------|---------|
| Variable shadowing | Allowed anywhere | **NOT allowed in same scope** |
| Mutability | `let` vs `let mut` | `let` vs `var` |
| Ownership | Borrow checker | Simple value/reference types |
| Null handling | `Option<T>` | `Option<T>` with auto-wrap |

**vs Swift/Kotlin:** Conditions **require parentheses** `if (x)`, interface uses `<:` not `:`, concurrency uses `spawn { => ... }`

**vs Java:** No `null` — use `Option`, default visibility is `internal` not `private`, functions are first-class

### Syntax Quick Reference

```cangjie
// Variables: let (immutable), var (mutable), const (compile-time)
let x = 10
var y = 20
// let x = 30  // ERROR: no shadowing in same scope!

// Types: struct = value type (copies), class = reference type (shares)
struct Point { var x: Int64; var y: Int64 }
class Node { var value: Int64 }

// Integers: Int8..Int64, UInt8..UInt64; Float: Float16/32/64
// Rune (char): r'a'; String: "Hello ${expr}"; Range: 1..=10 (inclusive), 1..10 (exclusive)

// Control flow (parentheses REQUIRED)
if (cond) { a } else { b }                  // expression
if (let Some(v) <- opt) { use(v) }          // pattern match in if
for (i in 0..10 where i % 2 == 0) { }       // for-in with filter
match (val) {
    case pattern => result                   // no braces needed
    case x where x > 0 => x                 // guard uses 'where', NOT 'if'
    case _ => default                        // exhaustive matching required
}

// Functions and lambdas
func add(a: Int64, b: Int64): Int64 { a + b }
func greet(name!: String = "World") { }      // named param with ! suffix
let f = { x: Int64 => x * 2 }               // lambda
x |> f |> g                                  // pipeline: g(f(x))

// Classes, interfaces, generics — use <: for inherit/implement
open class Animal { public open func speak() { } }  // 'open' for inheritance
class Dog <: Animal & Drawable { public override func speak() { } }
extend String { func rev(): String { } }     // extension
func id<T>(x: T): T where T <: Show { x }   // generic constraint

// Properties
class Temp {
    private var _c: Float64 = 0.0
    public prop celsius: Float64 { get() { _c } set(v) { _c = v } }
}

// Option type
let a: Option<Int64> = 100                   // auto-wraps to Some(100)
let v = opt ?? default                       // coalescing
obj?.property                                // optional chaining

// Collections
let arr: Array<Int64> = [1, 2, 3]            // fixed size
let list = ArrayList<String>()               // dynamic
let map = HashMap<String, Int64>()
let set = HashSet<Int64>()

// Concurrency and error handling
spawn { => doWork() }                        // lightweight thread
try { op() } catch (e: IOException) { }
try (r = open()) { use(r) }                  // auto-close resource

// Operators
x |> f |> g                                  // pipeline: g(f(x))
f ~> g                                       // composition
// Type check/cast: `is` and `as`

// Macros
@Test class MyTest { @TestCase func t() { @Assert(1+1, 2) } }

// Packages and visibility (default: internal)
package myapp.utils
import std.collection.*
// private | internal (DEFAULT) | protected | public
```

### Critical Pitfalls (Common LLM Errors)

```cangjie
// WRONG                          // CORRECT
let x = 1; let x = 2             // ERROR: no shadowing in same scope
if x > 0 { }                     if (x > 0) { }
"Value: {x}"                     "Value: ${x}"
case 1 if x > 0 => ...           case 1 where x > 0 => ...
class A : B { }                  class A <: B { }
spawn { x => ... }               spawn { => ... }  // no params
```

- `struct` copies on assignment, `class` shares reference
- Function parameters are immutable
- No labeled breaks (`break@label` not supported)
- Match must be exhaustive — cover all cases or use `_`

## Installation

```bash
# Run directly without installing (recommended)
uvx cangjie-mcp

# Install from PyPI
pip install cangjie-mcp

# Install from npm
npm install -g cangjie-mcp
```

All three methods provide the `cangjie-mcp` command:

```bash
cangjie-mcp query "generics"            # after pip/npm install
uvx cangjie-mcp query "generics"        # with uvx (no install needed)
npx cangjie-mcp query "generics"        # with npx (no install needed)
```

For LSP code intelligence, the Cangjie SDK must be installed in `PATH` or `CANGJIE_HOME` set.

## Configuration

### Config File

```bash
cangjie-mcp config path    # show config file location
cangjie-mcp config init    # create default config with all options commented out
```

Config file locations:
- **Linux:** `~/.config/cangjie/config.toml`
- **macOS:** `~/Library/Application Support/cangjie/config.toml`
- **Windows:** `%APPDATA%\cangjie\config.toml`

**Priority:** CLI flags > environment variables > config file > built-in defaults

Example `config.toml`:

```toml
# Documentation language: "zh" or "en"
lang = "zh"

# Embedding: "none" (BM25 only), "local", or "openai"
embedding = "none"

# Remote documentation server (skip local indexing)
server_url = "https://cj-mcp.learningman.top"

# OpenAI-compatible API (for embedding or rerank)
# openai_api_key = "sk-..."
# openai_base_url = "https://api.siliconflow.cn/v1"

# Daemon idle timeout in minutes
# daemon_timeout = 30
```

Run `cangjie-mcp config init` to generate a config file with all available options.

### Environment Variables

| Config Key | Environment Variable |
|---|---|
| `docs_version` | `CANGJIE_DOCS_VERSION` |
| `lang` | `CANGJIE_DOCS_LANG` |
| `embedding` | `CANGJIE_EMBEDDING_TYPE` |
| `openai_api_key` | `OPENAI_API_KEY` |
| `openai_base_url` | `OPENAI_BASE_URL` |
| `server_url` | `CANGJIE_SERVER_URL` |
| `data_dir` | `CANGJIE_DATA_DIR` |
| `daemon_timeout` | `CANGJIE_DAEMON_TIMEOUT` |

## Documentation Search

```bash
cangjie-mcp query "<search terms>"
cangjie-mcp query "<terms>" --category <cat> --top-k 10 --offset 0
cangjie-mcp query "<terms>" --extract-code              # extract code examples
cangjie-mcp query "<terms>" --package std.collection     # filter by stdlib package
cangjie-mcp topic <topic-name>
cangjie-mcp topic <topic-name> --category <cat>
cangjie-mcp topics
cangjie-mcp topics --category <cat>
```

**Recommended workflow:**
1. `cangjie-mcp topics` — discover categories and topic names (includes names by default)
2. `cangjie-mcp query "<terms>"` — semantic search for concepts (returns code examples by default)
3. `cangjie-mcp query "<terms>" --package <pkg>` — search stdlib APIs
4. `cangjie-mcp topic <name>` — retrieve full documentation for a specific topic

## LSP Code Intelligence

```bash
# By symbol name
cangjie-mcp lsp definition <file> --symbol <name>
cangjie-mcp lsp references <file> --symbol <name>
cangjie-mcp lsp hover <file> --symbol <name>
cangjie-mcp lsp incoming-calls <file> --symbol <name>
cangjie-mcp lsp outgoing-calls <file> --symbol <name>
cangjie-mcp lsp type-supertypes <file> --symbol <name>
cangjie-mcp lsp type-subtypes <file> --symbol <name>

# By position (1-based line and character)
cangjie-mcp lsp definition <file> --line N --character N

# File-scoped
cangjie-mcp lsp symbols <file>
cangjie-mcp lsp diagnostics <file>

# Workspace-scoped
cangjie-mcp lsp workspace-symbol <query>
```

## Daemon Management

The background daemon auto-starts on first use and stops after idle timeout. Manual control:

```bash
cangjie-mcp daemon status
cangjie-mcp daemon stop
cangjie-mcp daemon logs --tail 50
cangjie-mcp daemon logs --follow
```

## Output Format

- `query` — Markdown with ranked results, one section per match
- `topic` — plain text documentation content with title header
- `topics` — readable list grouped by category
- `lsp` — JSON (structured data for programmatic use)
