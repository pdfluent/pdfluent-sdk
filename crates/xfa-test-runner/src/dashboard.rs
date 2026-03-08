use crate::clustering::Cluster;
use crate::db::{Database, RunSummary, RunTrendEntry};

pub struct DashboardData {
    pub run_id: String,
    pub git_commit: String,
    pub timestamp: String,
    pub summary: RunSummary,
    pub clusters: Vec<Cluster>,
    pub prev_summary: Option<RunSummary>,
    pub quality_distribution: Vec<(String, String, usize)>,
    pub trend: Vec<RunTrendEntry>,
}

pub fn generate_dashboard(
    data: &DashboardData,
    output_dir: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::create_dir_all(output_dir)?;

    let index_html = render_index(data);
    std::fs::write(output_dir.join("index.html"), index_html)?;

    let clusters_html = render_clusters(data);
    std::fs::write(output_dir.join("clusters.html"), clusters_html)?;

    let run_html = render_run_detail(data);
    std::fs::write(
        output_dir.join(format!("run-{}.html", data.run_id)),
        run_html,
    )?;

    Ok(())
}

pub fn collect_dashboard_data(
    db: &Database,
    run_id: &str,
    clusters: Vec<Cluster>,
) -> DashboardData {
    let summary = db.summary(run_id);
    let (git_commit, timestamp) = db
        .run_info(run_id)
        .unwrap_or(("unknown".to_string(), "unknown".to_string()));
    let prev_summary = db.previous_run_id(run_id).map(|prev| db.summary(&prev));
    let quality_distribution = db.oracle_score_distribution(run_id);
    let trend = db.run_trend();

    DashboardData {
        run_id: run_id.to_string(),
        git_commit,
        timestamp,
        summary,
        clusters,
        prev_summary,
        quality_distribution,
        trend,
    }
}

fn render_trend_section(data: &DashboardData) -> String {
    if data.trend.len() < 2 {
        return String::new();
    }

    // Build SVG trend chart
    let entries = &data.trend;
    let width = 700;
    let height = 200;
    let margin = 40;
    let plot_w = width - 2 * margin;
    let plot_h = height - 2 * margin;

    let min_rate = entries
        .iter()
        .map(|e| e.pass_rate)
        .fold(100.0_f64, f64::min)
        .max(0.0);
    let max_rate = entries
        .iter()
        .map(|e| e.pass_rate)
        .fold(0.0_f64, f64::max)
        .min(100.0);

    // Ensure some vertical range
    let y_min = (min_rate - 5.0).max(0.0);
    let y_max = (max_rate + 2.0).min(100.0);
    let y_range = (y_max - y_min).max(1.0);

    let n = entries.len();
    let points: Vec<String> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let x = margin + (i * plot_w) / (n - 1).max(1);
            let y_frac = (e.pass_rate - y_min) / y_range;
            let y = margin + plot_h - (y_frac * plot_h as f64) as usize;
            format!("{x},{y}")
        })
        .collect();
    let polyline = points.join(" ");

    // Labels for first and last points
    let first = &entries[0];
    let last = entries.last().unwrap();

    // Table rows
    let mut rows = String::new();
    for entry in entries.iter().rev().take(10) {
        let oracle = entry
            .avg_oracle_score
            .map(|s| format!("{s:.3}"))
            .unwrap_or_else(|| "-".to_string());
        let short_id = if entry.run_id.len() > 30 {
            &entry.run_id[..30]
        } else {
            &entry.run_id
        };
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{:.1}%</td><td>{}</td><td>{}</td></tr>\n",
            short_id, entry.pass_rate, entry.total, oracle
        ));
    }

    let first_label = &first.run_id[..first.run_id.len().min(20)];
    let last_label = &last.run_id[..last.run_id.len().min(20)];
    let y_first = margin - 5;
    let x_last = width - margin;
    let y_bottom = height - 5;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<div class=\"card\">\n  <h2>Pass Rate Trend</h2>\n  \
         <svg width=\"{width}\" height=\"{height}\" style=\"background: rgba(15,22,41,1); border-radius: 4px;\">\n"
    ));
    svg.push_str(&format!(
        "    <polyline points=\"{polyline}\" fill=\"none\" stroke=\"rgba(0,212,255,1)\" stroke-width=\"2\"/>\n"
    ));
    svg.push_str(&format!(
        "    <text x=\"{margin}\" y=\"{y_first}\" fill=\"rgba(136,136,136,1)\" font-size=\"10\">{:.1}%</text>\n",
        first.pass_rate
    ));
    svg.push_str(&format!(
        "    <text x=\"{x_last}\" y=\"{y_first}\" fill=\"rgba(136,136,136,1)\" font-size=\"10\">{:.1}%</text>\n",
        last.pass_rate
    ));
    svg.push_str(&format!(
        "    <text x=\"{margin}\" y=\"{y_bottom}\" fill=\"rgba(102,102,102,1)\" font-size=\"9\">{first_label}</text>\n"
    ));
    svg.push_str(&format!(
        "    <text x=\"{x_last}\" y=\"{y_bottom}\" fill=\"rgba(102,102,102,1)\" font-size=\"9\">{last_label}</text>\n"
    ));
    svg.push_str("  </svg>\n");
    svg.push_str(
        "  <table>\n    <tr><th>Run</th><th>Pass Rate</th><th>Total</th><th>Avg Oracle</th></tr>\n",
    );
    svg.push_str(&rows);
    svg.push_str("  </table>\n</div>");

    svg
}

