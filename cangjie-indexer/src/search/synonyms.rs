use std::collections::HashMap;
use std::sync::LazyLock;

/// Domain-specific synonym groups for Cangjie programming language documentation.
/// Each group contains terms that are semantically equivalent in this context.
static SYNONYM_GROUPS: LazyLock<Vec<Vec<&'static str>>> = LazyLock::new(|| {
    vec![
        // OOP & type definitions
        vec!["\u{7c7b}", "class"],
        vec![
            "\u{51fd}\u{6570}",
            "func",
            "function",
            "\u{65b9}\u{6cd5}",
            "method",
        ],
        vec!["\u{53d8}\u{91cf}", "var", "variable", "let"],
        vec!["\u{63a5}\u{53e3}", "interface"],
        vec![
            "\u{679a}\u{4e3e}",
            "enum",
            "\u{679a}\u{4e3e}\u{7c7b}\u{578b}",
        ],
        vec!["\u{6570}\u{7ec4}", "array"],
        vec!["\u{7ed3}\u{6784}\u{4f53}", "struct"],
        vec![
            "\u{6cdb}\u{578b}",
            "generic",
            "generics",
            "\u{6cdb}\u{578b}\u{53c2}\u{6570}",
        ],
        vec![
            "\u{95ed}\u{5305}",
            "lambda",
            "\u{533f}\u{540d}\u{51fd}\u{6570}",
        ],
        vec![
            "\u{7c7b}\u{578b}",
            "type",
            "\u{7c7b}\u{578b}\u{7cfb}\u{7edf}",
        ],
        vec![
            "\u{7ee7}\u{627f}",
            "extend",
            "extends",
            "inherit",
            "inheritance",
        ],
        vec!["\u{5b9e}\u{73b0}", "implement", "impl"],
        vec!["\u{62bd}\u{8c61}", "abstract"],
        vec!["\u{5bc6}\u{5c01}", "sealed"],
        vec!["\u{5f00}\u{653e}", "open"],
        // Error handling
        vec![
            "\u{5f02}\u{5e38}",
            "exception",
            "\u{5f02}\u{5e38}\u{5904}\u{7406}",
        ],
        vec![
            "\u{9519}\u{8bef}\u{5904}\u{7406}",
            "error handling",
            "try",
            "catch",
        ],
        // Pattern matching & control flow
        vec![
            "\u{6a21}\u{5f0f}\u{5339}\u{914d}",
            "pattern matching",
            "match",
        ],
        vec!["\u{6761}\u{4ef6}", "if", "condition"],
        vec!["\u{5faa}\u{73af}", "loop", "for", "while"],
        // Modules & packages
        vec!["\u{5305}", "package"],
        vec!["\u{6a21}\u{5757}", "module"],
        vec!["\u{5bfc}\u{5165}", "import"],
        // Concurrency
        vec!["\u{7ebf}\u{7a0b}", "thread"],
        vec![
            "\u{5e76}\u{53d1}",
            "concurrent",
            "concurrency",
            "\u{5e76}\u{884c}",
        ],
        vec!["\u{534f}\u{7a0b}", "coroutine"],
        // Collections
        vec!["\u{96c6}\u{5408}", "collection"],
        vec![
            "\u{6620}\u{5c04}",
            "map",
            "hashmap",
            "\u{54c8}\u{5e0c}\u{8868}",
        ],
        vec!["\u{5143}\u{7ec4}", "tuple"],
        vec!["\u{5217}\u{8868}", "list"],
        // Operators & features
        vec![
            "\u{8fd0}\u{7b97}\u{7b26}",
            "operator",
            "\u{64cd}\u{4f5c}\u{7b26}",
        ],
        vec![
            "\u{91cd}\u{8f7d}",
            "overload",
            "\u{8fd0}\u{7b97}\u{7b26}\u{91cd}\u{8f7d}",
            "operator overloading",
        ],
        vec!["\u{6ce8}\u{89e3}", "annotation"],
        vec!["\u{5b8f}", "macro"],
        vec![
            "\u{5c5e}\u{6027}",
            "prop",
            "property",
            "\u{5c5e}\u{6027}\u{8bbf}\u{95ee}",
        ],
        // Access control
        vec!["\u{516c}\u{5f00}", "public", "pub"],
        vec!["\u{79c1}\u{6709}", "private"],
        vec!["\u{53d7}\u{4fdd}\u{62a4}", "protected"],
        // Memory & lifecycle
        vec!["\u{5f15}\u{7528}", "reference", "ref"],
        vec!["\u{53ef}\u{53d8}", "mut", "mutable"],
        vec!["\u{4e0d}\u{53ef}\u{53d8}", "immutable"],
        // Common concepts
        vec!["\u{5b57}\u{7b26}\u{4e32}", "string"],
        vec!["\u{6574}\u{6570}", "int", "integer"],
        vec!["\u{6d6e}\u{70b9}", "float", "\u{6d6e}\u{70b9}\u{6570}"],
        vec!["\u{5e03}\u{5c14}", "bool", "boolean"],
        vec!["\u{7a7a}\u{503c}", "null", "none", "nil"],
        vec!["\u{8fd4}\u{56de}", "return"],
        vec![
            "\u{6784}\u{9020}",
            "init",
            "constructor",
            "\u{6784}\u{9020}\u{51fd}\u{6570}",
        ],
        vec![
            "\u{6790}\u{6784}",
            "destructor",
            "\u{6790}\u{6784}\u{51fd}\u{6570}",
        ],
        vec![
            "\u{8fed}\u{4ee3}",
            "iterator",
            "\u{8fed}\u{4ee3}\u{5668}",
            "iter",
        ],
        vec![
            "\u{6d4b}\u{8bd5}",
            "test",
            "\u{5355}\u{5143}\u{6d4b}\u{8bd5}",
        ],
        vec![
            "\u{6587}\u{6863}",
            "doc",
            "documentation",
            "\u{6ce8}\u{91ca}",
        ],
        vec![
            "\u{7f16}\u{8bd1}",
            "compile",
            "\u{7f16}\u{8bd1}\u{5668}",
            "compiler",
        ],
        vec!["\u{58f0}\u{660e}", "declaration", "declare"],
        vec!["\u{5b9a}\u{4e49}", "definition", "define"],
        vec!["\u{8868}\u{8fbe}\u{5f0f}", "expression", "expr"],
        vec!["\u{8bed}\u{53e5}", "statement"],
    ]
});

