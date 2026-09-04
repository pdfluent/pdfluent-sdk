// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use criterion::{criterion_group, BenchmarkId, Criterion};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const MIB: u64 = 1024 * 1024;
const STANDARD_STRESS_LIMIT_BYTES: u64 = 150 * MIB;

#[derive(Clone)]
struct StressFixture {
    name: String,
    path: PathBuf,
    size_bytes: u64,
}

#[derive(Clone, Copy)]
enum StressParser {
    LopdfLoadMem,
    PdfSyntaxNew,
}

#[derive(Clone, Copy)]
struct StressMeasurement {
    elapsed: Duration,
    peak_rss_kb: u64,
}

impl StressParser {
    fn id(self) -> &'static str {
        match self {
            StressParser::LopdfLoadMem => "lopdf_load_mem",
            StressParser::PdfSyntaxNew => "pdf_syntax_new",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id {
            "lopdf_load_mem" => Some(StressParser::LopdfLoadMem),
            "pdf_syntax_new" => Some(StressParser::PdfSyntaxNew),
            _ => None,
        }
    }
}

const STRESS_PARSERS: [StressParser; 2] = [StressParser::LopdfLoadMem, StressParser::PdfSyntaxNew];

fn corpus_dir() -> PathBuf {
    repo_root().join("corpus")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn stress_fixtures_dir() -> PathBuf {
    std::env::var_os("PDF_STRESS_FIXTURES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root().join("benchmarks/stress-fixtures"))
}

fn load_corpus_samples() -> Vec<(String, Vec<u8>)> {
    let corpus = corpus_dir();
    if !corpus.exists() {
        return Vec::new();
    }

    let mut pdfs: Vec<(String, Vec<u8>)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&corpus) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "pdf") {
                if let Ok(data) = std::fs::read(&path) {
                    let name = path.file_stem().unwrap().to_string_lossy().to_string();
                    pdfs.push((name, data));
                }
            }
        }
    }
    pdfs.sort_by(|a, b| a.1.len().cmp(&b.1.len()));
    pdfs
}

fn load_stress_fixtures(allow_large: bool) -> Vec<StressFixture> {
    let dir = stress_fixtures_dir();
    if !dir.exists() {
        eprintln!(
            "stress: no fixtures found in {}; generate one with `python3 scripts/generate_stress_fixtures.py --size 100M`",
            dir.display()
        );
        return Vec::new();
    }

    let mut fixtures = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "pdf") {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let size_bytes = metadata.len();
            if !allow_large && size_bytes > STANDARD_STRESS_LIMIT_BYTES {
                eprintln!(
                    "stress: skipping {} ({} MiB); set BENCHMARK_STRESS_LARGE=1 for nightly-size fixtures",
                    path.display(),
                    size_bytes / MIB
                );
                continue;
            }
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            fixtures.push(StressFixture {
                name,
                path,
                size_bytes,
            });
        }
    }
    fixtures.sort_by(|a, b| a.size_bytes.cmp(&b.size_bytes));
    fixtures
}

fn pick_samples(pdfs: &[(String, Vec<u8>)]) -> Vec<&(String, Vec<u8>)> {
    if pdfs.len() >= 3 {
        vec![&pdfs[0], &pdfs[pdfs.len() / 2], &pdfs[pdfs.len() - 1]]
    } else {
        pdfs.iter().collect()
    }
}

fn bench_lopdf_parse(c: &mut Criterion) {
    let pdfs = load_corpus_samples();
    if pdfs.is_empty() {
        return;
    }
    let samples = pick_samples(&pdfs);

    let mut group = c.benchmark_group("lopdf_parse");
    for (name, data) in &samples {
        group.bench_with_input(
            BenchmarkId::new("load_mem", format!("{} ({}KB)", name, data.len() / 1024)),
            data,
            |b, data| {
                b.iter(|| {
                    let _ = lopdf::Document::load_mem(data);
                });
            },
        );
    }
    group.finish();
}

fn bench_pdf_syntax_parse(c: &mut Criterion) {
    let pdfs = load_corpus_samples();
    if pdfs.is_empty() {
        return;
    }
    let samples = pick_samples(&pdfs);

    let mut group = c.benchmark_group("pdf_syntax_parse");
    for (name, data) in &samples {
        group.bench_with_input(
            BenchmarkId::new("parse", format!("{} ({}KB)", name, data.len() / 1024)),
            data,
            |b, data| {
                b.iter(|| {
                    let _ = pdf_syntax::Pdf::new(data.clone());
                });
            },
        );
    }
    group.finish();

    let mut group = c.benchmark_group("pdf_syntax_pages");
    for (name, data) in &samples {
        if let Ok(pdf) = pdf_syntax::Pdf::new(data.clone()) {
            group.bench_with_input(BenchmarkId::new("iterate_pages", name), &pdf, |b, pdf| {
                b.iter(|| {
                    let pages = pdf.pages();
                    for page in pages.iter() {
                        for _op in page.typed_operations() {}
                    }
                });
            });
        }
    }
    group.finish();
}

