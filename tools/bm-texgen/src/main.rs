//! `cargo run --release -p bm-texgen` paints every block texture and item
//! icon into `assets/textures/`, and writes the contact sheet, the icon
//! sheet and the preview into `docs/media/`.
//!
//! `cargo run --release -p bm-texgen -- --size 128 --out DIR [NAME...]`
//! paints only the named textures (the trial six by default) at that size
//! into `DIR/blocks/`, for the texture trial
//! (`docs/proposals/textures-128.md`).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bm_texgen::{families, item_icons, overlay, preview, sheet, to_image};

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
    let items = root.join("assets/textures/items");
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
    let icons = item_icons()?;
    for (name, icon) in &icons {
        save(&to_image(icon), &items.join(format!("{name}.png")))?;
    }
    save(
        &sheet::contact_sheet(&fams, &cracks, &icons).to_image(),
        &media.join("textures.png"),
    )?;
    save(
        &sheet::icon_sheet(&fams, &icons).to_image(),
        &media.join("item-icons.png"),
    )?;
    save(&preview::preview(&fams).to_image(), &media.join("texture-preview.png"))?;
    println!(
        "wrote {count} block textures, {} crack stages, {} item icons, docs/media/textures.png, \
         docs/media/item-icons.png, docs/media/texture-preview.png",
        cracks.len(),
        icons.len()
    );
    Ok(())
}

/// `--size N --out DIR [NAME...]`.
fn run_sized(args: &[String]) -> Result<(), String> {
    let mut size = None;
    let mut out = None;
    let mut names: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--size" => size = it.next().and_then(|v| v.parse::<usize>().ok()),
            "--out" => out = it.next().map(PathBuf::from),
            _ => names.push(a.clone()),
        }
    }
    let size = size
        .filter(|n| *n >= 32 && n.is_power_of_two())
        .ok_or("--size takes 32, 64, 128 or 256")?;
    let out = out.ok_or("--out DIR is needed with --size")?;
    let names: Vec<&str> = if names.is_empty() {
        bm_texgen::TRIAL.to_vec()
    } else {
        names.iter().map(String::as_str).collect()
    };
    let fams = bm_texgen::families_at(size, &names)?;
    for fam in &fams {
        for (i, face) in fam.faces.iter().enumerate() {
            save(
                &to_image(face),
                &out.join("blocks").join(format!("{}.png", fam.file_stem(i))),
            )?;
        }
        println!("{:<14} {} face(s) at {size}", fam.name, fam.faces.len());
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = if args.is_empty() { run() } else { run_sized(&args) };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bm-texgen: {e}");
            ExitCode::FAILURE
        }
    }
}
