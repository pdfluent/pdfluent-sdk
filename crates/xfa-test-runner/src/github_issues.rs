use crate::clustering::{likely_crate_for_test, Cluster, Trend};

fn truncate_utf8(s: &str, max_chars: usize) -> String {
    let truncated: String = s.chars().take(max_chars).collect();
    if truncated.len() < s.len() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

pub fn generate_issue_body(cluster: &Cluster, run_id: &str) -> String {
    let example_table = cluster
        .example_pdfs
        .iter()
        .enumerate()
        .map(|(i, ex)| {
            let err_short = truncate_utf8(&ex.error_message, 60);
            format!(
                "| {} | `{}` | {} KB | {} |",
                i + 1,
                ex.path,
                ex.size / 1024,
                err_short,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let crate_name = likely_crate_for_test(&cluster.test_name);

    let volume_label = if cluster.pdf_count >= 100 {
        "high"
    } else if cluster.pdf_count >= 10 {
        "medium"
    } else {
        "low"
    };

    let trend_icon = match &cluster.trend {
        Trend::New => "NEW",
        Trend::Growing => "GROWING",
        Trend::Stable => "stable",
        Trend::Shrinking => "shrinking",
        Trend::Resolved => "RESOLVED",
    };

    format!(
        r#"## Cluster: {test_name} — {error_pattern}

**Category:** `{error_category}`
**PDFs affected:** {pdf_count}
**Priority:** {priority:.0}
**Severity:** {severity}
**First seen:** Run `{first_seen}`
**Last seen:** Run `{run_id}`
**Trend:** {trend_icon}

### Error Pattern
```
{error_pattern}
```

### Example PDFs (smallest, most reproducible)
| # | Path | Size | Error |
|---|------|------|-------|
{example_table}

### Root Cause Hints
- **Crate:** Likely in `{crate_name}`
- **Category:** `{error_category}`

### Fix Approach
1. Reproduce with smallest example PDF
2. Understand root cause conceptually (not PDF-specific)
3. Implement generic fix
4. Add example PDFs as regression tests
5. Re-run corpus to verify

### Labels
`cluster:{error_category}` `severity:{severity}` `volume:{volume_label}` `crate:{crate_name}`
"#,
        test_name = cluster.test_name,
        error_pattern = cluster.error_pattern,
        error_category = cluster.error_category,
        pdf_count = cluster.pdf_count,
        priority = cluster.priority_score,
        severity = cluster.severity,
        first_seen = cluster.first_seen,
        run_id = run_id,
        trend_icon = trend_icon,
        example_table = example_table,
        crate_name = crate_name,
        volume_label = volume_label,
    )
}

pub fn generate_issue_title(cluster: &Cluster) -> String {
    let pattern_short = truncate_utf8(&cluster.error_pattern, 60);
    format!(
        "[cluster] {}: {} ({} PDFs)",
        cluster.test_name, pattern_short, cluster.pdf_count
    )
}

pub fn generate_issue_labels(cluster: &Cluster) -> Vec<String> {
    let crate_name = likely_crate_for_test(&cluster.test_name);
    vec![
        format!("cluster:{}", cluster.error_category),
        format!("severity:{}", cluster.severity),
        format!("crate:{}", crate_name),
        "auto-cluster".to_string(),
    ]
}

pub fn generate_update_comment(cluster: &Cluster, run_id: &str) -> String {
    format!(
        "**Updated in run `{run_id}`:** now affects **{count}** PDFs (trend: {trend})\n\nPriority: {priority:.0}",
        run_id = run_id,
        count = cluster.pdf_count,
        trend = cluster.trend,
        priority = cluster.priority_score,
    )
}

pub fn generate_resolved_comment(run_id: &str) -> String {
    format!("Resolved: cluster has 0 PDFs in run `{run_id}`")
}
