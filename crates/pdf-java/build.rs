// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

fn main() {
    // On Apple targets ld64 stamps a dylib's LC_ID_DYLIB with the output
    // path, so the shipped library named the builder's target directory
    // (/Volumes/<runner>/.../target/release/deps/libpdfluent_java.dylib). --remap-path-prefix
    // never reaches the linker; a fixed @rpath name does, on every build
    // route (cargo, maturin, napi). Held by scripts/release/binary_host_paths.py
    // in the release gates.
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_VENDOR").as_deref() == Ok("apple") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libpdfluent_java.dylib");
    }
}