fn bench_xfa_extract(c: &mut Criterion) {
    let pdfs = load_corpus_samples();
    if pdfs.is_empty() {
        return;
    }

    let mut xfa_pdfs: Vec<&(String, Vec<u8>)> = Vec::new();
    for pdf in &pdfs {
        if pdf_xfa::extract::extract_xfa_from_bytes(pdf.1.clone()).is_ok() {
            xfa_pdfs.push(pdf);
            if xfa_pdfs.len() >= 5 {
                break;
            }
        }
    }

    let mut group = c.benchmark_group("xfa_extract");
    for (name, data) in &xfa_pdfs {
        group.bench_with_input(BenchmarkId::new("scan_xfa", name), data, |b, data| {
            b.iter(|| {
                let _ = pdf_xfa::extract::extract_xfa_from_bytes(data.to_vec());
            });
        });
    }
    group.finish();
}

fn bench_stress_parse(c: &mut Criterion) {
    if !criterion_stress_enabled() {
        return;
    }

    let fixtures = load_stress_fixtures(allow_large_stress_fixtures());
    if fixtures.is_empty() {
        return;
    }

    let mut group = c.benchmark_group("stress");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(10));

    for fixture in fixtures {
        for parser in STRESS_PARSERS {
            group.bench_with_input(
                BenchmarkId::new(
                    parser.id(),
                    format!("{} ({}MiB)", fixture.name, fixture.size_bytes / MIB),
                ),
                &fixture,
                |b, fixture| {
                    b.iter_custom(|iters| {
                        let mut total = Duration::ZERO;
                        let mut peak = StressMeasurement {
                            elapsed: Duration::ZERO,
                            peak_rss_kb: 0,
                        };
                        for _ in 0..iters {
                            let measurement = run_stress_worker(parser, &fixture.path)
                                .unwrap_or_else(|err| panic!("stress worker failed: {err}"));
                            total += measurement.elapsed;
                            if measurement.elapsed > peak.elapsed {
                                peak.elapsed = measurement.elapsed;
                            }
                            peak.peak_rss_kb = peak.peak_rss_kb.max(measurement.peak_rss_kb);
                        }
                        eprintln!(
                            "stress/{}/{}: iters={} peak-time-ms={:.3} peak-RSS-kb={} peak-RSS-MiB={:.1}",
                            parser.id(),
                            fixture.name,
                            iters,
                            peak.elapsed.as_secs_f64() * 1000.0,
                            peak.peak_rss_kb,
                            peak.peak_rss_kb as f64 / 1024.0
                        );
                        total
                    });
                },
            );
        }
    }
    group.finish();
}

fn bench_formcalc(c: &mut Criterion) {
    let scripts = [
        ("arithmetic", "1 + 2 * 3 - 4 / 2"),
        ("string_ops", "Concat(\"hello\", \" \", \"world\")"),
        ("conditional", "if (1 > 0) then \"yes\" else \"no\" endif"),
        (
            "loop_100",
            "var x = 0\nfor i = 1 upto 100 do\nx = x + i\nendfor\nx",
        ),
        (
            "string_heavy",
            "var s = \"\"\nfor i = 1 upto 50 do\ns = Concat(s, \"a\")\nendfor\nLen(s)",
        ),
    ];

    let mut group = c.benchmark_group("formcalc");
    for (name, script) in &scripts {
        group.bench_with_input(BenchmarkId::new("eval", name), script, |b, script| {
            b.iter(|| {
                let tokens = formcalc_interpreter::lexer::tokenize(script).unwrap();
                let ast = formcalc_interpreter::parser::parse(tokens).unwrap();
                let mut interp = formcalc_interpreter::interpreter::Interpreter::new();
                let _ = interp.exec(&ast);
            });
        });
    }
    group.finish();
}

