//! Builds `assets/makehuman/makehuman-cc0.bin` from a MakeHuman checkout.
//!
//! ```text
//! tools/mhpack/fetch.sh target/makehuman          # sparse clone of the CC0 assets
//! cargo run -p azam-mhpack --release -- \
//!     --makehuman target/makehuman --out assets/makehuman/makehuman-cc0.bin --check
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use azam_avatar::{AvatarParams, MEASUREMENTS, MhModel, Sex};
use azam_mhpack::{BuildOptions, build_pack, pack::DEFAULT_QUANTUM_MM};

const USAGE: &str = "usage: azam-mhpack --makehuman <checkout or makehuman/data dir> --out <pack> \
                     [--source <url@commit>] [--check] [--verbose]";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (mut input, mut out, mut source) = (None, None, String::new());
    let (mut check, mut verbose) = (false, false);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--makehuman" => input = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            "--source" => source = args.next().unwrap_or_default(),
            "--check" => check = true,
            "--verbose" => verbose = true,
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }
    let (Some(input), Some(out)) = (input, out) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let data_dir = if input.join("makehuman/data").is_dir() {
        input.join("makehuman/data")
    } else {
        input
    };
    let built = match build_pack(&BuildOptions {
        data_dir,
        source,
        quantum: DEFAULT_QUANTUM_MM,
    }) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    if verbose {
        for n in &built.notes {
            println!("{n}");
        }
    } else if let Some(last) = built.notes.last() {
        println!("{last}");
    }
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&out, &built.bytes) {
        eprintln!("error: {}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    println!("wrote {}", out.display());
    if check {
        return run_check(&built.bytes);
    }
    ExitCode::SUCCESS
}

/// Parses the pack and fits both presets, printing wanted and measured values.
fn run_check(bytes: &[u8]) -> ExitCode {
    let model = match MhModel::parse(bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: the pack does not parse: {e}");
            return ExitCode::FAILURE;
        }
    };
    let missing = model.missing_fit_targets();
    if !missing.is_empty() {
        eprintln!("error: missing fit targets {missing:?}");
        return ExitCode::FAILURE;
    }
    let base = model.measure_all(&model.base);
    println!("probe vertices: {}", model.probe_vertex_count());
    for sex in [Sex::Female, Sex::Male] {
        let params = AvatarParams::preset(sex);
        let t = std::time::Instant::now();
        let body = model.build_body(&params);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let r = &body.report;
        println!(
            "{sex:?}: fitted in {ms:.0} ms, scale {:.4}, weight {:.3}",
            r.fit.scale, r.fit.shape.weight
        );
        let mut worst: f64 = 0.0;
        for (i, d) in MEASUREMENTS.iter().enumerate() {
            let m = r.measured.0[i];
            let want = r.fit.wanted[i];
            let dev = want.map(|w| m - w);
            if let Some(dv) = dev {
                worst = worst.max(dv.abs());
            }
            println!(
                "  {:<12} base {:>7.1}  measured {:>7.1}  wanted {:>7}  {}",
                d.key,
                base.0[i],
                m,
                want.map_or("auto".to_owned(), |w| format!("{w:.1}")),
                dev.map_or(String::new(), |d| format!("dev {d:+.2}"))
            );
        }
        println!(
            "  weights {:?}",
            r.fit.shape.measures.map(|w| (w * 100.0).round() / 100.0)
        );
        println!("  largest deviation {worst:.2} mm");
    }
    ExitCode::SUCCESS
}
