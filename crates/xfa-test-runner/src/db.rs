use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

use crate::classifier::ErrorCategory;
use crate::tests::TestStatus;

pub struct Database {
    conn: Mutex<Connection>,
}

#[allow(dead_code)]
pub struct ClusterRow {
    pub cluster_id: String,
    pub test_name: String,
    pub error_category: String,
    pub error_pattern: String,
    pub pdf_count: i64,
    pub status: String,
    pub first_seen_run: Option<String>,
    pub github_issue_number: Option<u64>,
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.create_tables()?;
        Ok(db)
    }

    fn create_tables(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS runs (
                run_id TEXT PRIMARY KEY,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                corpus_path TEXT NOT NULL,
                total_pdfs INTEGER,
                git_commit TEXT,
                rust_version TEXT
            );

            CREATE TABLE IF NOT EXISTS test_results (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL REFERENCES runs(run_id),
                pdf_path TEXT NOT NULL,
                pdf_hash TEXT NOT NULL,
                pdf_size INTEGER NOT NULL,
                test_name TEXT NOT NULL,
                status TEXT NOT NULL,
                error_message TEXT,
                error_category TEXT,
                duration_ms INTEGER NOT NULL,
                oracle_score REAL,
                metadata_json TEXT,
                timestamp TEXT NOT NULL,
                UNIQUE(run_id, pdf_path, test_name)
            );

            CREATE TABLE IF NOT EXISTS error_clusters (
                cluster_id TEXT PRIMARY KEY,
                test_name TEXT NOT NULL,
                error_category TEXT NOT NULL,
                error_pattern TEXT NOT NULL,
                pdf_count INTEGER NOT NULL,
                first_seen_run TEXT,
                last_seen_run TEXT,
                github_issue_number INTEGER,
                status TEXT DEFAULT 'open'
            );

            CREATE TABLE IF NOT EXISTS oracle_cache (
                oracle_name TEXT NOT NULL,
                pdf_hash TEXT NOT NULL,
                result_json TEXT NOT NULL,
                cached_at TEXT NOT NULL,
                PRIMARY KEY (oracle_name, pdf_hash)
            );

            CREATE INDEX IF NOT EXISTS idx_results_status ON test_results(status);
            CREATE INDEX IF NOT EXISTS idx_results_category ON test_results(error_category);
            CREATE INDEX IF NOT EXISTS idx_results_hash ON test_results(pdf_hash);
            CREATE INDEX IF NOT EXISTS idx_results_run ON test_results(run_id);",
        )?;
        Ok(())
    }

    pub fn start_run(
        &self,
        run_id: &str,
        corpus_path: &str,
        total_pdfs: usize,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        let git_commit = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string());
        let rust_version = std::process::Command::new("rustc")
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string());

        conn.execute(
            "INSERT OR REPLACE INTO runs (run_id, started_at, corpus_path, total_pdfs, git_commit, rust_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                run_id,
                chrono::Utc::now().to_rfc3339(),
                corpus_path,
                total_pdfs as i64,
                git_commit,
                rust_version,
            ],
        )?;
        Ok(())
    }

    pub fn finish_run(&self, run_id: &str) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE runs SET finished_at = ?1 WHERE run_id = ?2",
            params![chrono::Utc::now().to_rfc3339(), run_id],
        )?;
        Ok(())
    }

    pub fn insert_result(&self, result: &TestResultRow) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO test_results
             (run_id, pdf_path, pdf_hash, pdf_size, test_name, status, error_message, error_category, duration_ms, oracle_score, metadata_json, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                result.run_id,
                result.pdf_path,
                result.pdf_hash,
                result.pdf_size,
                result.test_name,
                result.status,
                result.error_message,
                result.error_category,
                result.duration_ms,
                result.oracle_score,
                result.metadata_json,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn tests_completed_for_pdf(&self, run_id: &str, pdf_path: &str) -> usize {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM test_results WHERE run_id = ?1 AND pdf_path = ?2",
            params![run_id, pdf_path],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize
    }

    pub fn summary(&self, run_id: &str) -> RunSummary {
        let conn = self.conn.lock().unwrap();
        let mut summary = RunSummary::default();

        let mut stmt = conn
            .prepare("SELECT status, COUNT(*) FROM test_results WHERE run_id = ?1 GROUP BY status")
            .unwrap();
        let rows = stmt
            .query_map(params![run_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .unwrap();

        for row in rows.flatten() {
            match row.0.as_str() {
                "pass" => summary.pass = row.1 as usize,
                "fail" => summary.fail = row.1 as usize,
                "crash" => summary.crash = row.1 as usize,
                "timeout" => summary.timeout = row.1 as usize,
                "skip" => summary.skip = row.1 as usize,
                _ => {}
            }
        }
        summary.total =
            summary.pass + summary.fail + summary.crash + summary.timeout + summary.skip;
        summary
    }

    pub fn clusters(&self, run_id: &str) -> Vec<ClusterRow> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT test_name, error_category, error_message, COUNT(*) as cnt
                 FROM test_results
                 WHERE run_id = ?1 AND status != 'pass' AND status != 'skip'
                 GROUP BY test_name, error_category, error_message
                 ORDER BY cnt DESC",
            )
            .unwrap();

        let rows = stmt
            .query_map(params![run_id], |row| {
                let test_name: String = row.get(0)?;
                let error_category: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
                let error_pattern: String = row.get::<_, Option<String>>(2)?.unwrap_or_default();
                let pdf_count: i64 = row.get(3)?;
                let cluster_id = format!("{}-{}", test_name, error_category);
                Ok(ClusterRow {
                    cluster_id,
                    test_name,
                    error_category,
                    error_pattern,
                    pdf_count,
                    status: "open".to_string(),
                    first_seen_run: None,
                    github_issue_number: None,
                })
            })
            .unwrap();

        rows.flatten().collect()
    }

    /// Detailed comparison between two runs: per-test metrics, cluster diffs, verdict.
    pub fn compare_runs_detailed(&self, run_a: &str, run_b: &str) -> DetailedComparison {
        let summary_a = self.summary(run_a);
        let summary_b = self.summary(run_b);

        let test_metrics_a = self.per_test_metrics(run_a);
        let test_metrics_b = self.per_test_metrics(run_b);

        let clusters_a = self.clusters(run_a);
        let clusters_b = self.clusters(run_b);

        // Build cluster key sets for diffing
        let keys_a: std::collections::HashSet<String> =
            clusters_a.iter().map(|c| c.cluster_id.clone()).collect();
        let keys_b: std::collections::HashSet<String> =
            clusters_b.iter().map(|c| c.cluster_id.clone()).collect();

        let resolved: Vec<ClusterDelta> = clusters_a
            .iter()
            .filter(|c| !keys_b.contains(&c.cluster_id))
            .map(|c| ClusterDelta {
                cluster_id: c.cluster_id.clone(),
                test_name: c.test_name.clone(),
                category: c.error_category.clone(),
                before_count: c.pdf_count,
                after_count: 0,
            })
            .collect();

        let new_clusters: Vec<ClusterDelta> = clusters_b
            .iter()
            .filter(|c| !keys_a.contains(&c.cluster_id))
            .map(|c| ClusterDelta {
                cluster_id: c.cluster_id.clone(),
                test_name: c.test_name.clone(),
                category: c.error_category.clone(),
                before_count: 0,
                after_count: c.pdf_count,
            })
            .collect();

        let changed: Vec<ClusterDelta> = clusters_b
            .iter()
            .filter(|cb| keys_a.contains(&cb.cluster_id))
            .filter_map(|cb| {
                let ca = clusters_a
                    .iter()
                    .find(|ca| ca.cluster_id == cb.cluster_id)?;
                if ca.pdf_count != cb.pdf_count {
                    Some(ClusterDelta {
                        cluster_id: cb.cluster_id.clone(),
                        test_name: cb.test_name.clone(),
                        category: cb.error_category.clone(),
                        before_count: ca.pdf_count,
                        after_count: cb.pdf_count,
                    })
                } else {
                    None
                }
            })
            .collect();

        let verdict = if !new_clusters.is_empty() && new_clusters.len() > resolved.len() {
            Verdict::Regression
        } else if resolved.is_empty() && new_clusters.is_empty() && changed.is_empty() {
            Verdict::Neutral
        } else if resolved.len() >= new_clusters.len() && !resolved.is_empty() {
            Verdict::NetImprovement
        } else {
            Verdict::Neutral
        };

        // Collect all test names from both runs
        let mut all_tests: Vec<String> = test_metrics_a
            .keys()
            .chain(test_metrics_b.keys())
            .cloned()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        all_tests.sort();

        let metric_deltas: Vec<MetricDelta> = all_tests
            .iter()
            .map(|name| {
                let a = test_metrics_a.get(name);
                let b = test_metrics_b.get(name);
                let rate_a = a.map(|m| m.pass_rate()).unwrap_or(0.0);
                let rate_b = b.map(|m| m.pass_rate()).unwrap_or(0.0);
                MetricDelta {
                    test_name: name.clone(),
                    before_rate: rate_a,
                    after_rate: rate_b,
                    before_crashes: a.map(|m| m.crash).unwrap_or(0),
                    after_crashes: b.map(|m| m.crash).unwrap_or(0),
                }
            })
            .collect();

        DetailedComparison {
            run_a: run_a.to_string(),
            run_b: run_b.to_string(),
            summary_a,
            summary_b,
            metric_deltas,
            resolved,
            new_clusters,
            changed,
            verdict,
        }
    }

    /// Get per-test pass/fail/crash counts for a run.
    fn per_test_metrics(&self, run_id: &str) -> std::collections::HashMap<String, TestMetric> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT test_name, status, COUNT(*) FROM test_results
                 WHERE run_id = ?1 GROUP BY test_name, status",
            )
            .unwrap();

        let rows = stmt
            .query_map(params![run_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .unwrap();

        let mut metrics: std::collections::HashMap<String, TestMetric> =
            std::collections::HashMap::new();
        for row in rows.flatten() {
            let entry = metrics.entry(row.0).or_default();
            match row.1.as_str() {
                "pass" => entry.pass = row.2 as usize,
                "fail" => entry.fail = row.2 as usize,
                "crash" => entry.crash = row.2 as usize,
                "timeout" => entry.timeout = row.2 as usize,
                "skip" => entry.skip = row.2 as usize,
                _ => {}
            }
        }
        metrics
    }

    /// Get example PDFs for a cluster, ordered by file size (smallest first).
    pub fn cluster_examples(
        &self,
        run_id: &str,
        test_name: &str,
        error_category: &str,
        limit: usize,
    ) -> Vec<ClusterExample> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT pdf_path, pdf_size, pdf_hash, error_message
                 FROM test_results
                 WHERE run_id = ?1 AND test_name = ?2 AND error_category = ?3
                   AND status != 'pass' AND status != 'skip'
                 ORDER BY pdf_size ASC
                 LIMIT ?4",
            )
            .unwrap();

        let rows = stmt
            .query_map(
                params![run_id, test_name, error_category, limit as i64],
                |row| {
                    Ok(ClusterExample {
                        pdf_path: row.get(0)?,
                        pdf_size: row.get(1)?,
                        pdf_hash: row.get(2)?,
                        error_message: row.get(3)?,
                    })
                },
            )
            .unwrap();

        rows.flatten().collect()
    }

    pub fn get_oracle_cache(&self, oracle_name: &str, pdf_hash: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT result_json FROM oracle_cache WHERE oracle_name = ?1 AND pdf_hash = ?2",
            params![oracle_name, pdf_hash],
            |row| row.get(0),
        )
        .ok()
    }

    pub fn set_oracle_cache(
        &self,
        oracle_name: &str,
        pdf_hash: &str,
        result_json: &str,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO oracle_cache (oracle_name, pdf_hash, result_json, cached_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                oracle_name,
                pdf_hash,
                result_json,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn latest_run_id(&self) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT run_id FROM runs ORDER BY started_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok()
    }

    pub fn latest_run_id_for_corpus(&self, corpus_path: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT run_id FROM runs WHERE corpus_path = ?1 AND finished_at IS NULL ORDER BY started_at DESC LIMIT 1",
            params![corpus_path],
            |row| row.get(0),
        )
        .ok()
    }

    /// Query all failures for a run, returning (test_name, status, error_category, error_message, pdf_path, pdf_size).
    #[allow(dead_code)]
    pub fn query_failures(
        &self,
        run_id: &str,
    ) -> Vec<(String, String, String, String, String, i64)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT test_name, status, COALESCE(error_category, 'unknown'),
                        COALESCE(error_message, ''), pdf_path, pdf_size
                 FROM test_results
                 WHERE run_id = ?1 AND status != 'pass' AND status != 'skip'",
            )
            .unwrap();

        let rows = stmt
            .query_map(params![run_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .unwrap();

        rows.flatten().collect()
    }

    /// Load all stored clusters from the error_clusters table.
    #[allow(dead_code)]
    pub fn load_stored_clusters(&self) -> Vec<ClusterRow> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT cluster_id, test_name, error_category, error_pattern,
                        pdf_count, COALESCE(status, 'open'), first_seen_run, github_issue_number
                 FROM error_clusters",
            )
            .unwrap();

        let rows = stmt
            .query_map([], |row| {
                Ok(ClusterRow {
                    cluster_id: row.get(0)?,
                    test_name: row.get(1)?,
                    error_category: row.get(2)?,
                    error_pattern: row.get(3)?,
                    pdf_count: row.get(4)?,
                    status: row.get(5)?,
                    first_seen_run: row.get(6)?,
                    github_issue_number: row.get::<_, Option<i64>>(7)?.map(|n| n as u64),
                })
            })
            .unwrap();

        rows.flatten().collect()
    }

    /// Insert or update a cluster in the error_clusters table.
    #[allow(dead_code, clippy::too_many_arguments)]
    pub fn upsert_cluster(
        &self,
        cluster_id: &str,
        test_name: &str,
        error_category: &str,
        error_pattern: &str,
        pdf_count: i64,
        first_seen_run: &str,
        last_seen_run: &str,
        github_issue: Option<u64>,
        status: &str,
    ) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO error_clusters
             (cluster_id, test_name, error_category, error_pattern, pdf_count,
              first_seen_run, last_seen_run, github_issue_number, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                cluster_id,
                test_name,
                error_category,
                error_pattern,
                pdf_count,
                first_seen_run,
                last_seen_run,
                github_issue.map(|n| n as i64),
                status,
            ],
        )?;
        Ok(())
    }

    /// Get run info (git_commit, timestamp) for a given run_id.
    #[allow(dead_code)]
    pub fn run_info(&self, run_id: &str) -> Option<(String, String)> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(git_commit, 'unknown'), COALESCE(started_at, 'unknown')
             FROM runs WHERE run_id = ?1",
            params![run_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .ok()
    }

    /// Get the run_id of the previous run (by start time).
    #[allow(dead_code)]
    pub fn previous_run_id(&self, run_id: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT run_id FROM runs
             WHERE started_at < (SELECT started_at FROM runs WHERE run_id = ?1)
             ORDER BY started_at DESC LIMIT 1",
            params![run_id],
            |row| row.get(0),
        )
        .ok()
    }
}

