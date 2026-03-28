use std::collections::HashMap;
use std::path::PathBuf;

use cangjie_core::config::{DocLang, EmbeddingType, RerankType, Settings};
use cangjie_indexer::document::source::DocumentSource;
use cangjie_indexer::{DocData, DocMetadata, TextChunk};

/// Create a BM25-only `Settings` suitable for testing.
pub fn test_settings(data_dir: PathBuf) -> Settings {
    Settings {
        docs_lang: DocLang::Zh,
        embedding_type: EmbeddingType::None,
        rerank_type: RerankType::None,
        docs_version: "test".to_string(),
        data_dir,
        openai_model: String::new(),
        ..Settings::default()
    }
}

/// Return a set of realistic `TextChunk` values spanning multiple categories.
pub fn sample_chunks() -> Vec<TextChunk> {
    vec![
        // ── syntax category ────────────────────────────────────────────
        TextChunk {
            text: "# 函数定义\n\n仓颉语言使用 `func` 关键字定义函数。函数可以有参数和返回值。\n\n```cangjie\nfunc add(a: Int, b: Int): Int {\n    return a + b\n}\n```\n\n函数是仓颉程序的基本构建块。".to_string(),
            metadata: DocMetadata {
                file_path: "syntax/functions.md".to_string(),
                category: "syntax".to_string(),
                topic: "functions".to_string(),
                title: "函数定义".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# 变量与常量\n\n使用 `let` 声明不可变变量，使用 `var` 声明可变变量。\n\n```cangjie\nlet x: Int = 10\nvar y: String = \"hello\"\n```\n\n仓颉的类型系统是静态强类型的。".to_string(),
            metadata: DocMetadata {
                file_path: "syntax/variables.md".to_string(),
                category: "syntax".to_string(),
                topic: "variables".to_string(),
                title: "变量与常量".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# 控制流\n\n仓颉支持 `if`、`while`、`for` 等控制流语句。\n\n```cangjie\nif condition {\n    // do something\n}\n```\n\n模式匹配使用 `match` 表达式。".to_string(),
            metadata: DocMetadata {
                file_path: "syntax/control_flow.md".to_string(),
                category: "syntax".to_string(),
                topic: "control_flow".to_string(),
                title: "控制流".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# 类型系统\n\n仓颉拥有强大的类型系统，支持泛型、接口和代数数据类型。\n\n基本类型包括 Int、Float、Bool、String 等。".to_string(),
            metadata: DocMetadata {
                file_path: "syntax/types.md".to_string(),
                category: "syntax".to_string(),
                topic: "types".to_string(),
                title: "类型系统".to_string(),
                code_block_count: 0,
                has_code: false,
                ..Default::default()
            },
        },
        // ── stdlib category ────────────────────────────────────────────
        TextChunk {
            text: "# 集合类型\n\n标准库提供了 Array、HashMap、HashSet 等集合类型。\n\n```cangjie\nlet arr = Array<Int>([1, 2, 3])\nlet map = HashMap<String, Int>()\n```\n\n集合类型支持迭代器操作。".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/collections.md".to_string(),
                category: "stdlib".to_string(),
                topic: "collections".to_string(),
                title: "集合类型".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# IO 操作\n\n仓颉标准库提供文件读写和网络IO功能。\n\n```cangjie\nlet file = File.open(\"data.txt\")\nlet content = file.readAll()\n```\n\n支持异步IO操作。".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/io.md".to_string(),
                category: "stdlib".to_string(),
                topic: "io".to_string(),
                title: "IO 操作".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# 字符串处理\n\n标准库提供丰富的字符串操作方法，包括分割、替换、格式化等。\n\nString 类型是 UTF-8 编码的不可变字符序列。".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/string.md".to_string(),
                category: "stdlib".to_string(),
                topic: "string".to_string(),
                title: "字符串处理".to_string(),
                code_block_count: 0,
                has_code: false,
                ..Default::default()
            },
        },
        // ── cjpm category ──────────────────────────────────────────────
        TextChunk {
            text: "# 包管理器 CJPM\n\nCJPM 是仓颉的官方包管理工具。使用 `cjpm init` 创建新项目。\n\n```bash\ncjpm init my-project\ncjpm build\ncjpm test\n```\n\nCJPM 使用 cjpm.toml 文件管理依赖。".to_string(),
            metadata: DocMetadata {
                file_path: "cjpm/getting_started.md".to_string(),
                category: "cjpm".to_string(),
                topic: "getting_started".to_string(),
                title: "包管理器 CJPM".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# 依赖管理\n\n在 cjpm.toml 中声明依赖项，CJPM 会自动解析版本约束。\n\n```toml\n[dependencies]\nhttp = \"1.0.0\"\njson = \"2.3.0\"\n```\n\n使用 `cjpm update` 更新依赖。".to_string(),
            metadata: DocMetadata {
                file_path: "cjpm/dependencies.md".to_string(),
                category: "cjpm".to_string(),
                topic: "dependencies".to_string(),
                title: "依赖管理".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# 项目结构\n\nCJPM 项目标准目录结构如下：\n\n- src/ — 源代码\n- tests/ — 测试文件\n- cjpm.toml — 项目配置\n\n模块系统与目录结构对应。".to_string(),
            metadata: DocMetadata {
                file_path: "cjpm/project_structure.md".to_string(),
                category: "cjpm".to_string(),
                topic: "project_structure".to_string(),
                title: "项目结构".to_string(),
                code_block_count: 0,
                has_code: false,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# 错误处理\n\n仓颉使用 Result 和 Option 类型进行错误处理。\n\n```cangjie\nfunc divide(a: Int, b: Int): Result<Int, String> {\n    if b == 0 {\n        return Err(\"division by zero\")\n    }\n    return Ok(a / b)\n}\n```".to_string(),
            metadata: DocMetadata {
                file_path: "syntax/error_handling.md".to_string(),
                category: "syntax".to_string(),
                topic: "error_handling".to_string(),
                title: "错误处理".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
        },
    ]
}

/// Return sample `DocData` documents corresponding to `sample_chunks`.
pub fn sample_documents() -> Vec<DocData> {
    vec![
        DocData {
            text: "# 函数定义\n\n仓颉语言使用 `func` 关键字定义函数。函数可以有参数和返回值。\n\n```cangjie\nfunc add(a: Int, b: Int): Int {\n    return a + b\n}\n```\n\n函数是仓颉程序的基本构建块。".to_string(),
            metadata: DocMetadata {
                file_path: "syntax/functions.md".to_string(),
                category: "syntax".to_string(),
                topic: "functions".to_string(),
                title: "函数定义".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
            doc_id: "syntax/functions.md".to_string(),
        },
        DocData {
            text: "# 变量与常量\n\n使用 `let` 声明不可变变量，使用 `var` 声明可变变量。\n\n```cangjie\nlet x: Int = 10\nvar y: String = \"hello\"\n```\n\n仓颉的类型系统是静态强类型的。".to_string(),
            metadata: DocMetadata {
                file_path: "syntax/variables.md".to_string(),
                category: "syntax".to_string(),
                topic: "variables".to_string(),
                title: "变量与常量".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
            doc_id: "syntax/variables.md".to_string(),
        },
        DocData {
            text: "# 集合类型\n\n标准库提供了 Array、HashMap、HashSet 等集合类型。\n\n```cangjie\nlet arr = Array<Int>([1, 2, 3])\nlet map = HashMap<String, Int>()\n```\n\n集合类型支持迭代器操作。".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/collections.md".to_string(),
                category: "stdlib".to_string(),
                topic: "collections".to_string(),
                title: "集合类型".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
            doc_id: "stdlib/collections.md".to_string(),
        },
        DocData {
            text: "# 包管理器 CJPM\n\nCJPM 是仓颉的官方包管理工具。使用 `cjpm init` 创建新项目。\n\n```bash\ncjpm init my-project\ncjpm build\ncjpm test\n```\n\nCJPM 使用 cjpm.toml 文件管理依赖。".to_string(),
            metadata: DocMetadata {
                file_path: "cjpm/getting_started.md".to_string(),
                category: "cjpm".to_string(),
                topic: "getting_started".to_string(),
                title: "包管理器 CJPM".to_string(),
                code_block_count: 1,
                has_code: true,
                ..Default::default()
            },
            doc_id: "cjpm/getting_started.md".to_string(),
        },
    ]
}

/// Return a large document that spans multiple chunks when using small chunk sizes.
pub fn large_document() -> DocData {
    use std::fmt::Write;
    let mut sections = String::with_capacity(8192);
    write!(sections, "# 仓颉语言完整指南\n\n## 第一章：基础语法\n\n").unwrap();
    write!(
        sections,
        "仓颉（Cangjie）是一门面向现代应用开发的编程语言，支持多种编程范式。"
    )
    .unwrap();
    write!(sections, "它提供了强大的类型系统、模式匹配和协程支持。\n\n").unwrap();
    write!(sections, "```cangjie\nfunc main() {{\n    let message = \"Hello, Cangjie!\"\n    println(message)\n}}\n```\n\n").unwrap();
    for i in 2..=8 {
        write!(sections, "## 第{i}章：高级特性 Part {i}\n\n").unwrap();
        write!(
            sections,
            "本章介绍仓颉语言的第{i}个高级特性。包括泛型编程、错误处理、异步编程等重要概念。\n\n"
        )
        .unwrap();
        write!(sections, "```cangjie\n// 示例代码 {i}\nfunc example{i}() {{\n    let x = {i}\n    println(x)\n}}\n```\n\n").unwrap();
        for j in 0..5 {
            write!(
                sections,
                "这是第{i}章第{j}段的详细说明文字，用来确保文档长度足够产生多个分块。"
            )
            .unwrap();
            write!(sections, "仓颉语言提供了丰富的标准库和工具链支持。\n\n").unwrap();
        }
    }
    DocData {
        text: sections,
        metadata: DocMetadata {
            file_path: "syntax/complete_guide.md".to_string(),
            category: "syntax".to_string(),
            topic: "complete_guide".to_string(),
            title: "仓颉语言完整指南".to_string(),
            code_block_count: 8,
            has_code: true,
            ..Default::default()
        },
        doc_id: "syntax/complete_guide.md".to_string(),
    }
}

/// Return chunks that simulate cross-category duplicate topic names.
pub fn cross_category_chunks() -> Vec<TextChunk> {
    vec![
        TextChunk {
            text: "# 语法概述\n\n仓颉语法简洁而富有表现力，支持类型推导和模式匹配。".to_string(),
            metadata: DocMetadata {
                file_path: "syntax/overview.md".to_string(),
                category: "syntax".to_string(),
                topic: "overview".to_string(),
                title: "语法概述".to_string(),
                has_code: false,
                code_block_count: 0,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# 标准库概述\n\n标准库提供 IO、集合、网络等基础模块。\n\n```cangjie\nimport std.collection.*\n```".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/overview.md".to_string(),
                category: "stdlib".to_string(),
                topic: "overview".to_string(),
                title: "标准库概述".to_string(),
                has_code: true,
                code_block_count: 1,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# CJPM 概述\n\nCJPM 是仓颉的官方包管理器和构建工具。".to_string(),
            metadata: DocMetadata {
                file_path: "cjpm/overview.md".to_string(),
                category: "cjpm".to_string(),
                topic: "overview".to_string(),
                title: "CJPM 概述".to_string(),
                has_code: false,
                code_block_count: 0,
                ..Default::default()
            },
        },
    ]
}

/// Convert a slice of `TextChunk` into `Vec<DocData>`.
pub fn chunks_to_docs(chunks: &[TextChunk]) -> Vec<DocData> {
    chunks
        .iter()
        .map(|c| DocData {
            doc_id: c.metadata.file_path.clone(),
            text: c.text.clone(),
            metadata: c.metadata.clone(),
        })
        .collect()
}

/// Return documents matching cross_category_chunks for MockDocumentSource.
pub fn cross_category_documents() -> Vec<DocData> {
    chunks_to_docs(&cross_category_chunks())
}

/// Return chunks simulating stdlib package documentation with import statements.
pub fn stdlib_package_chunks() -> Vec<TextChunk> {
    vec![
        TextChunk {
            text: "# ArrayList\n\nimport std.collection\n\nArrayList 是一个动态数组实现。\n\n```cangjie\nlet list = ArrayList<Int>()\nlist.add(1)\nlist.add(2)\nprintln(list.size) // 2\n```".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/collection_arraylist.md".to_string(),
                category: "stdlib".to_string(),
                topic: "collection_arraylist".to_string(),
                title: "ArrayList".to_string(),
                has_code: true,
                code_block_count: 1,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# HashMap\n\nimport std.collection\n\nHashMap 是键值对存储容器。\n\n```cangjie\nlet map = HashMap<String, Int>()\nmap.put(\"key\", 42)\n```".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/collection_hashmap.md".to_string(),
                category: "stdlib".to_string(),
                topic: "collection_hashmap".to_string(),
                title: "HashMap".to_string(),
                has_code: true,
                code_block_count: 1,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# File IO\n\nimport std.fs\n\n文件读写操作。\n\n```cangjie\nlet content = File.readText(\"data.txt\")\nprintln(content)\n```".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/fs_file.md".to_string(),
                category: "stdlib".to_string(),
                topic: "fs_file".to_string(),
                title: "File IO".to_string(),
                has_code: true,
                code_block_count: 1,
                ..Default::default()
            },
        },
        TextChunk {
            text: "# HTTP Client\n\nimport std.net.http\n\nHTTP 客户端用于发送网络请求。\n\n```cangjie\nlet resp = HttpClient.get(\"https://example.com\")\nprintln(resp.body)\n```".to_string(),
            metadata: DocMetadata {
                file_path: "stdlib/net_http.md".to_string(),
                category: "stdlib".to_string(),
                topic: "net_http".to_string(),
                title: "HTTP Client".to_string(),
                has_code: true,
                code_block_count: 1,
                ..Default::default()
            },
        },
    ]
}

/// An in-memory `DocumentSource` implementation for testing.
pub struct MockDocumentSource {
    documents: HashMap<String, DocData>,
    categories: HashMap<String, Vec<String>>,
}

impl MockDocumentSource {
    /// Build a `MockDocumentSource` from a slice of `DocData`.
    pub fn from_docs(docs: &[DocData]) -> Self {
        let mut documents = HashMap::new();
        let mut categories: HashMap<String, Vec<String>> = HashMap::new();

        for doc in docs {
            let key = format!("{}:{}", doc.metadata.category, doc.metadata.topic);
            documents.insert(key, doc.clone());
            categories
                .entry(doc.metadata.category.clone())
                .or_default()
                .push(doc.metadata.topic.clone());
        }

        // Deduplicate and sort topics within each category
        for topics in categories.values_mut() {
            topics.sort();
            topics.dedup();
        }

        Self {
            documents,
            categories,
        }
    }
}

#[async_trait::async_trait]
impl DocumentSource for MockDocumentSource {
    async fn is_available(&self) -> bool {
        true
    }

    async fn get_categories(&self) -> anyhow::Result<Vec<String>> {
        let mut cats: Vec<String> = self.categories.keys().cloned().collect();
        cats.sort();
        Ok(cats)
    }

    async fn get_topics_in_category(&self, category: &str) -> anyhow::Result<Vec<String>> {
        Ok(self.categories.get(category).cloned().unwrap_or_default())
    }

    async fn get_document_by_topic(
        &self,
        topic: &str,
        category: Option<&str>,
    ) -> anyhow::Result<Option<DocData>> {
        if let Some(cat) = category {
            let key = format!("{cat}:{topic}");
            return Ok(self.documents.get(&key).cloned());
        }
        // Search across all categories
        for cat in self.categories.keys() {
            let key = format!("{cat}:{topic}");
            if let Some(doc) = self.documents.get(&key) {
                return Ok(Some(doc.clone()));
            }
        }
        Ok(None)
    }

    async fn load_all_documents(&self) -> anyhow::Result<Vec<DocData>> {
        Ok(self.documents.values().cloned().collect())
    }

    async fn get_all_topic_names(&self) -> anyhow::Result<Vec<String>> {
        let mut names: Vec<String> = self
            .categories
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    async fn get_topic_titles(&self, category: &str) -> anyhow::Result<HashMap<String, String>> {
        let topics = self.categories.get(category).cloned().unwrap_or_default();
        let mut titles = HashMap::new();
        for topic in &topics {
            let key = format!("{category}:{topic}");
            let title = self
                .documents
                .get(&key)
                .map(|d| d.metadata.title.clone())
                .unwrap_or_default();
            titles.insert(topic.clone(), title);
        }
        Ok(titles)
    }
}
