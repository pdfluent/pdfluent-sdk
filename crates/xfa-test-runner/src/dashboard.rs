use crate::clustering::Cluster;
use crate::db::{Database, RunSummary};

pub struct DashboardData {
    pub run_id: String,
    pub git_commit: String,
    pub timestamp: String,
    pub summary: RunSummary,
    pub clusters: Vec<Cluster>,
    pub prev_summary: Option<RunSummary>,
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

    DashboardData {
        run_id: run_id.to_string(),
        git_commit,
        timestamp,
        summary,
        clusters,
        prev_summary,
    }
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
        top_cluster = top_cluster,
    )
}

fn render_clusters(data: &DashboardData) -> String {
    let mut rows = String::new();
    for c in &data.clusters {
        let pattern_short = if c.error_pattern.len() > 50 {
            format!("{}...", &c.error_pattern[..50])
        } else {
            c.error_pattern.clone()
        };
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
