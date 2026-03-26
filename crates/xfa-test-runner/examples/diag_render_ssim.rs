use pdf_engine::{PdfDocument, RenderOptions};

fn render_page_to_png(path: &str, page: usize, out: &str) {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("read {path}: {err}");
            return;
        }
    };
    let doc = match PdfDocument::open(data) {
        Ok(doc) => doc,
        Err(err) => {
            eprintln!("open {path}: {err}");
            return;
        }
    };
    let render = match doc.render_page(
        page,
        &RenderOptions {
            dpi: 150.0,
            ..Default::default()
        },
    ) {
        Ok(render) => render,
        Err(err) => {
            eprintln!("render {path} page {page}: {err}");
            return;
        }
    };

    let image =
        image::RgbaImage::from_raw(render.width, render.height, render.pixels).expect("rgba");
    image.save(out).expect("save");
    println!("OK  {out}  ({}x{})", render.width, render.height);
}

#[allow(dead_code)]
fn render_all_pages(path: &str, prefix: &str) {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("read {path}: {err}");
            return;
        }
    };
    let doc = match PdfDocument::open(data) {
        Ok(doc) => doc,
        Err(err) => {
            eprintln!("open {path}: {err}");
            return;
        }
    };

    for page in 0..doc.page_count().min(3) {
        render_page_to_png(path, page, &format!("/tmp/{}_{}.png", prefix, page + 1));
    }
}

fn debug_page(path: &str, page: usize) {
    let data = std::fs::read(path).expect("read");
    let doc = PdfDocument::open(data).expect("open");
    let pages = doc.pdf().pages();
    let page = &pages[page];

    match page.page_stream() {
        None => println!("page_stream() = None"),
        Some(stream) => {
            println!("page_stream() = {} bytes", stream.len());
            println!(
                "page_stream preview: {:?}",
                std::str::from_utf8(&stream[..80.min(stream.len())])
            );
        }
    }

    let ops: Vec<_> = page.typed_operations().collect();
    println!("typed_operations count: {}", ops.len());
}

fn main() {
    debug_page("/tmp/gen-661_661518.pdf", 0);
    render_page_to_png("/tmp/gen-661_661518.pdf", 0, "/tmp/gen661_p1_ours.png");
    render_page_to_png("/tmp/gen-661_661518.pdf", 1, "/tmp/gen661_p2_ours.png");
}
