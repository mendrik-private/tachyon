use std::{hint::black_box, time::Instant};

use document_core::{Document, EditCommand};
use document_view::{LayoutIndex, PreparedDocumentView, TextProjection};
use serde::Serialize;

#[derive(Serialize)]
struct Report {
    requested_bytes: usize,
    actual_bytes: usize,
    runs: usize,
    open_prepare_ms: Distribution,
    parse_ms: Distribution,
    projection_ms: Distribution,
    prepared_view_ms: Distribution,
    localized_edit_ms: Distribution,
    model_edit_ms: Distribution,
    view_edit_ms: Distribution,
    layout_ms: Distribution,
    cached_layout_ms: Distribution,
    viewport_lookup_us: Distribution,
    environment: Environment,
}

#[derive(Serialize)]
struct Distribution {
    min: f64,
    median: f64,
    p95: f64,
    p99: f64,
    max: f64,
}

#[derive(Serialize)]
struct Environment {
    rustc: String,
    wayland_display: Option<String>,
    desktop: Option<String>,
    session_type: Option<String>,
    power_profile: Option<String>,
}

fn main() {
    let (requested_bytes, runs) = arguments();
    let source = fixture(requested_bytes);
    let mut parse = Vec::with_capacity(runs);
    let mut open_prepare = Vec::with_capacity(runs);
    let mut projection = Vec::with_capacity(runs);
    let mut prepared_view = Vec::with_capacity(runs);
    let mut localized_edit = Vec::with_capacity(runs);
    let mut model_edit = Vec::with_capacity(runs);
    let mut view_edit = Vec::with_capacity(runs);
    let mut layout = Vec::with_capacity(runs);
    let mut cached_layout = Vec::with_capacity(runs);
    let mut viewport = Vec::with_capacity(runs);

    for _ in 0..runs {
        let started = Instant::now();
        let mut document =
            Document::from_markdown(source.clone()).expect("generated fixture parses");
        let parse_elapsed = started.elapsed().as_secs_f64() * 1_000.;
        parse.push(parse_elapsed);

        let snapshot = document.snapshot();
        let started = Instant::now();
        let projected = TextProjection::from_snapshot(&snapshot);
        black_box(projected.text().len());
        projection.push(started.elapsed().as_secs_f64() * 1_000.);

        let started = Instant::now();
        let mut prepared = PreparedDocumentView::prepare(&document);
        black_box(prepared.line_count());
        let prepared_elapsed = started.elapsed().as_secs_f64() * 1_000.;
        prepared_view.push(prepared_elapsed);
        open_prepare.push(parse_elapsed + prepared_elapsed);

        let snapshot = document.snapshot();
        let target = snapshot
            .blocks()
            .iter()
            .rev()
            .find(|block| block.text().is_some())
            .expect("fixture has editable text");
        let node_id = target.id();
        let offset = target.text().expect("editable target").len();
        let started = Instant::now();
        let result = document
            .apply(EditCommand::ReplaceText {
                node_id,
                range: offset..offset,
                text: "x".into(),
                selection_after: None,
                typing: true,
            })
            .expect("localized edit applies");
        model_edit.push(started.elapsed().as_secs_f64() * 1_000.);
        let view_started = Instant::now();
        assert!(prepared.refresh_text_node(&result.snapshot, node_id));
        view_edit.push(view_started.elapsed().as_secs_f64() * 1_000.);
        localized_edit.push(started.elapsed().as_secs_f64() * 1_000.);

        let started = Instant::now();
        let mut index = LayoutIndex::default();
        index
            .rebuild(&snapshot, 760., 1, 1.)
            .expect("generated heights are valid");
        layout.push(started.elapsed().as_secs_f64() * 1_000.);

        let started = Instant::now();
        let mut cached_index = index.fresh_with_shared_cache();
        cached_index
            .rebuild(&snapshot, 760., 1, 1.)
            .expect("cached heights are valid");
        cached_layout.push(started.elapsed().as_secs_f64() * 1_000.);
        index = cached_index;

        let samples = 10_000;
        let total_height = index.total_height().max(1.);
        let started = Instant::now();
        for sample in 0..samples {
            let y = total_height * sample as f32 / samples as f32;
            black_box(index.index_at_y(y));
        }
        viewport.push(started.elapsed().as_secs_f64() * 1_000_000. / samples as f64);
    }

    let report = Report {
        requested_bytes,
        actual_bytes: source.len(),
        runs,
        open_prepare_ms: distribution(open_prepare),
        parse_ms: distribution(parse),
        projection_ms: distribution(projection),
        prepared_view_ms: distribution(prepared_view),
        localized_edit_ms: distribution(localized_edit),
        model_edit_ms: distribution(model_edit),
        view_edit_ms: distribution(view_edit),
        layout_ms: distribution(layout),
        cached_layout_ms: distribution(cached_layout),
        viewport_lookup_us: distribution(viewport),
        environment: Environment {
            rustc: command_output("rustc", &["-V"]),
            wayland_display: std::env::var("WAYLAND_DISPLAY").ok(),
            desktop: std::env::var("XDG_CURRENT_DESKTOP").ok(),
            session_type: std::env::var("XDG_SESSION_TYPE").ok(),
            power_profile: std::fs::read_to_string("/sys/firmware/acpi/platform_profile")
                .ok()
                .map(|value| value.trim().to_owned()),
        },
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("report serializes")
    );
}

fn arguments() -> (usize, usize) {
    let mut bytes = 100 * 1024;
    let mut runs = 30;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--bytes" => {
                bytes = args
                    .next()
                    .expect("--bytes needs a value")
                    .parse()
                    .expect("--bytes must be an integer");
            }
            "--runs" => {
                runs = args
                    .next()
                    .expect("--runs needs a value")
                    .parse::<usize>()
                    .expect("--runs must be an integer")
                    .max(1);
            }
            "--help" | "-h" => {
                println!("Usage: tachyon-perf [--bytes N] [--runs N]");
                std::process::exit(0);
            }
            unknown => panic!("unknown argument: {unknown}"),
        }
    }
    (bytes, runs)
}

fn fixture(target_bytes: usize) -> String {
    let mut source = String::with_capacity(target_bytes + 1024);
    let mut section = 1;
    while source.len() < target_bytes {
        source.push_str(&format!(
            "## Section {section}\n\nTachyon measures **rich editing**, Unicode 🎉, [links](https://example.com), and source-preserving serialization across realistic prose.\n\n- first item\n- second item\n\n| Name | Value |\n| --- | ---: |\n| frame | 6 ms |\n\n"
        ));
        section += 1;
    }
    source.truncate(source.floor_char_boundary(target_bytes));
    source
}

fn distribution(mut samples: Vec<f64>) -> Distribution {
    samples.sort_by(f64::total_cmp);
    Distribution {
        min: samples[0],
        median: percentile(&samples, 0.5),
        p95: percentile(&samples, 0.95),
        p99: percentile(&samples, 0.99),
        max: *samples.last().expect("non-empty samples"),
    }
}

fn percentile(samples: &[f64], percentile: f64) -> f64 {
    let index = ((samples.len() - 1) as f64 * percentile).ceil() as usize;
    samples[index]
}

fn command_output(program: &str, args: &[&str]) -> String {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unavailable".into())
}