fn bench_data_dom(c: &mut Criterion) {
    let small_xml = r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
        <xfa:data><form><field1>value1</field1><field2>value2</field2></form></xfa:data>
    </xfa:datasets>"#;

    let mut large_xml = String::from(
        r#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><form>"#,
    );
    for i in 0..500 {
        large_xml.push_str(&format!("<field{i}>value{i}</field{i}>"));
    }
    large_xml.push_str("</form></xfa:data></xfa:datasets>");

    let mut group = c.benchmark_group("data_dom");
    group.bench_function("parse_small", |b| {
        b.iter(|| {
            let _ = xfa_dom_resolver::data_dom::DataDom::from_xml(small_xml);
        });
    });
    group.bench_function("parse_large_500_fields", |b| {
        b.iter(|| {
            let _ = xfa_dom_resolver::data_dom::DataDom::from_xml(&large_xml);
        });
    });
    group.bench_function("to_xml_roundtrip", |b| {
        let dom = xfa_dom_resolver::data_dom::DataDom::from_xml(&large_xml).unwrap();
        b.iter(|| {
            let _ = dom.to_xml();
        });
    });
    group.finish();
}

fn criterion_stress_enabled() -> bool {
    matches!(
        std::env::var("BENCHMARK_STRESS").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES") | Ok("all") | Ok("nightly")
    )
}

fn allow_large_stress_fixtures() -> bool {
    matches!(
        std::env::var("BENCHMARK_STRESS_LARGE").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    ) || matches!(
        std::env::var("BENCHMARK_STRESS").as_deref(),
        Ok("all") | Ok("nightly")
    )
}

fn run_stress_worker(parser: StressParser, path: &Path) -> Result<StressMeasurement, String> {
    let output = Command::new(std::env::current_exe().map_err(|err| err.to_string())?)
        .arg("--stress-worker")
        .arg(parser.id())
        .arg(path)
        .output()
        .map_err(|err| format!("spawn {} failed: {err}", parser.id()))?;

    if !output.status.success() {
        return Err(format!(
            "{} failed with status {:?}: {}",
            parser.id(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    parse_worker_measurement(&String::from_utf8_lossy(&output.stdout))
}

fn parse_worker_measurement(stdout: &str) -> Result<StressMeasurement, String> {
    let mut elapsed_us = None;
    let mut peak_rss_kb = None;
    for part in stdout.split_whitespace() {
        if let Some(value) = part.strip_prefix("elapsed_us=") {
            elapsed_us = Some(
                value
                    .parse::<u64>()
                    .map_err(|err| format!("invalid elapsed_us in worker output: {err}"))?,
            );
        } else if let Some(value) = part.strip_prefix("peak_rss_kb=") {
            peak_rss_kb = Some(
                value
                    .parse::<u64>()
                    .map_err(|err| format!("invalid peak_rss_kb in worker output: {err}"))?,
            );
        }
    }

    let elapsed_us =
        elapsed_us.ok_or_else(|| format!("missing elapsed_us in worker output: {stdout}"))?;
    let peak_rss_kb =
        peak_rss_kb.ok_or_else(|| format!("missing peak_rss_kb in worker output: {stdout}"))?;
    Ok(StressMeasurement {
        elapsed: Duration::from_micros(elapsed_us),
        peak_rss_kb,
    })
}

fn maybe_run_stress_worker(args: &[OsString]) -> Option<Result<(), String>> {
    if args.get(1).and_then(|arg| arg.to_str()) != Some("--stress-worker") {
        return None;
    }
    Some(run_stress_worker_cli(args))
}

fn run_stress_worker_cli(args: &[OsString]) -> Result<(), String> {
    let parser_id = args
        .get(2)
        .and_then(|arg| arg.to_str())
        .ok_or_else(|| "missing stress parser id".to_string())?;
    let path = args
        .get(3)
        .map(PathBuf::from)
        .ok_or_else(|| "missing stress fixture path".to_string())?;

    let parser = StressParser::from_id(parser_id)
        .ok_or_else(|| format!("unknown stress parser id: {parser_id}"))?;
    let measurement = run_stress_worker_once(parser, &path)?;
    println!(
        "elapsed_us={} peak_rss_kb={}",
        measurement.elapsed.as_micros(),
        measurement.peak_rss_kb
    );
    Ok(())
}

fn run_stress_worker_once(parser: StressParser, path: &Path) -> Result<StressMeasurement, String> {
    let start = Instant::now();
    let data =
        std::fs::read(path).map_err(|err| format!("read {} failed: {err}", path.display()))?;
    match parser {
        StressParser::LopdfLoadMem => {
            let _doc = lopdf::Document::load_mem(&data)
                .map_err(|err| format!("lopdf load_mem {} failed: {err}", path.display()))?;
        }
        StressParser::PdfSyntaxNew => {
            let _pdf = pdf_syntax::Pdf::new(data)
                .map_err(|err| format!("pdf_syntax new {} failed: {err:?}", path.display()))?;
        }
    }
    let elapsed = start.elapsed();
    Ok(StressMeasurement {
        elapsed,
        peak_rss_kb: peak_rss_kb().unwrap_or(0),
    })
}

fn peak_rss_kb() -> Option<u64> {
    proc_status_peak_rss_kb().or_else(rusage_peak_rss_kb)
}

fn proc_status_peak_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            return rest.split_whitespace().next()?.parse::<u64>().ok();
        }
    }
    None
}

fn rusage_peak_rss_kb() -> Option<u64> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    let raw = unsafe { usage.assume_init() }.ru_maxrss as u64;
    #[cfg(target_os = "macos")]
    {
        Some(raw.div_ceil(1024))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(raw)
    }
}

