//! `cargo run --release -p bm-texgen --example m18_part2_sheet` writes the review
//! sheet of milestone 18 part 2's blocks and icons to `docs/media/m18-part2-icons.png`.

fn main() -> Result<(), String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/media/m18-part2-icons.png");
    let sheet = bm_texgen::gems::review_sheet()?;
    sheet
        .to_image()
        .save(&path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}
