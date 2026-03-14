use std::collections::HashMap;

use crate::{SearchResult, SearchResultMetadata};

fn dedup_key(result: &SearchResult) -> String {
    result.metadata.chunk_id.clone()
}

/// Limit results so that no single file contributes more than `max_per_file` results.
/// Results are processed in order; once a file reaches the limit, further results from
/// that file are dropped while preserving the relative order of kept results.
pub fn enforce_diversity(results: Vec<SearchResult>, max_per_file: usize) -> Vec<SearchResult> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        let count = counts.entry(r.metadata.file_path.clone()).or_insert(0);
        *count += 1;
        if *count <= max_per_file {
            out.push(r);
        }
    }
    out
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
                    chunk_id: original.metadata.chunk_id.clone(),
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(text: &str, score: f64, file: &str, chunk_id: &str) -> SearchResult {
        SearchResult {
            text: text.to_string(),
            score,
            metadata: SearchResultMetadata {
                file_path: file.to_string(),
                category: "test".to_string(),
                topic: "test".to_string(),
                title: "Test".to_string(),
                has_code: false,
                chunk_id: chunk_id.to_string(),
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
            make_result("doc1", 0.9, "a.md", "a.md#0"),
            make_result("doc2", 0.8, "b.md", "b.md#0"),
        ];
        let result = reciprocal_rank_fusion(&[list], 60, 5);
        assert_eq!(result.len(), 2);
        assert!(result[0].score > result[1].score);
    }

    #[test]
    fn test_rrf_overlap_boosts_score() {
        let list1 = vec![
            make_result("shared doc", 0.9, "a.md", "a.md#0"),
            make_result("only in list1", 0.8, "b.md", "b.md#0"),
        ];
        let list2 = vec![
            make_result("shared doc", 0.7, "a.md", "a.md#0"),
            make_result("only in list2", 0.6, "c.md", "c.md#0"),
        ];
        let result = reciprocal_rank_fusion(&[list1, list2], 60, 5);
        // "shared doc" appears in both lists, so it should have the highest RRF score
        assert_eq!(result[0].text, "shared doc");
    }

    #[test]
    fn test_rrf_respects_top_k() {
        let list = vec![
            make_result("doc1", 0.9, "a.md", "a.md#0"),
            make_result("doc2", 0.8, "b.md", "b.md#0"),
            make_result("doc3", 0.7, "c.md", "c.md#0"),
        ];
        let result = reciprocal_rank_fusion(&[list], 60, 2);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_rrf_deduplicates_by_chunk_id() {
        let list1 = vec![make_result("text variant A", 0.9, "a.md", "a.md#0")];
        let list2 = vec![make_result("text variant B", 0.8, "a.md", "a.md#0")];
        let result = reciprocal_rank_fusion(&[list1, list2], 60, 5);
        assert_eq!(result.len(), 1, "Same chunk_id should dedup to one result");
    }

    #[test]
    fn test_rrf_different_chunk_ids_not_deduped() {
        let list1 = vec![make_result("text A", 0.9, "a.md", "a.md#0")];
        let list2 = vec![make_result("text B", 0.8, "a.md", "a.md#1")];
        let result = reciprocal_rank_fusion(&[list1, list2], 60, 5);
        assert_eq!(result.len(), 2, "Different chunk_ids should not be deduped");
    }

    #[test]
    fn test_enforce_diversity_limits_per_file() {
        let results = vec![
            make_result("r1", 0.9, "a.md", "a.md#0"),
            make_result("r2", 0.8, "a.md", "a.md#1"),
            make_result("r3", 0.7, "a.md", "a.md#2"),
            make_result("r4", 0.6, "b.md", "b.md#0"),
        ];
        let diverse = enforce_diversity(results, 2);
        assert_eq!(diverse.len(), 3);
        let a_count = diverse
            .iter()
            .filter(|r| r.metadata.file_path == "a.md")
            .count();
        assert_eq!(a_count, 2, "At most 2 results per file");
    }

    #[test]
    fn test_enforce_diversity_preserves_order() {
        let results = vec![
            make_result("r1", 0.9, "a.md", "a.md#0"),
            make_result("r2", 0.8, "b.md", "b.md#0"),
            make_result("r3", 0.7, "a.md", "a.md#1"),
            make_result("r4", 0.6, "a.md", "a.md#2"),
            make_result("r5", 0.5, "b.md", "b.md#1"),
        ];
        let diverse = enforce_diversity(results, 2);
        assert_eq!(diverse.len(), 4);
        assert_eq!(diverse[0].text, "r1");
        assert_eq!(diverse[1].text, "r2");
        assert_eq!(diverse[2].text, "r3");
        assert_eq!(diverse[3].text, "r5");
    }

    #[test]
    fn test_enforce_diversity_empty() {
        let diverse = enforce_diversity(Vec::new(), 2);
        assert!(diverse.is_empty());
    }

    #[test]
    fn test_enforce_diversity_max_one() {
        let results = vec![
            make_result("r1", 0.9, "a.md", "a.md#0"),
            make_result("r2", 0.8, "a.md", "a.md#1"),
            make_result("r3", 0.7, "b.md", "b.md#0"),
        ];
        let diverse = enforce_diversity(results, 1);
        assert_eq!(diverse.len(), 2);
        assert_eq!(diverse[0].text, "r1");
        assert_eq!(diverse[1].text, "r3");
    }
}