pub struct TestResultRow {
    pub run_id: String,
    pub pdf_path: String,
    pub pdf_hash: String,
    pub pdf_size: i64,
    pub test_name: String,
    pub status: String,
    pub error_message: Option<String>,
    pub error_category: Option<String>,
    pub duration_ms: i64,
    pub oracle_score: Option<f64>,
    pub metadata_json: Option<String>,
}

impl TestResultRow {
    #[allow(clippy::too_many_arguments)]
    pub fn from_test_result(
        run_id: &str,
        pdf_path: &str,
        pdf_hash: &str,
        pdf_size: i64,
        test_name: &str,
        status: &TestStatus,
        error_message: Option<&str>,
        error_category: Option<&ErrorCategory>,
        duration_ms: u64,
    ) -> Self {
        Self {
            run_id: run_id.to_string(),
            pdf_path: pdf_path.to_string(),
            pdf_hash: pdf_hash.to_string(),
            pdf_size,
            test_name: test_name.to_string(),
            status: status.as_str().to_string(),
            error_message: error_message.map(|s| s.to_string()),
            error_category: error_category.map(|c| c.to_string()),
            duration_ms: duration_ms as i64,
            oracle_score: None,
            metadata_json: None,
        }
    }
}

#[derive(Default, Debug)]
pub struct RunSummary {
    pub total: usize,
    pub pass: usize,
    pub fail: usize,
    pub crash: usize,
    pub timeout: usize,
    pub skip: usize,
}

