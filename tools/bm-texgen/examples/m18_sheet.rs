//! `cargo run --release -p bm-texgen --example m18_sheet` writes the review
//! sheet of milestone 18's seams and icons to `docs/media/m18-icons.png`.

fn main() -> Result<(), String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/media/m18-icons.png");
    let sheet = bm_texgen::worlds::review_sheet()?;
    sheet
        .to_image()
        .save(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}
