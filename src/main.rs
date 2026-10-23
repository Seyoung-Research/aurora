//! Aurora is a data visualization and statistical analysis program written entirely in Rust.
//!
//! Run without arguments to open the desktop application. `--selftest` builds
//! the demo project, runs every analysis and renders every graph without a
//! window, which is handy on headless machines (and writes PNGs when
//! `--write-samples <dir>` or `GP_WRITE_SAMPLES=<dir>` is given).
//!
//! ## Command line
//!
//! - `aurora` — open the desktop app.
//! - `aurora --open FILE.json` — open a saved project on start.
//! - `aurora --selftest` — run the demo project headlessly.
//! - `aurora --selftest --write-samples DIR` — also export every demo graph as PNG.
//! - `aurora --open FILE.json --tab graph` — start on a given tab (`data`,
//!   `results` or `graph`).
//! - `aurora -h | --help` — print the usage text.

// The stats/plot/data/analysis layers expose a broader API than the GUI currently
// exercises (extra distributions, quantiles, chart helpers, table utilities).
// Those items are covered by unit tests, so the dead-code lint is turned off
// for the engine modules only — the UI code stays strictly checked.
#[allow(dead_code)]
mod analysis;
#[allow(dead_code)]
mod data;
#[allow(dead_code)]
mod plot;
#[allow(dead_code)]
mod stats;
mod ui;

use anyhow::Result;

use data::persistence::{load_project, save_project};

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return Ok(());
    }
    let sample_dir = args
        .iter()
        .position(|a| a == "--write-samples")
        .and_then(|i| args.get(i + 1).cloned())
        .or_else(|| std::env::var("GP_WRITE_SAMPLES").ok());
    if args.iter().any(|a| a == "--selftest") {
        return selftest(sample_dir.as_deref());
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1320.0, 880.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("Aurora"),
        ..Default::default()
    };
    eframe::run_native(
        "Aurora",
        options,
        Box::new(|cc| Box::new(ui::App::new(cc)) as Box<dyn eframe::App>),
    )
    .map_err(|e| anyhow::anyhow!("could not start the GUI: {e}"))?;
    Ok(())
}

/// Print the command-line usage text to stdout.
fn print_usage() {
    println!(
        "Aurora — statistics, curve fitting and graphing\n\
         \n\
         USAGE\n\
         \x20 aurora                                 open the desktop app\n\
         \x20 aurora --open FILE.json                open a saved project on start\n\
         \x20 aurora --selftest                      run the demo project headlessly\n\
         \x20 aurora --selftest --write-samples DIR\n\
         \x20                                      also export every demo graph as PNG\n\
         \x20 aurora --open FILE.json --tab graph\n\
         \x20                                      start on a given tab (data|results|graph)\n\
         \x20 aurora -h | --help                     this text\n\
         \n\
         In the app: use the File menu to load the demo project, then the Data,\n\
         Results and Graph tabs to edit tables, run analyses and draw graphs."
    );
}

/// Count pixels that are not near-white, as a cheap "did anything draw?" test.
fn count_ink(img: &plot::render::ChartImage) -> usize {
    img.rgb
        .chunks_exact(3)
        .filter(|p| p[0] < 250 || p[1] < 250 || p[2] < 250)
        .count()
}

/// Build the demo project, run everything and render everything. Exits non-zero
/// when any step fails.
///
/// Steps: list tables, run every analysis, render every graph (optionally
/// writing PNGs to `sample_dir`), and do a save/load round trip.
///
/// # Errors
///
/// Returns an error if the demo project cannot be built, rendered or saved.
fn selftest(sample_dir: Option<&str>) -> Result<()> {
    let mut project = ui::demo::demo_project();
    let mut failures: Vec<String> = Vec::new();

    if let Some(dir) = sample_dir {
        std::fs::create_dir_all(dir)?;
    }

    println!("Aurora self-test");
    println!("  tables: {}", project.tables.len());
    for t in &project.tables {
        println!("    [{}] {}", t.flavor().label(), t.as_table().name());
    }

    println!("  analyses: {}", project.analyses.len());
    for i in 0..project.analyses.len() {
        let _ = project.run_analysis(i);
        let a = &project.analyses[i];
        match (&a.result, &a.error) {
            (Some(r), _) => println!("    ok    {} -> {}", a.spec.label(), r.headline()),
            (None, e) => failures.push(format!(
                "{} did not run: {}",
                a.spec.label(),
                e.clone().unwrap_or_else(|| "no result".into())
            )),
        }
    }

    println!("  graphs: {}", project.graphs.len());
    for i in 0..project.graphs.len() {
        let name = project.graphs[i].name.clone();
        let (w, h) = (project.graphs[i].width, project.graphs[i].height);
        match project.chart_for_graph(i) {
            Some(chart) => match plot::render::render(&chart, w, h) {
                Ok(img) => {
                    let ink = count_ink(&img);
                    println!(
                        "    ok    '{name}' {w}x{h}, {} series, {ink} non-white px",
                        chart.series.len()
                    );
                    if ink < 200 {
                        failures.push(format!("graph '{name}' rendered (almost) blank"));
                    }
                    if let Some(dir) = sample_dir {
                        let path = format!("{}/{}.png", dir.trim_end_matches(['/', '\\']), ui::slug(&name));
                        plot::render::save_png(&chart, &path, w, h)?;
                        println!("          wrote {path}");
                    }
                }
                Err(e) => failures.push(format!("graph '{name}' failed to render: {e}")),
            },
            None => failures.push(format!("graph '{name}' has no renderable data")),
        }
    }

    // Persistence round trip.
    let tmp = std::env::temp_dir().join("aurora_selftest.json");
    let tmp = tmp.to_string_lossy().to_string();
    save_project(&project, &tmp)?;
    let loaded = load_project(&tmp)?;
    if loaded.tables.len() != project.tables.len() || loaded.graphs.len() != project.graphs.len() {
        failures.push("save/load round trip lost data".into());
    } else {
        println!("    ok    persistence round trip via {tmp}");
    }
    if let Some(dir) = sample_dir {
        // A ready-made project file to open in the app.
        let path = format!(
            "{}/demo_project.json",
            dir.trim_end_matches(['/', '\\'])
        );
        save_project(&project, &path)?;
        println!("    ok    demo project written to {path}");
    }

    println!();
    if failures.is_empty() {
        println!("All self-test checks passed.");
        Ok(())
    } else {
        for f in &failures {
            println!("  FAIL {f}");
        }
        anyhow::bail!("{} self-test check(s) failed", failures.len())
    }
}
