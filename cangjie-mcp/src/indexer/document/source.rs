use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::{info, warn};

use crate::config::DocLang;
use crate::indexer::document::loader::{extract_title_from_content, load_document_from_content};
use crate::indexer::DocData;

// -- Document Source trait ---------------------------------------------------

pub trait DocumentSource: Send + Sync {
    fn is_available(&self) -> bool;
    fn get_categories(&self) -> Result<Vec<String>>;
    fn get_topics_in_category(&self, category: &str) -> Result<Vec<String>>;
    fn get_document_by_topic(&self, topic: &str, category: Option<&str>)
        -> Result<Option<DocData>>;
    fn load_all_documents(&self) -> Result<Vec<DocData>>;
    fn get_all_topic_names(&self) -> Result<Vec<String>>;
    fn get_topic_titles(&self, category: &str) -> Result<HashMap<String, String>>;
}

// -- Git Document Source -----------------------------------------------------

/// Reads documentation from a local git clone using git2.
///
/// This struct does NOT hold a git2::Repository (which is not Sync).
/// Instead it stores paths and reopens the repo as needed.
pub struct GitDocumentSource {
    repo_dir: PathBuf,
    docs_base_path: String,
    topic_index: std::sync::OnceLock<HashMap<String, String>>,
}

impl GitDocumentSource {
    pub fn new(repo_dir: PathBuf, lang: DocLang) -> Result<Self> {
        let docs_base_path = format!("docs/dev-guide/{}", lang.source_dir_name());

        Ok(Self {
            repo_dir,
            docs_base_path,
            topic_index: std::sync::OnceLock::new(),
        })
    }

    /// Open the repository for short-lived operations.
    fn open_repo(&self) -> Result<git2::Repository> {
        git2::Repository::open(&self.repo_dir).context("Failed to open git repository")
    }

    /// Read a file from the HEAD tree.
    fn read_file(&self, path: &str) -> Result<String> {
        let repo = self.open_repo()?;
        let head = repo.head()?;
        let tree = head.peel_to_tree()?;
        let entry = tree
            .get_path(Path::new(path))
            .with_context(|| format!("Path not found: {path}"))?;
        let blob = repo.find_blob(entry.id())?;
        Ok(std::str::from_utf8(blob.content())?.to_string())
    }

    /// List subdirectories under a path in the HEAD tree.
    fn list_dirs(&self, path: &str) -> Result<Vec<String>> {
        let repo = self.open_repo()?;
        let head = repo.head()?;
        let tree = head.peel_to_tree()?;
        let entry = tree.get_path(Path::new(path))?;
        let subtree = repo.find_tree(entry.id())?;
        let mut dirs = Vec::new();
        for item in subtree.iter() {
            if item.kind() == Some(git2::ObjectType::Tree) {
                if let Some(name) = item.name() {
                    if !name.starts_with('.') && !name.starts_with('_') {
                        dirs.push(name.to_string());
                    }
                }
            }
        }
        dirs.sort();
        Ok(dirs)
    }

    /// List .md files recursively under a path. Returns relative paths from base.
    fn list_md_files(&self, base_path: &str) -> Result<Vec<String>> {
        let repo = self.open_repo()?;
        let head = repo.head()?;
        let root_tree = head.peel_to_tree()?;
        let entry = root_tree.get_path(Path::new(base_path))?;
        let subtree = repo.find_tree(entry.id())?;
        let mut files = Vec::new();
        Self::collect_md_files_recursive(&repo, &subtree, "", &mut files)?;
        files.sort();
        Ok(files)
    }

