//! Retest failures from a previous corpus run.
//!
//! Reads failing PDF paths from a source database, re-runs the compliance
//! test with the current binary, and reports what changed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::Database;
use crate::oracle_db::OracleDb;
use crate::oracles::verapdf::VeraPdfOracle;
use crate::runner::run_single_pdf;
use crate::tests;

/// Run retest on all failures from a previous run.
pub fn retest_failures(
    source_db_path: &Path,
    source_run_id: &str,
    verapdf_path: &Path,
    oracle_db_path: Option<&Path>,
    test_filter: Option<&str>,
    timeout: u64,
    workers: usize,
) {
    // 1. Read failing PDFs from source DB
    let source_db = Database::open(source_db_path).expect("Failed to open source database");
    let failed_paths = source_db.failed_pdf_paths(source_run_id);
    let total = failed_paths.len();

    if total == 0 {
        eprintln!("No failures found in run '{source_run_id}'");
        return;
    }
    eprintln!("[retest] {total} failing PDFs from run '{source_run_id}'");

    // 2. Set up oracle
    let oracle_db = oracle_db_path.and_then(|p| OracleDb::open(p).ok());
    let oracle = {
        let mut o = VeraPdfOracle::new(verapdf_path.to_path_buf());
        if let Some(odb) = oracle_db {
            o = o.with_oracle_db(Arc::new(std::sync::Mutex::new(odb)));
        }
        Arc::new(o)
    };

    // 3. Process each PDF
    let results: Vec<RetestResult> = if workers <= 1 {
        failed_paths
            .iter()
            .enumerate()
            .map(|(i, path)| {
                if (i + 1) % 10 == 0 {
                    eprintln!("[{}/{total}]", i + 1);
                }
                run_one(path, &oracle, test_filter, timeout)
            })
            .collect()
    } else {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let counter = Arc::new(AtomicUsize::new(0));
        let oracle = Arc::clone(&oracle);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .expect("thread pool");
        let paths: Vec<String> = failed_paths;
        pool.install(|| {
            use rayon::prelude::*;
            paths
                .par_iter()
                .map(|path| {
                    let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
                    if n.is_multiple_of(20) {
                        eprintln!("[{n}/{total}]");
                    }
                    run_one(path, &oracle, test_filter, timeout)
                })
                .collect()
        })
    };

    // 4. Summarize
    let mut fixed = 0;
    let mut still_fail = 0;
    let mut fn_counts: HashMap<String, usize> = HashMap::new();
    let mut fp_counts: HashMap<String, usize> = HashMap::new();

    for r in &results {
        match r.status {
            RetestStatus::Fixed => fixed += 1,
            RetestStatus::StillFail => {
                still_fail += 1;
                for rule in &r.fn_rules {
                    *fn_counts.entry(rule.clone()).or_default() += 1;
                }
                for rule in &r.fp_rules {
                    *fp_counts.entry(rule.clone()).or_default() += 1;
                }
            }
            RetestStatus::Error => still_fail += 1,
        }
    }

    println!("=== RETEST SUMMARY ===");
    println!("Total retested: {total}");
    println!("Fixed (fail → pass): {fixed}");
    println!("Still failing: {still_fail}");
    println!();

    if !fn_counts.is_empty() {
        let mut fn_sorted: Vec<_> = fn_counts.iter().collect();
        fn_sorted.sort_by(|a, b| b.1.cmp(a.1));
        println!(
            "FN breakdown ({} total):",
            fn_sorted.iter().map(|(_, c)| *c).sum::<usize>()
        );
        for (rule, count) in &fn_sorted {
            println!("  {rule}: {count}");
        }
        println!();
    }

    if !fp_counts.is_empty() {
        let mut fp_sorted: Vec<_> = fp_counts.iter().collect();
        fp_sorted.sort_by(|a, b| b.1.cmp(a.1));
        println!(
            "FP breakdown ({} total):",
            fp_sorted.iter().map(|(_, c)| *c).sum::<usize>()
        );
        for (rule, count) in fp_sorted.iter().take(15) {
            println!("  {rule}: {count}");
        }
        if fp_sorted.len() > 15 {
            println!("  ... +{} more", fp_sorted.len() - 15);
        }
        println!();
    }

    // Per-PDF details for still-failing
    if still_fail > 0 {
        println!("=== STILL FAILING ===");
        for r in &results {
            if matches!(r.status, RetestStatus::StillFail | RetestStatus::Error) {
                let fn_str = if r.fn_rules.is_empty() {
                    String::new()
                } else {
                    format!(" FN={}", r.fn_rules.join(","))
                };
                let fp_str = if r.fp_rules.is_empty() {
                    String::new()
                } else {
                    format!(" FP={}", r.fp_rules.join(","))
                };
                println!("  {}{fn_str}{fp_str}", r.pdf_path);
            }
        }
    }
}

