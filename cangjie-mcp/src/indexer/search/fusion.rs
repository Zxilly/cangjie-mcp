use std::collections::HashMap;

use crate::indexer::{SearchResult, SearchResultMetadata};

fn dedup_key(result: &SearchResult) -> String {
    let text_prefix: String = result.text.chars().take(200).collect();
    format!("{}|{}", result.metadata.file_path, text_prefix)
}

/// Merge multiple ranked result lists using Reciprocal Rank Fusion.
///
/// Each result gets a score of `1 / (k + rank)` for each list it appears in.
/// Results appearing in multiple lists accumulate higher scores.
pub fn reciprocal_rank_fusion(
    result_lists: &[Vec<SearchResult>],
    k: u32,
    top_k: usize,
) -> Vec<SearchResult> {
    if result_lists.is_empty() {
        return Vec::new();
    }

    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut best_result: HashMap<String, &SearchResult> = HashMap::new();

    for results in result_lists {
        for (rank, result) in results.iter().enumerate() {
            let key = dedup_key(result);
            let rrf_score = 1.0 / (k as f64 + rank as f64 + 1.0);
            *scores.entry(key.clone()).or_insert(0.0) += rrf_score;
            let entry = best_result.entry(key).or_insert(result);
            if result.score > entry.score {
                *entry = result;
            }
        }
    }

    let mut sorted_keys: Vec<String> = scores.keys().cloned().collect();
    sorted_keys.sort_by(|a, b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    sorted_keys
        .into_iter()
        .take(top_k)
        .filter_map(|key| {
            let original = best_result.get(&key)?;
            Some(SearchResult {
                text: original.text.clone(),
                score: scores[&key],
                metadata: SearchResultMetadata {
                    file_path: original.metadata.file_path.clone(),
                    category: original.metadata.category.clone(),
                    topic: original.metadata.topic.clone(),
                    title: original.metadata.title.clone(),
                    has_code: original.metadata.has_code,
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(text: &str, score: f64, file: &str) -> SearchResult {
        SearchResult {
            text: text.to_string(),
            score,
            metadata: SearchResultMetadata {
                file_path: file.to_string(),
                category: "test".to_string(),
                topic: "test".to_string(),
                title: "Test".to_string(),
                has_code: false,
            },
        }
    }

    #[test]
    fn test_rrf_empty() {
        let result = reciprocal_rank_fusion(&[], 60, 5);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rrf_single_list() {
        let list = vec![
            make_result("doc1", 0.9, "a.md"),
            make_result("doc2", 0.8, "b.md"),
        ];
        let result = reciprocal_rank_fusion(&[list], 60, 5);
        assert_eq!(result.len(), 2);
        assert!(result[0].score > result[1].score);
    }

    #[test]
    fn test_rrf_overlap_boosts_score() {
        let list1 = vec![
            make_result("shared doc", 0.9, "a.md"),
            make_result("only in list1", 0.8, "b.md"),
        ];
        let list2 = vec![
            make_result("shared doc", 0.7, "a.md"),
            make_result("only in list2", 0.6, "c.md"),
        ];
        let result = reciprocal_rank_fusion(&[list1, list2], 60, 5);
        // "shared doc" appears in both lists, so it should have the highest RRF score
        assert_eq!(result[0].text, "shared doc");
    }

    #[test]
    fn test_rrf_respects_top_k() {
        let list = vec![
            make_result("doc1", 0.9, "a.md"),
            make_result("doc2", 0.8, "b.md"),
            make_result("doc3", 0.7, "c.md"),
        ];
        let result = reciprocal_rank_fusion(&[list], 60, 2);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_rrf_deduplicates() {
        let list1 = vec![make_result("same text", 0.9, "a.md")];
        let list2 = vec![make_result("same text", 0.8, "a.md")];
        let result = reciprocal_rank_fusion(&[list1, list2], 60, 5);
        assert_eq!(result.len(), 1);
    }
}