fn render_quality_section(data: &DashboardData) -> String {
    if data.quality_distribution.is_empty() {
        return String::new();
    }

    let mut tests: Vec<String> = data
        .quality_distribution
        .iter()
        .map(|(t, _, _)| t.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    tests.sort();

    let buckets = [
        "0.0-0.5", "0.5-0.8", "0.8-0.9", "0.9-0.95", "0.95-1.0", "1.0",
    ];

    let mut rows = String::new();
    for test in &tests {
        rows.push_str(&format!("<tr><td><strong>{test}</strong></td>"));
        for bucket in &buckets {
            let count = data
                .quality_distribution
                .iter()
                .find(|(t, b, _)| t == test && b == bucket)
                .map(|(_, _, c)| *c)
                .unwrap_or(0);
            let class = if *bucket == "1.0" || *bucket == "0.95-1.0" {
                "delta-good"
            } else if *bucket == "0.0-0.5" {
                "delta-bad"
            } else {
                ""
            };
            rows.push_str(&format!("<td class=\"{class}\">{count}</td>"));
        }
        rows.push_str("</tr>\n");
    }

    format!(
        r#"
<div class="card">
  <h2>Quality Distribution (Oracle Scores)</h2>
  <table>
  <tr><th>Test</th><th>0-50%</th><th>50-80%</th><th>80-90%</th><th>90-95%</th><th>95-100%</th><th>100%</th></tr>
  {rows}
  </table>
</div>"#,
        rows = rows,
    )
}

fn pct(n: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        n as f64 / total as f64 * 100.0
    }
}

fn delta_str(current: usize, prev: Option<usize>) -> String {
    match prev {
        Some(p) => {
            let d = current as i64 - p as i64;
            if d > 0 {
                format!(" <span class=\"delta-bad\">+{d}</span>")
            } else if d < 0 {
                format!(" <span class=\"delta-good\">{d}</span>")
            } else {
                String::new()
            }
        }
        None => String::new(),
    }
}