fn maybe_run_stress_cli(args: &[OsString]) -> Option<Result<(), String>> {
    let stress_filter = args.iter().skip(1).any(|arg| {
        arg.to_str()
            .is_some_and(|arg| arg == "stress" || arg.starts_with("stress/"))
    });
    if !stress_filter {
        return None;
    }
    Some(run_stress_cli(args))
}

fn run_stress_cli(args: &[OsString]) -> Result<(), String> {
    let sample_size = parse_stress_sample_size(args).unwrap_or(3);
    if sample_size == 0 {
        return Err("--sample-size must be >= 1 for stress benchmarks".to_string());
    }

    let fixtures = load_stress_fixtures(allow_large_stress_fixtures());
    if fixtures.is_empty() {
        return Err(format!(
            "no stress fixtures found in {}; run `python3 scripts/generate_stress_fixtures.py --size 100M` first",
            stress_fixtures_dir().display()
        ));
    }

    println!(
        "stress benchmark: fixtures={} sample-size={} parsers={}",
        stress_fixtures_dir().display(),
        sample_size,
        STRESS_PARSERS
            .iter()
            .map(|parser| parser.id())
            .collect::<Vec<_>>()
            .join(",")
    );

    for fixture in fixtures {
        for parser in STRESS_PARSERS {
            let mut runs = Vec::with_capacity(sample_size);
            for index in 0..sample_size {
                let measurement = run_stress_worker(parser, &fixture.path)?;
                println!(
                    "stress/{}/{} run={}/{} time-ms={:.3} peak-RSS-kb={} peak-RSS-MiB={:.1}",
                    parser.id(),
                    fixture.name,
                    index + 1,
                    sample_size,
                    measurement.elapsed.as_secs_f64() * 1000.0,
                    measurement.peak_rss_kb,
                    measurement.peak_rss_kb as f64 / 1024.0
                );
                runs.push(measurement);
            }

            let total = runs
                .iter()
                .fold(Duration::ZERO, |acc, run| acc + run.elapsed);
            let mean = total.as_secs_f64() * 1000.0 / runs.len() as f64;
            let peak_time = runs.iter().map(|run| run.elapsed).max().unwrap_or_default();
            let peak_rss_kb = runs.iter().map(|run| run.peak_rss_kb).max().unwrap_or(0);
            println!(
                "stress/{}/{} summary runs={} mean-time-ms={:.3} peak-time-ms={:.3} peak-RSS-kb={} peak-RSS-MiB={:.1}",
                parser.id(),
                fixture.name,
                runs.len(),
                mean,
                peak_time.as_secs_f64() * 1000.0,
                peak_rss_kb,
                peak_rss_kb as f64 / 1024.0
            );
        }
    }

    Ok(())
}

fn parse_stress_sample_size(args: &[OsString]) -> Option<usize> {
    for (index, arg) in args.iter().enumerate() {
        let arg = arg.to_str()?;
        if arg == "--sample-size" {
            return args.get(index + 1)?.to_str()?.parse::<usize>().ok();
        }
        if let Some(value) = arg.strip_prefix("--sample-size=") {
            return value.parse::<usize>().ok();
        }
    }
    None
}

criterion_group!(
    benches,
    bench_lopdf_parse,
    bench_pdf_syntax_parse,
    bench_xfa_extract,
    bench_stress_parse,
    bench_formcalc,
    bench_data_dom,
);

fn main() {
    let args = std::env::args_os().collect::<Vec<_>>();
    if let Some(result) = maybe_run_stress_worker(&args).or_else(|| maybe_run_stress_cli(&args)) {
        if let Err(err) = result {
            eprintln!("stress benchmark failed: {err}");
            std::process::exit(1);
        }
        return;
    }
    benches();
}
