use std::collections::HashMap;

use crate::db::Database;

#[derive(Debug, Clone)]
pub struct Cluster {
    pub id: String,
    pub test_name: String,
    pub error_category: String,
    pub error_pattern: String,
    pub pdf_count: usize,
    pub example_pdfs: Vec<ExamplePdf>,
    pub first_seen: String,
    pub last_seen: String,
    pub trend: Trend,
    pub severity: Severity,
    pub priority_score: f64,
    pub github_issue: Option<u64>,
    pub status: ClusterStatus,
}

#[derive(Debug, Clone)]
pub struct ExamplePdf {
    pub path: String,
    pub size: i64,
    pub error_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trend {
    New,
    Growing,
    Stable,
    Shrinking,
    Resolved,
}

impl std::fmt::Display for Trend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::New => write!(f, "new"),
            Self::Growing => write!(f, "growing"),
            Self::Stable => write!(f, "stable"),
            Self::Shrinking => write!(f, "shrinking"),
            Self::Resolved => write!(f, "resolved"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

impl Severity {
    pub fn weight(&self) -> f64 {
        match self {
            Self::Critical => 10.0,
            Self::High => 5.0,
            Self::Medium => 3.0,
            Self::Low => 1.0,
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "critical"),
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ClusterStatus {
    Open,
    Fixed,
    WontFix,
}

impl std::fmt::Display for ClusterStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "open"),
            Self::Fixed => write!(f, "fixed"),
            Self::WontFix => write!(f, "wontfix"),
        }
    }
}

fn severity_from_status_and_category(status: &str, category: &str) -> Severity {
    match status {
        "crash" => Severity::Critical,
        "timeout" => Severity::High,
        _ => match category {
            "panic" | "out_of_memory" => Severity::Critical,
            "timeout" => Severity::High,
            "invalid_xref" | "corrupt_stream" | "malformed_object" | "invalid_header" => {
                Severity::Medium
            }
            _ => Severity::Low,
        },
    }
}

fn extract_pattern(error_msg: &str) -> String {
    // Normalize variable parts of error messages into patterns.
    // Replace specific numbers, paths, offsets with placeholders.
    let mut pattern = error_msg.to_string();

    // Truncate very long messages
    if pattern.len() > 120 {
        pattern.truncate(120);
        pattern.push_str("...");
    }

    // Replace object IDs first: "42 0 R" -> "{obj} 0 R" (before offset regex eats the number)
    let re_objid = regex_lite::Regex::new(r"\d+ 0 R").unwrap();
    pattern = re_objid.replace_all(&pattern, "{obj} 0 R").to_string();

    // Replace numeric offsets/indices: "at offset 12345" -> "at offset {N}"
    let re_offset =
        regex_lite::Regex::new(r"(?i)(offset|position|index|page|object)\s+\d+").unwrap();
    pattern = re_offset.replace_all(&pattern, "$1 {N}").to_string();

    // Replace hex values: "0x1A2B" -> "{hex}"
    let re_hex = regex_lite::Regex::new(r"0x[0-9a-fA-F]+").unwrap();
    pattern = re_hex.replace_all(&pattern, "{hex}").to_string();

    // Replace file paths
    let re_path = regex_lite::Regex::new(r"/[\w/.-]+\.pdf").unwrap();
    pattern = re_path.replace_all(&pattern, "{path}").to_string();

    pattern
}

