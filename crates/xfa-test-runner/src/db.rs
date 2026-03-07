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
                let error_category: String =
                    row.get::<_, Option<String>>(1)?.unwrap_or_default();
                let error_pattern: String =
                    row.get::<_, Option<String>>(2)?.unwrap_or_default();
                let pdf_count: i64 = row.get(3)?;
                let cluster_id = format!("{}-{}", test_name, error_category);
                Ok(ClusterRow {
                    cluster_id,
                    test_name,
                    error_category,
                    error_pattern,
                    pdf_count,
                    status: "open".to_string(),
                })
            })
            .unwrap();

        rows.flatten().collect()
    }

    pub fn compare_runs(&self, run_a: &str, run_b: &str) -> CompareResult {
        let a = self.summary(run_a);
        let b = self.summary(run_b);
        CompareResult {
            run_a: run_a.to_string(),
            run_b: run_b.to_string(),
            summary_a: a,
            summary_b: b,
        }
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

pub struct CompareResult {
    pub run_a: String,
    pub run_b: String,
    pub summary_a: RunSummary,
    pub summary_b: RunSummary,
}

impl std::fmt::Display for CompareResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Run A ({}): {}", self.run_a, self.summary_a)?;
        writeln!(f, "Run B ({}): {}", self.run_b, self.summary_b)?;

        let delta_pass = self.summary_b.pass as i64 - self.summary_a.pass as i64;
        let delta_fail = self.summary_b.fail as i64 - self.summary_a.fail as i64;
        let delta_crash = self.summary_b.crash as i64 - self.summary_a.crash as i64;

        writeln!(
            f,
            "Delta: pass={:+}, fail={:+}, crash={:+}",
            delta_pass, delta_fail, delta_crash
        )
    }
}