enum RetestStatus {
    Fixed,
    StillFail,
    Error,
}

struct RetestResult {
    pdf_path: String,
    status: RetestStatus,
    fn_rules: Vec<String>,
    fp_rules: Vec<String>,
}

fn run_one(
    pdf_path: &str,
    oracle: &Arc<VeraPdfOracle>,
    test_filter: Option<&str>,
    timeout: u64,
) -> RetestResult {
    let path = PathBuf::from(pdf_path);
    if !path.exists() {
        return RetestResult {
            pdf_path: pdf_path.to_string(),
            status: RetestStatus::Error,
            fn_rules: vec![],
            fp_rules: vec![],
        };
    }

    // Build test config with oracle
    let test_config = tests::TestConfig {
        verapdf_oracle: Some(Arc::clone(oracle)),
        #[cfg(feature = "pdfium-oracle")]
        diff_dir: None,
    };

    let mut available = tests::all_tests(test_config);
    // Only run compliance test for speed
    available.retain(|t| t.name() == "compliance");
    if let Some(filter) = test_filter {
        let names: Vec<&str> = filter.split(',').map(str::trim).collect();
        available.retain(|t| names.iter().any(|f| *f == t.name()));
    }

    match run_single_pdf(available, &path, timeout) {
        Ok(output) => {
            // Check compliance result
            for r in &output.results {
                if r.test_name == "compliance" {
                    if r.status == "fail" {
                        // Parse metadata for FN/FP
                        let meta: HashMap<String, String> = r
                            .metadata_json
                            .as_deref()
                            .and_then(|s| serde_json::from_str(s).ok())
                            .unwrap_or_default();
                        let fn_rules: Vec<String> = meta
                            .get("fn_rules")
                            .map(|s| {
                                s.split(',')
                                    .filter(|r| !r.is_empty())
                                    .map(String::from)
                                    .collect()
                            })
                            .unwrap_or_default();
                        let fp_rules: Vec<String> = meta
                            .get("fp_rules")
                            .map(|s| {
                                s.split(',')
                                    .filter(|r| !r.is_empty())
                                    .map(String::from)
                                    .collect()
                            })
                            .unwrap_or_default();
                        return RetestResult {
                            pdf_path: pdf_path.to_string(),
                            status: RetestStatus::StillFail,
                            fn_rules,
                            fp_rules,
                        };
                    }
                    // pass or skip = fixed
                    return RetestResult {
                        pdf_path: pdf_path.to_string(),
                        status: RetestStatus::Fixed,
                        fn_rules: vec![],
                        fp_rules: vec![],
                    };
                }
            }
            // No compliance result = skip = fixed
            RetestResult {
                pdf_path: pdf_path.to_string(),
                status: RetestStatus::Fixed,
                fn_rules: vec![],
                fp_rules: vec![],
            }
        }
        Err(_) => RetestResult {
            pdf_path: pdf_path.to_string(),
            status: RetestStatus::Error,
            fn_rules: vec![],
            fp_rules: vec![],
        },
    }
}