    fn collect_md_files_recursive(
        repo: &git2::Repository,
        tree: &git2::Tree,
        prefix: &str,
        files: &mut Vec<String>,
    ) -> Result<()> {
        for item in tree.iter() {
            let name = item.name().unwrap_or("");
            let path = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}/{name}")
            };
            match item.kind() {
                Some(git2::ObjectType::Blob) if name.ends_with(".md") => {
                    files.push(path);
                }
                Some(git2::ObjectType::Tree) => {
                    let subtree = repo.find_tree(item.id())?;
                    Self::collect_md_files_recursive(repo, &subtree, &path, files)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn build_topic_index(&self) -> Result<HashMap<String, String>> {
        let categories = self.get_categories()?;
        let mut mapping = HashMap::new();
        for cat in &categories {
            if let Ok(topics) = self.get_topics_in_category(cat) {
                for topic in topics {
                    mapping.entry(topic).or_insert_with(|| cat.clone());
                }
            }
        }
        info!(
            "Topic index built: {} topics across categories",
            mapping.len()
        );
        Ok(mapping)
    }

    fn get_cached_topic_index(&self) -> &HashMap<String, String> {
        self.topic_index
            .get_or_init(|| self.build_topic_index().unwrap_or_default())
    }
}

impl DocumentSource for GitDocumentSource {
    fn is_available(&self) -> bool {
        self.repo_dir.exists() && self.repo_dir.join(".git").exists()
    }

    fn get_categories(&self) -> Result<Vec<String>> {
        self.list_dirs(&self.docs_base_path)
    }

    fn get_topics_in_category(&self, category: &str) -> Result<Vec<String>> {
        let path = format!("{}/{category}", self.docs_base_path);
        let files = self.list_md_files(&path)?;
        let mut topics: Vec<String> = files
            .iter()
            .filter_map(|f| {
                f.rsplit('/')
                    .next()
                    .and_then(|name| name.strip_suffix(".md"))
                    .map(|s| s.to_string())
            })
            .collect();
        topics.sort();
        Ok(topics)
    }

    fn get_document_by_topic(
        &self,
        topic: &str,
        category: Option<&str>,
    ) -> Result<Option<DocData>> {
        let category = match category {
            Some(c) => c.to_string(),
            None => match self.get_cached_topic_index().get(topic) {
                Some(c) => c.clone(),
                None => return Ok(None),
            },
        };

        let filename = format!("{topic}.md");
        let path = format!("{}/{category}", self.docs_base_path);

        let files = self.list_md_files(&path)?;
        for file in &files {
            let file_name = file.rsplit('/').next().unwrap_or("");
            if file_name == filename {
                let full_path = format!("{path}/{file}");
                match self.read_file(&full_path) {
                    Ok(content) => {
                        let relative_path = format!("{category}/{file}");
                        return Ok(load_document_from_content(
                            content,
                            &relative_path,
                            &category,
                            topic,
                        ));
                    }
                    Err(e) => {
                        warn!("Failed to read {}: {}", full_path, e);
                    }
                }
            }
        }

        Ok(None)
    }

    fn load_all_documents(&self) -> Result<Vec<DocData>> {
        let categories = self.get_categories()?;
        let mut documents = Vec::new();

        for category in &categories {
            let path = format!("{}/{category}", self.docs_base_path);
            let files = self.list_md_files(&path)?;

            for file in &files {
                let full_path = format!("{path}/{file}");
                match self.read_file(&full_path) {
                    Ok(content) => {
                        let topic = file
                            .rsplit('/')
                            .next()
                            .and_then(|n| n.strip_suffix(".md"))
                            .unwrap_or("")
                            .to_string();
                        let relative_path = format!("{category}/{file}");
                        if let Some(doc) =
                            load_document_from_content(content, &relative_path, category, &topic)
                        {
                            documents.push(doc);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to load {}: {}", full_path, e);
                    }
                }
            }
        }

        info!("Loaded {} documents from git.", documents.len());
        Ok(documents)
    }

    fn get_all_topic_names(&self) -> Result<Vec<String>> {
        let index = self.get_cached_topic_index();
        let mut names: Vec<String> = index.keys().cloned().collect();
        names.sort();
        Ok(names)
    }

    fn get_topic_titles(&self, category: &str) -> Result<HashMap<String, String>> {
        let path = format!("{}/{category}", self.docs_base_path);
        let files = self.list_md_files(&path)?;
        let mut titles = HashMap::new();

        for file in &files {
            let topic = file
                .rsplit('/')
                .next()
                .and_then(|n| n.strip_suffix(".md"))
                .unwrap_or("")
                .to_string();
            let full_path = format!("{path}/{file}");
            match self.read_file(&full_path) {
                Ok(content) => {
                    let title = extract_title_from_content(&content);
                    titles.insert(topic, title);
                }
                Err(_) => {
                    titles.insert(topic, String::new());
                }
            }
        }

        Ok(titles)
    }
}

// -- Remote protocol types --------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
struct TopicEntry {
    name: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteTopicsResponse {
    categories: HashMap<String, Vec<TopicEntry>>,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteTopicDetailResponse {
    #[serde(default)]
    content: String,
    #[serde(default)]
    file_path: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    topic: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, serde::Deserialize)]
struct RemoteHealthResponse {
    #[serde(default)]
    status: String,
}

// -- Remote Document Source --------------------------------------------------

pub struct RemoteDocumentSource {
    server_url: String,
    cache: std::sync::OnceLock<HashMap<String, Vec<TopicEntry>>>,
}

impl RemoteDocumentSource {
    pub fn new(server_url: &str) -> Self {
        Self {
            server_url: server_url.trim_end_matches('/').to_string(),
            cache: std::sync::OnceLock::new(),
        }
    }

    fn fetch_topics_blocking(&self) -> Result<HashMap<String, Vec<TopicEntry>>> {
        let url = format!("{}/topics", self.server_url);
        let resp = reqwest::blocking::get(&url)?;
        let data: RemoteTopicsResponse = resp.json().context("Invalid /topics response")?;
        Ok(data.categories)
    }

    fn get_cached_topics(&self) -> &HashMap<String, Vec<TopicEntry>> {
        self.cache
            .get_or_init(|| self.fetch_topics_blocking().unwrap_or_default())
    }
}

impl DocumentSource for RemoteDocumentSource {
    fn is_available(&self) -> bool {
        let url = format!("{}/health", self.server_url);
        reqwest::blocking::get(&url)
            .and_then(|r| r.json::<RemoteHealthResponse>())
            .map(|h| h.status == "ok")
            .unwrap_or(false)
    }

    fn get_categories(&self) -> Result<Vec<String>> {
        let topics = self.get_cached_topics();
        let mut cats: Vec<String> = topics.keys().cloned().collect();
        cats.sort();
        Ok(cats)
    }

    fn get_topics_in_category(&self, category: &str) -> Result<Vec<String>> {
        let topics = self.get_cached_topics();
        let entries = topics.get(category).cloned().unwrap_or_default();
        let mut names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
        names.sort();
        Ok(names)
    }

    fn get_document_by_topic(
        &self,
        topic: &str,
        category: Option<&str>,
    ) -> Result<Option<DocData>> {
        let category = match category {
            Some(c) => c.to_string(),
            None => {
                let topics = self.get_cached_topics();
                let mut found = None;
                for (cat, entries) in topics {
                    if entries.iter().any(|e| e.name == topic) {
                        found = Some(cat.clone());
                        break;
                    }
                }
                match found {
                    Some(c) => c,
                    None => return Ok(None),
                }
            }
        };

        let url = format!("{}/topics/{}/{}", self.server_url, category, topic);
        let resp = reqwest::blocking::get(&url)?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let detail: RemoteTopicDetailResponse =
            resp.json().context("Invalid topic detail response")?;

        let file_path = if detail.file_path.is_empty() {
            format!("{category}/{topic}")
        } else {
            detail.file_path
        };

        Ok(Some(DocData {
            doc_id: file_path.clone(),
            text: detail.content,
            metadata: crate::indexer::DocMetadata {
                file_path,
                category: if detail.category.is_empty() {
                    category
                } else {
                    detail.category
                },
                topic: if detail.topic.is_empty() {
                    topic.to_string()
                } else {
                    detail.topic
                },
                title: detail.title,
                ..Default::default()
            },
        }))
    }

    fn load_all_documents(&self) -> Result<Vec<DocData>> {
        anyhow::bail!("RemoteDocumentSource does not support load_all_documents")
    }

    fn get_all_topic_names(&self) -> Result<Vec<String>> {
        let topics = self.get_cached_topics();
        let mut names: Vec<String> = topics
            .values()
            .flat_map(|entries| entries.iter().map(|e| e.name.clone()))
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    fn get_topic_titles(&self, category: &str) -> Result<HashMap<String, String>> {
        let topics = self.get_cached_topics();
        let entries = topics.get(category).cloned().unwrap_or_default();
        Ok(entries.into_iter().map(|e| (e.name, e.title)).collect())
    }
}
