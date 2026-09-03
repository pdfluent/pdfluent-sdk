// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

fn main() -> visual_regression::Result<()> {
    for fixture in visual_regression::fixtures::generate()? {
        std::fs::write(
            visual_regression::root()
                .join("fixtures")
                .join(format!("{}.pdf", fixture.name)),
            &fixture.bytes,
        )?;
        println!("| {} | {} |", fixture.name, fixture.features);
    }
    Ok(())
}