impl std::fmt::Display for RunSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Total: {} | Pass: {} ({:.1}%) | Fail: {} | Crash: {} | Timeout: {} | Skip: {}",
            self.total,
            self.pass,
            if self.total > 0 {
                self.pass as f64 / self.total as f64 * 100.0
            } else {
                0.0
            },
            self.fail,
            self.crash,
            self.timeout,
            self.skip,
        )
    }
}

// ─── Detailed comparison types ─────────────────────────────────────

#[derive(Default)]
pub struct TestMetric {
    pub pass: usize,
    pub fail: usize,
    pub crash: usize,
    pub timeout: usize,
    pub skip: usize,
}

impl TestMetric {
    pub fn total(&self) -> usize {
        self.pass + self.fail + self.crash + self.timeout + self.skip
    }

    pub fn pass_rate(&self) -> f64 {
        let t = self.total();
        if t == 0 {
            0.0
        } else {
            self.pass as f64 / t as f64 * 100.0
        }
    }
}

pub struct MetricDelta {
    pub test_name: String,
    pub before_rate: f64,
    pub after_rate: f64,
    pub before_crashes: usize,
    pub after_crashes: usize,
}

#[allow(dead_code)]
pub struct ClusterDelta {
    pub cluster_id: String,
    pub test_name: String,
    pub category: String,
    pub before_count: i64,
    pub after_count: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    NetImprovement,
    Neutral,
    Regression,
}