fn bar(pct_val: f64) -> String {
    let filled = (pct_val / 5.0).round() as usize;
    let empty = 20_usize.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn render_index(data: &DashboardData) -> String {
    let s = &data.summary;
    let total = s.total;
    let pass_pct = pct(s.pass, total);

    let prev = data.prev_summary.as_ref();
    let pass_delta = delta_str(s.pass, prev.map(|p| p.pass));
    let fail_delta = delta_str(s.fail, prev.map(|p| p.fail));
    let crash_delta = delta_str(s.crash, prev.map(|p| p.crash));

    let new_clusters = data
        .clusters
        .iter()
        .filter(|c| c.trend == crate::clustering::Trend::New)
        .count();
    let resolved_clusters = data
        .clusters
        .iter()
        .filter(|c| c.trend == crate::clustering::Trend::Resolved)
        .count();

    let top_cluster = data
        .clusters
        .first()
        .map(|c| {
            format!(
                "{}: {} ({} PDFs)",
                c.test_name, c.error_pattern, c.pdf_count
            )
        })
        .unwrap_or_else(|| "none".to_string());

    format!(
        r#"<!DOCTYPE html>
<html><head>
<meta charset="utf-8">
<title>XFA Test Dashboard — Run {run_id}</title>
<style>
body {{ font-family: monospace; background: #1a1a2e; color: #e0e0e0; max-width: 900px; margin: 0 auto; padding: 20px; }}
h1 {{ color: #00d4ff; }}
.card {{ background: #16213e; border-radius: 8px; padding: 16px; margin: 12px 0; }}
.metric {{ display: flex; justify-content: space-between; align-items: center; margin: 8px 0; }}
.metric-label {{ min-width: 140px; }}
.metric-bar {{ color: #00d4ff; letter-spacing: -2px; }}
.metric-value {{ min-width: 80px; text-align: right; }}
.delta-good {{ color: #00ff88; }}
.delta-bad {{ color: #ff4444; }}
a {{ color: #00d4ff; }}
table {{ width: 100%; border-collapse: collapse; }}
th, td {{ padding: 6px 10px; text-align: left; border-bottom: 1px solid #333; }}
th {{ color: #00d4ff; }}
</style>
</head><body>
<h1>XFA-Native-Rust Test Dashboard</h1>
<div class="card">
  <strong>Run:</strong> {run_id} | <strong>Commit:</strong> {git_commit} | <strong>Time:</strong> {timestamp}
</div>

<div class="card">
  <h2>Summary</h2>
  <div class="metric">
    <span class="metric-label">Total PDFs:</span>
    <span class="metric-value">{total}</span>
  </div>
  <div class="metric">
    <span class="metric-label">Pass Rate:</span>
    <span class="metric-bar">{pass_bar}</span>
    <span class="metric-value">{pass_pct:.1}%{pass_delta}</span>
  </div>
  <div class="metric">
    <span class="metric-label">Failures:</span>
    <span class="metric-value">{fail}{fail_delta}</span>
  </div>
  <div class="metric">
    <span class="metric-label">Crashes:</span>
    <span class="metric-value">{crash}{crash_delta}</span>
  </div>
  <div class="metric">
    <span class="metric-label">Timeouts:</span>
    <span class="metric-value">{timeout}</span>
  </div>
  <div class="metric">
    <span class="metric-label">Skipped:</span>
    <span class="metric-value">{skip}</span>
  </div>
</div>

{quality_section}

{trend_section}

<div class="card">
  <h2>Clusters</h2>
  <p>Open: {open_clusters} ({new_clusters} new, {resolved_clusters} resolved)</p>
  <p>Top issue: {top_cluster}</p>
  <p><a href="clusters.html">View all clusters →</a></p>
</div>
</body></html>"#,
        run_id = data.run_id,
        git_commit = data.git_commit,
        timestamp = data.timestamp,
        total = total,
        pass_bar = bar(pass_pct),
        pass_pct = pass_pct,
        pass_delta = pass_delta,
        fail = s.fail,
        fail_delta = fail_delta,
        crash = s.crash,
        crash_delta = crash_delta,
        timeout = s.timeout,
        skip = s.skip,
        open_clusters = data.clusters.len(),
        new_clusters = new_clusters,
        resolved_clusters = resolved_clusters,
        quality_section = render_quality_section(data),
        trend_section = render_trend_section(data),
        top_cluster = top_cluster,
    )
}

fn render_clusters(data: &DashboardData) -> String {
    let mut rows = String::new();
    for c in &data.clusters {
        let pattern_short = truncate_utf8(&c.error_pattern, 50);
        let trend_class = match c.trend {
            crate::clustering::Trend::Growing => "delta-bad",
            crate::clustering::Trend::Shrinking | crate::clustering::Trend::Resolved => {
                "delta-good"
            }
            _ => "",
        };
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.0}</td><td class=\"{}\">{}</td><td>{}</td></tr>\n",
            c.test_name, c.error_category, c.pdf_count, c.priority_score, trend_class, c.trend, pattern_short,
        ));
    }

    format!(
        r#"<!DOCTYPE html>
<html><head>
<meta charset="utf-8">
<title>Clusters — Run {run_id}</title>
<style>
body {{ font-family: monospace; background: #1a1a2e; color: #e0e0e0; max-width: 1100px; margin: 0 auto; padding: 20px; }}
h1 {{ color: #00d4ff; }}
a {{ color: #00d4ff; }}
table {{ width: 100%; border-collapse: collapse; }}
th, td {{ padding: 6px 10px; text-align: left; border-bottom: 1px solid #333; }}
th {{ color: #00d4ff; }}
.delta-good {{ color: #00ff88; }}
.delta-bad {{ color: #ff4444; }}
</style>
</head><body>
<h1>Error Clusters — Run {run_id}</h1>
<p><a href="index.html">← Back to dashboard</a></p>
<table>
<tr><th>Test</th><th>Category</th><th>Count</th><th>Priority</th><th>Trend</th><th>Pattern</th></tr>
{rows}
</table>
</body></html>"#,
        run_id = data.run_id,
        rows = rows,
    )
}

fn truncate_utf8(s: &str, max_chars: usize) -> String {
    let truncated: String = s.chars().take(max_chars).collect();
    if truncated.len() < s.len() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn render_run_detail(data: &DashboardData) -> String {
    let s = &data.summary;
    format!(
        r#"<!DOCTYPE html>
<html><head>
<meta charset="utf-8">
<title>Run {run_id}</title>
<style>
body {{ font-family: monospace; background: #1a1a2e; color: #e0e0e0; max-width: 900px; margin: 0 auto; padding: 20px; }}
h1 {{ color: #00d4ff; }}
a {{ color: #00d4ff; }}
.stat {{ margin: 4px 0; }}
</style>
</head><body>
<h1>Run Detail: {run_id}</h1>
<p><a href="index.html">← Back to dashboard</a></p>
<div class="stat"><strong>Commit:</strong> {git_commit}</div>
<div class="stat"><strong>Timestamp:</strong> {timestamp}</div>
<div class="stat"><strong>Total:</strong> {total}</div>
<div class="stat"><strong>Pass:</strong> {pass} ({pass_pct:.1}%)</div>
<div class="stat"><strong>Fail:</strong> {fail}</div>
<div class="stat"><strong>Crash:</strong> {crash}</div>
<div class="stat"><strong>Timeout:</strong> {timeout}</div>
<div class="stat"><strong>Skip:</strong> {skip}</div>
<div class="stat"><strong>Clusters:</strong> {cluster_count}</div>
</body></html>"#,
        run_id = data.run_id,
        git_commit = data.git_commit,
        timestamp = data.timestamp,
        total = s.total,
        pass = s.pass,
        pass_pct = pct(s.pass, s.total),
        fail = s.fail,
        crash = s.crash,
        timeout = s.timeout,
        skip = s.skip,
        cluster_count = data.clusters.len(),
    )
}