/// Lookup table: token -> list of all synonyms (including the token itself).
pub static SYNONYM_MAP: LazyLock<HashMap<&'static str, &'static [&'static str]>> =
    LazyLock::new(|| {
        let mut map = HashMap::new();
        for group in SYNONYM_GROUPS.iter() {
            let slice: &'static [&'static str] = group.as_slice();
            for &term in slice {
                map.insert(term, slice);
            }
        }
        map
    });

/// Expand pre-tokenized query terms by replacing tokens that have known
/// synonyms with OR-clause groups.
///
/// Example: `&["函数", "定义"]` -> `"(函数 OR func OR function OR 方法 OR method) (定义 OR definition OR define)"`
pub fn expand_query(tokens: &[&str]) -> String {
    if tokens.is_empty() {
        return String::new();
    }

    let mut parts: Vec<String> = Vec::with_capacity(tokens.len());

    for token in tokens {
        if let Some(group) = SYNONYM_MAP.get(token) {
            if group.len() > 1 {
                parts.push(format!("({})", group.join(" OR ")));
            } else {
                parts.push(token.to_string());
            }
        } else {
            parts.push(token.to_string());
        }
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_with_synonyms() {
        let expanded = expand_query(&["\u{51fd}\u{6570}"]);
        assert!(expanded.contains("func"));
        assert!(expanded.contains("function"));
        assert!(expanded.contains("\u{51fd}\u{6570}"));
    }

    #[test]
    fn test_expand_no_synonyms() {
        let expanded = expand_query(&["\u{4ed3}\u{9889}"]);
        assert_eq!(expanded.trim(), "\u{4ed3}\u{9889}");
    }

    #[test]
    fn test_expand_mixed() {
        let expanded = expand_query(&["\u{53d8}\u{91cf}", "\u{58f0}\u{660e}"]);
        assert!(expanded.contains("var"));
        assert!(expanded.contains("variable"));
        assert!(expanded.contains("declaration"));
    }

    #[test]
    fn test_expand_empty() {
        let expanded = expand_query(&[]);
        assert_eq!(expanded, "");
    }

    #[test]
    fn test_synonym_map_bidirectional() {
        // "class" should map back to the same group as "类"
        let group_class = SYNONYM_MAP.get("class").unwrap();
        let group_lei = SYNONYM_MAP.get("\u{7c7b}").unwrap();
        assert_eq!(
            group_class.len(),
            group_lei.len(),
            "class and \u{7c7b} should be in the same synonym group"
        );
    }
}