#[allow(dead_code)]
pub struct DetailedComparison {
    pub run_a: String,
    pub run_b: String,
    pub summary_a: RunSummary,
    pub summary_b: RunSummary,
    pub metric_deltas: Vec<MetricDelta>,
    pub resolved: Vec<ClusterDelta>,
    pub new_clusters: Vec<ClusterDelta>,
    pub changed: Vec<ClusterDelta>,
    pub verdict: Verdict,
}

impl std::fmt::Display for DetailedComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Run Comparison: {} -> {}", self.run_a, self.run_b)?;
        writeln!(f, "{}", "=".repeat(60))?;
        writeln!(f)?;

        // Per-test metrics
        writeln!(
            f,
            "  {:<20} {:>10} {:>10} {:>10}",
            "Test", "Before", "After", "Delta"
        )?;
        writeln!(f, "  {}", "-".repeat(54))?;
        for m in &self.metric_deltas {
            let delta = m.after_rate - m.before_rate;
            let indicator = if delta > 0.1 {
                " +"
            } else if delta < -0.1 {
                " -"
            } else {
                "  "
            };
            writeln!(
                f,
                "  {:<20} {:>9.1}% {:>9.1}% {:>+9.1}%{}",
                m.test_name, m.before_rate, m.after_rate, delta, indicator
            )?;
        }

        // Crash summary
        let crashes_before: usize = self.metric_deltas.iter().map(|m| m.before_crashes).sum();
        let crashes_after: usize = self.metric_deltas.iter().map(|m| m.after_crashes).sum();
        if crashes_before > 0 || crashes_after > 0 {
            writeln!(f)?;
            writeln!(
                f,
                "  Panics: {} -> {} ({:+})",
                crashes_before,
                crashes_after,
                crashes_after as i64 - crashes_before as i64
            )?;
        }

        // Cluster changes
        if !self.resolved.is_empty() {
            writeln!(f)?;
            writeln!(f, "  Resolved clusters:")?;
            for c in &self.resolved {
                writeln!(
                    f,
                    "    RESOLVED  {:<20} {} ({} PDFs)",
                    c.category, c.test_name, c.before_count
                )?;
            }
        }
        if !self.new_clusters.is_empty() {
            writeln!(f)?;
            writeln!(f, "  New clusters:")?;
            for c in &self.new_clusters {
                writeln!(
                    f,
                    "    NEW       {:<20} {} ({} PDFs)",
                    c.category, c.test_name, c.after_count
                )?;
            }
        }
        if !self.changed.is_empty() {
            writeln!(f)?;
            writeln!(f, "  Changed clusters:")?;
            for c in &self.changed {
                let delta = c.after_count - c.before_count;
                writeln!(
                    f,
                    "    CHANGED   {:<20} {} ({} -> {} PDFs, {:+})",
                    c.category, c.test_name, c.before_count, c.after_count, delta
                )?;
            }
        }

        // Verdict
        writeln!(f)?;
        let v = match self.verdict {
            Verdict::NetImprovement => format!(
                "NET IMPROVEMENT (+{} resolved, +{} new)",
                self.resolved.len(),
                self.new_clusters.len()
            ),
            Verdict::Neutral => "NEUTRAL (no cluster changes)".to_string(),
            Verdict::Regression => format!(
                "REGRESSION (+{} new clusters, only {} resolved)",
                self.new_clusters.len(),
                self.resolved.len()
            ),
        };
        writeln!(f, "  Verdict: {v}")?;
        Ok(())
    }
}

