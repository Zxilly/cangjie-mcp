use std::collections::HashMap;
use std::path::PathBuf;

use cangjie_mcp::config::{
    DocLang, EmbeddingType, RerankType, Settings, DEFAULT_CHUNK_MAX_SIZE, DEFAULT_OPENAI_BASE_URL,
    DEFAULT_RERANK_INITIAL_K, DEFAULT_RERANK_TOP_K, DEFAULT_RRF_K,
};
use cangjie_mcp::indexer::document::source::DocumentSource;
use cangjie_mcp::indexer::{DocData, DocMetadata, TextChunk};

/// Create a BM25-only `Settings` suitable for testing.
pub fn test_settings(data_dir: PathBuf) -> Settings {
    Settings {
        docs_version: "test".to_string(),
        docs_lang: DocLang::Zh,
        embedding_type: EmbeddingType::None,
        local_model: String::new(),
        rerank_type: RerankType::None,
        rerank_model: String::new(),
        rerank_top_k: DEFAULT_RERANK_TOP_K,
        rerank_initial_k: DEFAULT_RERANK_INITIAL_K,
        rrf_k: DEFAULT_RRF_K,
        chunk_max_size: DEFAULT_CHUNK_MAX_SIZE,
        data_dir,
        server_url: None,
        openai_api_key: None,
        openai_base_url: DEFAULT_OPENAI_BASE_URL.to_string(),
        openai_model: String::new(),
        prebuilt: cangjie_mcp::config::PrebuiltMode::Off,
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
            },
            doc_id: "cjpm/getting_started.md".to_string(),
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

impl DocumentSource for MockDocumentSource {
    fn is_available(&self) -> bool {
        true
    }

    fn get_categories(&self) -> anyhow::Result<Vec<String>> {
        let mut cats: Vec<String> = self.categories.keys().cloned().collect();
        cats.sort();
        Ok(cats)
    }

    fn get_topics_in_category(&self, category: &str) -> anyhow::Result<Vec<String>> {
        Ok(self.categories.get(category).cloned().unwrap_or_default())
    }

    fn get_document_by_topic(
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

    fn load_all_documents(&self) -> anyhow::Result<Vec<DocData>> {
        Ok(self.documents.values().cloned().collect())
    }

    fn get_all_topic_names(&self) -> anyhow::Result<Vec<String>> {
        let mut names: Vec<String> = self
            .categories
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    fn get_topic_titles(&self, category: &str) -> anyhow::Result<HashMap<String, String>> {
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
