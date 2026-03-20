//! Standalone oracle result database.
//!
//! Stores ground-truth results from external tools (veraPDF, poppler, etc.)
//! keyed by `(pdf_hash, oracle_tool, oracle_version)`. This DB survives across
//! corpus runs and is shared between machines.

use rusqlite::{params, Connection};
use std::path::Path;

pub struct OracleDb {
    conn: Connection,
}

impl OracleDb {
    /// Open (or create) the oracle database at the given path.
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("oracle db open: {e}"))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("oracle db pragma: {e}"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS oracle_results (
                pdf_hash TEXT NOT NULL,
                oracle_tool TEXT NOT NULL,
                oracle_version TEXT NOT NULL,
                profile TEXT,
                result_json TEXT NOT NULL,
                cached_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (pdf_hash, oracle_tool, oracle_version)
            );
            CREATE INDEX IF NOT EXISTS idx_oracle_hash ON oracle_results(pdf_hash);",
        )
        .map_err(|e| format!("oracle db schema: {e}"))?;
        Ok(Self { conn })
    }

    /// Look up a cached oracle result.
    pub fn lookup(
        &self,
        pdf_hash: &str,
        oracle_tool: &str,
        oracle_version: &str,
    ) -> Option<String> {
        self.conn
            .query_row(
                "SELECT result_json FROM oracle_results
                 WHERE pdf_hash = ?1 AND oracle_tool = ?2 AND oracle_version = ?3",
                params![pdf_hash, oracle_tool, oracle_version],
                |row| row.get(0),
            )
            .ok()
    }

    /// Store an oracle result.
    pub fn store(
        &self,
        pdf_hash: &str,
        oracle_tool: &str,
        oracle_version: &str,
        profile: Option<&str>,
        result_json: &str,
    ) {
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO oracle_results
             (pdf_hash, oracle_tool, oracle_version, profile, result_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![pdf_hash, oracle_tool, oracle_version, profile, result_json],
        );
    }

    /// Count total cached entries.
    #[allow(dead_code)]
    pub fn count(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM oracle_results", [], |row| row.get(0))
            .unwrap_or(0)
    }

    /// Count entries for a specific tool+version.
    pub fn count_for(&self, oracle_tool: &str, oracle_version: &str) -> usize {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM oracle_results WHERE oracle_tool = ?1 AND oracle_version = ?2",
                params![oracle_tool, oracle_version],
                |row| row.get(0),
            )
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let db = OracleDb::open(Path::new(":memory:")).unwrap();
        assert_eq!(db.count(), 0);
        assert!(db.lookup("abc123", "verapdf", "1.28").is_none());

        db.store(
            "abc123",
            "verapdf",
            "1.28",
            Some("PDF/A-2B"),
            r#"{"compliant":false}"#,
        );
        assert_eq!(db.count(), 1);

        let result = db.lookup("abc123", "verapdf", "1.28").unwrap();
        assert!(result.contains("compliant"));

        // Different version = cache miss
        assert!(db.lookup("abc123", "verapdf", "1.29").is_none());
    }

    #[test]
    fn skip_existing() {
        let db = OracleDb::open(Path::new(":memory:")).unwrap();
        db.store("hash1", "verapdf", "1.28", None, "{}");
        db.store("hash2", "verapdf", "1.28", None, "{}");
        assert_eq!(db.count_for("verapdf", "1.28"), 2);
        // Overwrite
        db.store("hash1", "verapdf", "1.28", None, r#"{"new":true}"#);
        assert_eq!(db.count_for("verapdf", "1.28"), 2);
    }
}