fn cluster_id(test_name: &str, category: &str, pattern: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    test_name.hash(&mut hasher);
    category.hash(&mut hasher);
    pattern.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn likely_crate(test_name: &str) -> &str {
    match test_name {
        "parse" | "metadata" | "geometry" => "pdf-syntax",
        "render" | "render_oracle" | "bookmarks" => "pdf-engine",
        "text_extract" | "text_oracle" | "images" | "search" => "pdf-extract",
        "form_fields" => "pdf-forms",
        "annotations" => "pdf-annot",
        "signatures" => "pdf-sign",
        "compliance" | "metadata_oracle" => "pdf-compliance",
        _ => "unknown",
    }
}

struct FailureRow {
    test_name: String,
    status: String,
    error_category: String,
    error_message: String,
    pdf_path: String,
    pdf_size: i64,
}

pub fn compute_clusters(db: &Database, run_id: &str) -> Vec<Cluster> {
    let failures = query_failures(db, run_id);

    // Group by (test_name, error_category, extracted_pattern)
    let mut groups: HashMap<(String, String, String), Vec<FailureRow>> = HashMap::new();
    for f in failures {
        let pattern = extract_pattern(&f.error_message);
        groups
            .entry((f.test_name.clone(), f.error_category.clone(), pattern))
            .or_default()
            .push(f);
    }

    let mut clusters: Vec<Cluster> = groups
        .into_iter()
        .map(|((test_name, error_category, pattern), rows)| {
            let severity = severity_from_status_and_category(
                rows.first().map(|r| r.status.as_str()).unwrap_or("fail"),
                &error_category,
            );
            let pdf_count = rows.len();
            let priority_score = pdf_count as f64 * severity.weight();

            // Pick smallest PDFs as examples (most reproducible)
            let mut examples: Vec<ExamplePdf> = rows
                .iter()
                .map(|r| ExamplePdf {
                    path: r.pdf_path.clone(),
                    size: r.pdf_size,
                    error_message: r.error_message.clone(),
                })
                .collect();
            examples.sort_by_key(|e| e.size);
            examples.truncate(5);

            let id = cluster_id(&test_name, &error_category, &pattern);

            Cluster {
                id,
                test_name,
                error_category,
                error_pattern: pattern,
                pdf_count,
                example_pdfs: examples,
                first_seen: run_id.to_string(),
                last_seen: run_id.to_string(),
                trend: Trend::New,
                severity,
                priority_score,
                github_issue: None,
                status: ClusterStatus::Open,
            }
        })
        .collect();

    // Sort by priority (highest first)
    clusters.sort_by(|a, b| {
        b.priority_score
            .partial_cmp(&a.priority_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute trends from stored cluster data
    compute_trends(db, &mut clusters, run_id);

    // Persist clusters to error_clusters table
    persist_clusters(db, &clusters, run_id);

    clusters
}

fn query_failures(db: &Database, run_id: &str) -> Vec<FailureRow> {
    db.query_failures(run_id)
        .into_iter()
        .map(|row| FailureRow {
            test_name: row.0,
            status: row.1,
            error_category: row.2,
            error_message: row.3,
            pdf_path: row.4,
            pdf_size: row.5,
        })
        .collect()
}

fn compute_trends(db: &Database, clusters: &mut [Cluster], run_id: &str) {
    let stored = db.load_stored_clusters();
    let stored_map: HashMap<&str, (i64, &str, Option<u64>)> = stored
        .iter()
        .map(|s| {
            (
                s.cluster_id.as_str(),
                (s.pdf_count, s.status.as_str(), s.github_issue_number),
            )
        })
        .collect();

    for cluster in clusters.iter_mut() {
        if let Some(&(prev_count, status, issue_num)) = stored_map.get(cluster.id.as_str()) {
            cluster.github_issue = issue_num;
            if status == "wontfix" {
                cluster.status = ClusterStatus::WontFix;
            }
            // Update first_seen from stored data
            if let Some(stored_row) = stored.iter().find(|s| s.cluster_id == cluster.id) {
                if let Some(first) = &stored_row.first_seen_run {
                    cluster.first_seen = first.clone();
                }
            }
            cluster.last_seen = run_id.to_string();

            let prev = prev_count as usize;
            cluster.trend = if cluster.pdf_count > prev {
                Trend::Growing
            } else if cluster.pdf_count < prev {
                Trend::Shrinking
            } else {
                Trend::Stable
            };
        }
        // else: stays as Trend::New
    }
}

fn persist_clusters(db: &Database, clusters: &[Cluster], run_id: &str) {
    for cluster in clusters {
        let _ = db.upsert_cluster(
            &cluster.id,
            &cluster.test_name,
            &cluster.error_category,
            &cluster.error_pattern,
            cluster.pdf_count as i64,
            &cluster.first_seen,
            run_id,
            cluster.github_issue,
            &cluster.status.to_string(),
        );
    }
}

pub fn format_cluster_table(clusters: &[Cluster]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<18} {:<22} {:>6} {:>8.0} {:<10} Pattern\n",
        "Test", "Category", "Count", "Priority", "Trend"
    ));
    out.push_str(&"-".repeat(100));
    out.push('\n');
    for c in clusters {
        let pattern_short = truncate_utf8(&c.error_pattern, 35);
        out.push_str(&format!(
            "{:<18} {:<22} {:>6} {:>8.0} {:<10} {}\n",
            c.test_name, c.error_category, c.pdf_count, c.priority_score, c.trend, pattern_short,
        ));
    }
    out
}

pub fn likely_crate_for_test(test_name: &str) -> &str {
    likely_crate(test_name)
}

fn truncate_utf8(s: &str, max_chars: usize) -> String {
    let truncated: String = s.chars().take(max_chars).collect();
    if truncated.len() < s.len() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_pattern_replaces_offsets() {
        let msg = "Invalid xref at offset 12345";
        let pat = extract_pattern(msg);
        assert!(pat.contains("{N}"));
        assert!(!pat.contains("12345"));
    }

    #[test]
    fn extract_pattern_replaces_object_ids() {
        let msg = "Referenced object 42 0 R not found";
        let pat = extract_pattern(msg);
        assert!(pat.contains("{obj} 0 R"));
    }

    #[test]
    fn extract_pattern_replaces_paths() {
        let msg = "Failed to parse /opt/corpus/test.pdf";
        let pat = extract_pattern(msg);
        assert!(pat.contains("{path}"));
    }

    #[test]
    fn extract_pattern_truncates_long_messages() {
        let msg = "a".repeat(200);
        let pat = extract_pattern(&msg);
        assert!(pat.len() <= 130);
        assert!(pat.ends_with("..."));
    }

    #[test]
    fn severity_weights() {
        assert_eq!(
            severity_from_status_and_category("crash", "panic").weight(),
            10.0
        );
        assert_eq!(
            severity_from_status_and_category("timeout", "unknown").weight(),
            5.0
        );
        assert_eq!(
            severity_from_status_and_category("fail", "invalid_xref").weight(),
            3.0
        );
        assert_eq!(
            severity_from_status_and_category("fail", "unknown").weight(),
            1.0
        );
    }

    #[test]
    fn cluster_id_deterministic() {
        let a = cluster_id("render", "panic", "index out of bounds");
        let b = cluster_id("render", "panic", "index out of bounds");
        assert_eq!(a, b);
    }

    #[test]
    fn cluster_id_differs_for_different_inputs() {
        let a = cluster_id("render", "panic", "index out of bounds");
        let b = cluster_id("parse", "panic", "index out of bounds");
        assert_ne!(a, b);
    }
}
