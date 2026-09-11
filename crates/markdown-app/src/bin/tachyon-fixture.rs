use std::{env, fmt::Write as _, fs, path::PathBuf};

fn main() {
    let (target_bytes, output, image_source) = arguments();
    let source = fixture(target_bytes, &image_source);
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).expect("fixture directory can be created");
    }
    fs::write(&output, source.as_bytes()).expect("fixture can be written");
    eprintln!("wrote {} bytes to {}", source.len(), output.display());
}

fn arguments() -> (usize, PathBuf, String) {
    let mut bytes = None;
    let mut output = None;
    let mut image_source = "../visual-assets/tachyon-strata.svg".to_owned();
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--bytes" => {
                bytes = Some(
                    args.next()
                        .expect("--bytes needs a value")
                        .parse::<usize>()
                        .expect("--bytes must be an integer"),
                );
            }
            "--output" => {
                output = Some(PathBuf::from(args.next().expect("--output needs a value")));
            }
            "--image-source" => {
                image_source = args.next().expect("--image-source needs a value");
            }
            "--help" | "-h" => {
                println!(
                    "Usage: tachyon-fixture --bytes N --output PATH [--image-source MARKDOWN_PATH]"
                );
                std::process::exit(0);
            }
            unknown => panic!("unknown argument: {unknown}"),
        }
    }
    (
        bytes.expect("--bytes is required").max(32 * 1024),
        output.expect("--output is required"),
        image_source,
    )
}

fn fixture(target_bytes: usize, image_source: &str) -> String {
    let mut source = String::with_capacity(target_bytes);
    source.push_str(
        "---\ntitle: Tachyon release qualification\nfixture: deterministic-rich-v1\n---\n\n# Tachyon release qualification\n\nThis document combines image-heavy sections, large tables, deep lists, Unicode 你好 مرحبا 🎉, and long code lines. It is intentionally editable and autosaved during compositor tests.\n\n",
    );
    let mut section = 1;
    while source.len() + 12_000 < target_bytes {
        append_section(&mut source, section, image_source);
        section += 1;
    }
    if source.len() < target_bytes {
        let remaining = target_bytes - source.len();
        if remaining >= 18 {
            source.push_str("\n<!-- padding:");
            let suffix = "-->\n";
            source.extend(std::iter::repeat_n(
                'x',
                target_bytes - source.len() - suffix.len(),
            ));
            source.push_str(suffix);
        } else {
            source.extend(std::iter::repeat_n('x', remaining));
        }
    }
    debug_assert_eq!(source.len(), target_bytes);
    source
}

fn append_section(source: &mut String, section: usize, image_source: &str) {
    writeln!(source, "## Field section {section}\n").expect("string write");
    source.push_str(
        "Tachyon measures **rich editing**, *selection*, ~~strikethrough~~, `inline code`, and [links](https://example.com) while keeping raw source stable. Combining marks: e\u{301}; bidirectional text: العربية; CJK: 日本語.\n\n> The viewport should remain anchored while images and wrapped content are refined.\n\n",
    );
    for image in 1..=8 {
        writeln!(
            source,
            "![Tachyon strata {section}.{image}]({image_source})\n"
        )
        .expect("string write");
    }
    source.push_str("### Deep task hierarchy\n\n");
    for depth in 0..12 {
        writeln!(
            source,
            "{}- [{}] depth {depth}: stable marker alignment and grapheme-safe editing",
            "    ".repeat(depth),
            if depth % 3 == 0 { "x" } else { " " }
        )
        .expect("string write");
    }
    source.push_str("\n### Wide measurement table\n\n");
    source.push_str("| Sample | Frame | Input | Present | Layout | Autosave | Scale | Notes |\n| ---: | ---: | ---: | ---: | ---: | ---: | ---: | :--- |\n");
    for row in 0..48 {
        writeln!(
            source,
            "| {row} | {} ms | {} ms | {} Hz | {} nodes | yes | 1.667 | section {section}, row {row}, source preserving |",
            2 + row % 5,
            4 + row % 7,
            120,
            256 + row * 13,
        )
        .expect("string write");
    }
    source.push_str(
        "\n### Long code and prose\n\n```rust\nfn measured_frame(section: usize) -> &'static str { let _keep_the_line_wide_to_exercise_local_horizontal_scrolling = section; \"input -> layout -> paint -> present\" }\n```\n\n",
    );
    for paragraph in 0..16 {
        writeln!(
            source,
            "Paragraph {section}.{paragraph} keeps the reading measure busy with representative prose. Selection dragging crosses shaped runs, while scrolling virtualizes distant blocks and a background autosave serializes only the changed unit."
        )
        .expect("string write");
        source.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_are_exact_size_and_cover_required_stressors() {
        let source = fixture(100 * 1024, "image.svg");
        assert_eq!(source.len(), 100 * 1024);
        assert!(source.contains("![Tachyon strata"));
        assert!(source.contains("| Sample | Frame |"));
        assert!(source.contains("                                            - ["));
        assert!(source.contains("```rust"));
    }
}
