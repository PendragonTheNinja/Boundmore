//! `cargo run --release -p bm-texgen` paints every block texture into
//! `assets/textures/`, and writes the contact sheet and preview into
//! `docs/media/`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bm_texgen::{families, overlay, preview, sheet, to_image};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn save(img: &image::RgbaImage, path: &Path) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    img.save(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn run() -> Result<(), String> {
    let root = repo_root();
    let blocks = root.join("assets/textures/blocks");
    let overlays = root.join("assets/textures/overlay");
    let media = root.join("docs/media");

    let fams = families()?;
    let mut count = 0;
    for fam in &fams {
        for (i, face) in fam.faces.iter().enumerate() {
            save(&to_image(face), &blocks.join(format!("{}.png", fam.file_stem(i))))?;
            count += 1;
        }
        let m = fam.base().mean().to_u8();
        println!(
            "{:<14} {} face(s), mean #{:02x}{:02x}{:02x} alpha {:.2}",
            fam.name,
            fam.faces.len(),
            m[0],
            m[1],
            m[2],
            m[3] as f32 / 255.0
        );
    }
    let cracks = overlay::cracks();
    for (i, crack) in cracks.iter().enumerate() {
        save(&to_image(crack), &overlays.join(format!("crack_{i}.png")))?;
    }
    save(
        &sheet::contact_sheet(&fams, &cracks).to_image(),
        &media.join("textures.png"),
    )?;
    save(&preview::preview(&fams).to_image(), &media.join("texture-preview.png"))?;
    println!(
        "wrote {count} block textures, {} crack stages, docs/media/textures.png, docs/media/texture-preview.png",
        cracks.len()
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bm-texgen: {e}");
            ExitCode::FAILURE
        }
    }
}
