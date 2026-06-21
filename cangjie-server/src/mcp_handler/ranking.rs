use std::collections::{HashMap, HashSet};

use cangjie_indexer::SearchResult;

use super::CangjieServer;

impl CangjieServer {
    pub(super) fn has_package(result: &SearchResult, package: &str) -> bool {
        result.text.contains(package) || result.text.contains(&format!("import {package}"))
    }

    fn query_terms(query: &str) -> Vec<String> {
        let jieba = &**cangjie_indexer::search::GLOBAL_JIEBA;
        let lower = query.to_lowercase();
        let mut terms: Vec<String> = jieba
            .cut_for_search(&lower, true)
            .into_iter()
            .map(|t| t.word.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        for token in lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| !t.is_empty())
        {
            if !terms.iter().any(|t| t == token) {
                terms.push(token.to_string());
            }
        }

        terms
    }

    fn lexical_boost(query_terms: &[String], query_lc: &str, item: &SearchResult) -> f64 {
        let topic = item.metadata.topic.to_lowercase();
        let title = item.metadata.title.to_lowercase();
        let path = item.metadata.file_path.to_lowercase();
        let text = item.text.to_lowercase();
        let mut boost = 0.0;

        for term in query_terms {
            if topic == *term {
                boost += 8.0;
            } else if topic.contains(term) {
                boost += 5.0;
            }

            if title == *term {
                boost += 6.0;
            } else if title.contains(term) {
                boost += 4.0;
            }

            if path.contains(term) {
                boost += 2.0;
            }

            if text.contains(term) {
                boost += 1.5;
            }
        }

        if !query_lc.is_empty() {
            if topic.contains(query_lc) {
                boost += 6.0;
            }
            if title.contains(query_lc) {
                boost += 5.0;
            }
            if text.contains(query_lc) {
                boost += 2.0;
            }
        }

        boost
    }

    pub(super) fn rerank_and_dedup_results(
        results: Vec<SearchResult>,
        query: &str,
        top_k: usize,
        offset: usize,
    ) -> Vec<SearchResult> {
        /// Maximum possible boost per query term (topic exact 8 + title exact 6 + text 1.5)
        const MAX_BOOST_PER_TERM: f64 = 15.5;
        /// Maximum whole-query boost (topic 6 + title 5 + text 2)
        const MAX_WHOLE_QUERY_BOOST: f64 = 13.0;
        /// Weight cap for lexical boost in final score
        const BOOST_WEIGHT: f64 = 0.3;

        let query_terms = Self::query_terms(query);
        let query_lc = query.to_lowercase();
        let max_possible = query_terms.len() as f64 * MAX_BOOST_PER_TERM + MAX_WHOLE_QUERY_BOOST;
        let mut scored: Vec<(SearchResult, f64)> = results
            .into_iter()
            .map(|r| {
                let raw_boost = Self::lexical_boost(&query_terms, &query_lc, &r);
                let normalized_boost = if max_possible > 0.0 {
                    raw_boost / max_possible
                } else {
                    0.0
                };
                let adjusted = r.score + BOOST_WEIGHT * normalized_boost;
                (r, adjusted)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let per_doc_limit = if top_k <= 3 { 1 } else { 2 };
        let limit = offset + top_k + 1;

        // Suppress near-identical snippets.
        let mut seen_text_keys: HashSet<String> = HashSet::new();
        let mut candidates: Vec<(SearchResult, f64)> = Vec::new();
        for (result, adjusted) in scored {
            let text_key = result
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            if !seen_text_keys.insert(text_key) {
                continue;
            }
            candidates.push((result, adjusted));
        }

        // Phase 1: maximize document coverage (at most one per document).
        let mut selected: Vec<(SearchResult, f64)> = Vec::new();
        let mut per_doc_count: HashMap<String, usize> = HashMap::new();
        for (result, adjusted) in &candidates {
            if selected.len() >= limit {
                break;
            }
            let key = result.metadata.file_path.clone();
            if per_doc_count.get(&key).copied().unwrap_or(0) == 0 {
                selected.push((result.clone(), *adjusted));
                per_doc_count.insert(key, 1);
            }
        }

        // Phase 2: backfill with additional high-scoring snippets up to per-doc cap.
        for (result, adjusted) in candidates {
            if selected.len() >= limit {
                break;
            }
            let key = result.metadata.file_path.clone();
            let count = per_doc_count.get(&key).copied().unwrap_or(0);
            if count >= per_doc_limit {
                continue;
            }
            if count == 0 {
                continue;
            }
            selected.push((result, adjusted));
            per_doc_count.insert(key, count + 1);
        }

        selected.into_iter().map(|(result, _)| result).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cangjie_indexer::{SearchResult, SearchResultMetadata};

    #[test]
    fn test_has_package_direct_match() {
        let result = SearchResult {
            text: "This text mentions std.collection directly".to_string(),
            score: 1.0,
            metadata: SearchResultMetadata::default(),
        };
        assert!(CangjieServer::has_package(&result, "std.collection"));
    }

    #[test]
    fn test_has_package_import_match() {
        let result = SearchResult {
            text: "You can use import std.fs to access filesystem APIs".to_string(),
            score: 1.0,
            metadata: SearchResultMetadata::default(),
        };
        assert!(CangjieServer::has_package(&result, "std.fs"));
    }

    #[test]
    fn test_has_package_no_match() {
        let result = SearchResult {
            text: "This text has nothing relevant to any package".to_string(),
            score: 1.0,
            metadata: SearchResultMetadata::default(),
        };
        assert!(!CangjieServer::has_package(&result, "std.collection"));
    }
}