#[allow(dead_code)]
pub struct ClusterExample {
    pub pdf_path: String,
    pub pdf_size: i64,
    pub pdf_hash: String,
    pub error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        Database::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn ensure_run(db: &Database, run_id: &str) {
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO runs (run_id, started_at, corpus_path, total_pdfs)
             VALUES (?1, '2026-01-01T00:00:00Z', '/test', 0)",
            params![run_id],
        )
        .unwrap();
    }

    fn insert_row(db: &Database, run_id: &str, test_name: &str, status: &str, pdf: &str) {
        ensure_run(db, run_id);
        let conn = db.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO test_results
             (run_id, pdf_path, pdf_hash, pdf_size, test_name, status, error_message, error_category, duration_ms, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                run_id,
                pdf,
                "hash",
                1000i64,
                test_name,
                status,
                if status == "fail" {
                    Some("some error")
                } else {
                    None::<&str>
                },
                if status == "fail" {
                    Some("unknown")
                } else {
                    None::<&str>
                },
                10i64,
                "2026-01-01T00:00:00Z"
            ],
        )
        .unwrap();
    }

    #[test]
    fn detailed_compare_net_improvement() {
        let db = test_db();

        // Run A: parse fails on 2 PDFs
        insert_row(&db, "a", "parse", "pass", "1.pdf");
        insert_row(&db, "a", "parse", "fail", "2.pdf");
        insert_row(&db, "a", "parse", "fail", "3.pdf");

        // Run B: all pass
        insert_row(&db, "b", "parse", "pass", "1.pdf");
        insert_row(&db, "b", "parse", "pass", "2.pdf");
        insert_row(&db, "b", "parse", "pass", "3.pdf");

        let cmp = db.compare_runs_detailed("a", "b");
        assert_eq!(cmp.verdict, Verdict::NetImprovement);
        assert!(!cmp.resolved.is_empty());
        assert!(cmp.new_clusters.is_empty());
    }

    #[test]
    fn detailed_compare_regression() {
        let db = test_db();

        // Run A: all pass
        insert_row(&db, "a", "parse", "pass", "1.pdf");
        insert_row(&db, "a", "render", "pass", "1.pdf");

        // Run B: render fails, and a new text_extract failure appears
        insert_row(&db, "b", "parse", "pass", "1.pdf");
        insert_row(&db, "b", "render", "fail", "1.pdf");
        insert_row(&db, "b", "render", "fail", "2.pdf");

        let cmp = db.compare_runs_detailed("a", "b");
        assert_eq!(cmp.verdict, Verdict::Regression);
        assert!(cmp.resolved.is_empty());
        assert!(!cmp.new_clusters.is_empty());
    }

    #[test]
    fn detailed_compare_neutral() {
        let db = test_db();

        insert_row(&db, "a", "parse", "pass", "1.pdf");
        insert_row(&db, "b", "parse", "pass", "1.pdf");

        let cmp = db.compare_runs_detailed("a", "b");
        assert_eq!(cmp.verdict, Verdict::Neutral);
    }

    #[test]
    fn cluster_examples_ordered_by_size() {
        let db = test_db();
        ensure_run(&db, "r1");
        let conn = db.conn.lock().unwrap();
        // Insert two failures with different sizes
        conn.execute(
            "INSERT INTO test_results
             (run_id, pdf_path, pdf_hash, pdf_size, test_name, status, error_message, error_category, duration_ms, timestamp)
             VALUES ('r1', 'big.pdf', 'h1', 50000, 'parse', 'fail', 'err', 'invalid_xref', 10, '2026-01-01')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO test_results
             (run_id, pdf_path, pdf_hash, pdf_size, test_name, status, error_message, error_category, duration_ms, timestamp)
             VALUES ('r1', 'small.pdf', 'h2', 5000, 'parse', 'fail', 'err', 'invalid_xref', 10, '2026-01-01')",
            [],
        ).unwrap();
        drop(conn);

        let examples = db.cluster_examples("r1", "parse", "invalid_xref", 5);
        assert_eq!(examples.len(), 2);
        assert_eq!(examples[0].pdf_path, "small.pdf");
        assert_eq!(examples[1].pdf_path, "big.pdf");
    }

    #[test]
    fn display_detailed_comparison() {
        let db = test_db();
        insert_row(&db, "a", "parse", "pass", "1.pdf");
        insert_row(&db, "a", "parse", "fail", "2.pdf");
        insert_row(&db, "b", "parse", "pass", "1.pdf");
        insert_row(&db, "b", "parse", "pass", "2.pdf");

        let cmp = db.compare_runs_detailed("a", "b");
        let output = format!("{cmp}");
        assert!(output.contains("Run Comparison: a -> b"));
        assert!(output.contains("parse"));
        assert!(output.contains("Verdict"));
    }
}
