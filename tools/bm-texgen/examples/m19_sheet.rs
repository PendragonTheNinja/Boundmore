//! `cargo run --release -p bm-texgen --example m19_sheet` writes the review
//! sheet of milestone 19's rocks, soils, turf, trunks and leaves to `docs/media/m19-materials.png`.

fn main() -> Result<(), String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/media/m19-materials.png");
    let sheet = bm_texgen::living::review_sheet()?;
    sheet
        .to_image()
        .save(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}
