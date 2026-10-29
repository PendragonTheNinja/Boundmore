//! `cargo run --release -p bm-soundgen` rewrites `assets/sounds/`, or
//! the folder given as the first argument.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/sounds"));
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("bm-soundgen: cannot make {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    let (mut files, mut bytes) = (0, 0);
    for s in bm_soundgen::sounds() {
        for n in 1..=s.variants {
            let data = bm_soundgen::dsp::wav_adpcm(&s.render(n));
            let path = dir.join(format!("{}_{n}.wav", s.name));
            if let Err(e) = std::fs::write(&path, &data) {
                eprintln!("bm-soundgen: cannot write {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
            files += 1;
            bytes += data.len();
        }
    }
    println!("wrote {files} sounds ({} KB) to {}", bytes / 1024, dir.display());
    ExitCode::SUCCESS
}
