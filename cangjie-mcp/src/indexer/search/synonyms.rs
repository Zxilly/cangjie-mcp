use std::collections::HashMap;
use std::sync::LazyLock;

/// Domain-specific synonym groups for Cangjie programming language documentation.
/// Each group contains terms that are semantically equivalent in this context.
static SYNONYM_GROUPS: LazyLock<Vec<Vec<&'static str>>> = LazyLock::new(|| {
    vec![
        // OOP & type definitions
        vec!["类", "class"],
        vec!["函数", "func", "function", "方法", "method"],
        vec!["变量", "var", "variable", "let"],
        vec!["接口", "interface"],
        vec!["枚举", "enum", "枚举类型"],
        vec!["数组", "array"],
        vec!["结构体", "struct"],
        vec!["泛型", "generic", "generics", "泛型参数"],
        vec!["闭包", "lambda", "匿名函数"],
        vec!["类型", "type", "类型系统"],
        vec!["继承", "extend", "extends", "inherit", "inheritance"],
        vec!["实现", "implement", "impl"],
        vec!["抽象", "abstract"],
        vec!["密封", "sealed"],
        vec!["开放", "open"],
        // Error handling
        vec!["异常", "exception", "异常处理"],
        vec!["错误处理", "error handling", "try", "catch"],
        // Pattern matching & control flow
        vec!["模式匹配", "pattern matching", "match"],
        vec!["条件", "if", "condition"],
        vec!["循环", "loop", "for", "while"],
        // Modules & packages
        vec!["包", "package"],
        vec!["模块", "module"],
        vec!["导入", "import"],
        // Concurrency
        vec!["线程", "thread"],
        vec!["并发", "concurrent", "concurrency", "并行"],
        vec!["协程", "coroutine"],
        // Collections
        vec!["集合", "collection"],
        vec!["映射", "map", "hashmap", "哈希表"],
        vec!["元组", "tuple"],
        vec!["列表", "list"],
        // Operators & features
        vec!["运算符", "operator", "操作符"],
        vec!["重载", "overload", "运算符重载", "operator overloading"],
        vec!["注解", "annotation"],
        vec!["宏", "macro"],
        vec!["属性", "prop", "property", "属性访问"],
        // Access control
        vec!["公开", "public", "pub"],
        vec!["私有", "private"],
        vec!["受保护", "protected"],
        // Memory & lifecycle
        vec!["引用", "reference", "ref"],
        vec!["可变", "mut", "mutable"],
        vec!["不可变", "immutable"],
        // Common concepts
        vec!["字符串", "string"],
        vec!["整数", "int", "integer"],
        vec!["浮点", "float", "浮点数"],
        vec!["布尔", "bool", "boolean"],
        vec!["空值", "null", "none", "nil"],
        vec!["返回", "return"],
        vec!["构造", "init", "constructor", "构造函数"],
        vec!["析构", "destructor", "析构函数"],
        vec!["迭代", "iterator", "迭代器", "iter"],
        vec!["测试", "test", "单元测试"],
        vec!["文档", "doc", "documentation", "注释"],
        vec!["编译", "compile", "编译器", "compiler"],
        vec!["声明", "declaration", "declare"],
        vec!["定义", "definition", "define"],
        vec!["表达式", "expression", "expr"],
        vec!["语句", "statement"],
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
        let expanded = expand_query(&["函数"]);
        assert!(expanded.contains("func"));
        assert!(expanded.contains("function"));
        assert!(expanded.contains("函数"));
    }

    #[test]
    fn test_expand_no_synonyms() {
        let expanded = expand_query(&["仓颉"]);
        assert_eq!(expanded.trim(), "仓颉");
    }

    #[test]
    fn test_expand_mixed() {
        let expanded = expand_query(&["变量", "声明"]);
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
        let group_lei = SYNONYM_MAP.get("类").unwrap();
        assert_eq!(
            group_class.len(),
            group_lei.len(),
            "class and 类 should be in the same synonym group"
        );
    }
}
