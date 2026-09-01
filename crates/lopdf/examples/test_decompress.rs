use std::collections::HashSet;
use std::env;
use std::fs;

fn main() {
    let path = env::args().nth(1).expect("usage: test_decompress <pdf>");
    let data = fs::read(&path).expect("read");
    let doc = pdfluent_lopdf::Document::load_mem(&data).expect("load");

    let mut success = 0;
    let mut fail = 0;
    let mut filters_seen = HashSet::new();

    for (&id, obj) in &doc.objects {
        // `let ... else` rather than nested `if let`: this workspace is edition
        // 2021, so the let-chain that clippy's suggestion implies is not
        // available here.
        let pdfluent_lopdf::Object::Stream(ref stream) = *obj else {
            continue;
        };
        let Ok(filter_list) = stream.filters() else {
            continue;
        };
        for f in &filter_list {
            let name = String::from_utf8_lossy(f).to_string();
            filters_seen.insert(name);
        }
        let mut s = stream.clone();
        match s.decompress() {
            Ok(_) => success += 1,
            Err(e) => {
                let filter_names: Vec<_> = filter_list
                    .iter()
                    .map(|f| String::from_utf8_lossy(f).to_string())
                    .collect();
                eprintln!("  FAIL {:?}: {} (filters: {:?})", id, e, filter_names);
                fail += 1;
            }
        }
    }

    println!("{}: {} ok, {} fail. Filters: {:?}", path, success, fail, filters_seen);
}
