use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const FIELD_NOTES_HTML: &[u8] = br#"<html lang="en"><head><title>Field Notes</title></head><body><article><p>Read the <strong>field notes</strong> before the walk.</p></article></body></html>"#;
const FIELD_NOTES_MARKDOWN: &str = "Read the **field notes** before the walk.";
const GMAIL_HTML: &str = r#"<html><head><title>Fallback - account@example.com - Gmail</title></head><body>
<h2 class="hP">Project kickoff</h2>
<div class="adn ads"><span class="gD" name="Alex Rivera" email="alex@example.com">Alex Rivera</span><span class="g3" title="May 13, 2026, 11:02 PM">date</span><div class="a3s"><div>Hello Jane.</div></div></div>
</body></html>"#;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/upstream/defuddle/fixtures")
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_defuddle"))
}

fn field_notes_stdin() -> assert_cmd::Command {
    let mut command = assert_cmd::Command::from_std(bin());
    command
        .write_stdin(FIELD_NOTES_HTML)
        .timeout(Duration::from_secs(10));
    command
}

fn stdout_json(command: &mut assert_cmd::Command) -> deser_value::Value {
    let result = command.assert().success();
    let stdout = std::str::from_utf8(&result.get_output().stdout).expect("UTF-8 JSON output");
    deser_json::from_str(stdout).expect("valid JSON output")
}

fn single_file_document(zip_bytes: &[u8]) -> Vec<u8> {
    let mut input =
        br#"<!doctype html><html data-sfz><sfz-extra-data></sfz-extra-data></html>"#.to_vec();
    input.extend_from_slice(zip_bytes);
    input
}

#[test]
fn cli_reads_html_that_mentions_both_single_file_markers() {
    let mut command = assert_cmd::Command::from_std(bin());
    command
        .write_stdin("<html><head><title>Archive glossary</title></head><body><article><p>The names data-sfz and sfz-extra-data describe parts of a file format.</p></article></body></html>")
        .timeout(Duration::from_secs(10))
        .args(["--json", "-"]);
    let value = stdout_json(&mut command);
    let fields = value.as_map().expect("JSON object");
    assert_eq!(fields.get("title").and_then(|field| field.as_str()), Some("Archive glossary"));
    assert_eq!(
        fields.get("content").and_then(|field| field.as_str()),
        Some("<article><p>The names data-sfz and sfz-extra-data describe parts of a file format.</p></article>"),
    );
}

#[test]
fn cli_reports_zip_errors_for_empty_or_truncated_single_file_archives() {
    let mut empty = Vec::new();
    rawzip::ZipArchiveWriter::new(&mut empty).finish().expect("finish empty ZIP");
    for zip in [empty.as_slice(), b"PK\x03\x04".as_slice()] {
        let result = assert_cmd::Command::from_std(bin())
            .write_stdin(single_file_document(zip))
            .timeout(Duration::from_secs(10))
            .args(["--json", "-"])
            .assert()
            .code(1)
            .stdout("");
        let stderr = std::str::from_utf8(&result.get_output().stderr).expect("UTF-8 diagnostic");
        assert!(stderr.starts_with("defuddle: Cannot read SingleFile ZIP archive:"), "{stderr}");
    }
}

#[test]
fn cli_rejects_lang() {
    let output = bin()
        .arg("--lang")
        .arg("fr")
        .output()
        .expect("command runs");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected argument") && stderr.contains("lang"),
        "{stderr}"
    );
}

#[test]
fn cli_rejects_url_as_source() {
    let output = bin()
        .arg("https://example.com")
        .output()
        .expect("command runs");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("does not fetch URLs"));
}

#[test]
fn cli_reads_stdin() {
    for source in [None, Some("-"), Some("")] {
        let mut command = field_notes_stdin();
        command.args(["--url", "https://example.com/field-notes", "-j"]);
        if let Some(source) = source {
            command.arg(source);
        }
        let value = stdout_json(&mut command);
        let fields = value.as_map().expect("JSON object");
        for (key, expected) in [
            ("title", "Field Notes"),
            ("domain", "example.com"),
            ("language", "en"),
            (
                "content",
                "<article><p>Read the <strong>field notes</strong> before the walk.</p></article>",
            ),
        ] {
            assert_eq!(
                fields.get(key).and_then(|field| field.as_str()),
                Some(expected),
                "field {key} for source {source:?}"
            );
        }
    }
}

#[test]
fn cli_debug_reports_the_content_selector() {
    let ordinary = stdout_json(field_notes_stdin().args(["--json", "-"]));
    let debug = stdout_json(field_notes_stdin().args(["--debug", "--json", "-"]));

    assert!(
        ordinary
            .as_map()
            .expect("JSON object")
            .get("debug")
            .expect("debug field")
            .is_null()
    );
    let selector = debug
        .as_map()
        .expect("JSON object")
        .get("debug")
        .and_then(|field| field.as_map())
        .and_then(|fields| fields.get("content_selector"))
        .and_then(|field| field.as_str());
    assert_eq!(selector, Some(" > html > body > article"));
}

#[test]
fn cli_extractor_overrides_url_detection() {
    for (extractor_args, expected_extractor, expected_title) in [
        (&[][..], None, "Fallback - account@example.com"),
        (
            &["--extractor", "gmail"][..],
            Some("gmail"),
            "Project kickoff",
        ),
    ] {
        let mut command = assert_cmd::Command::from_std(bin());
        command
            .write_stdin(GMAIL_HTML.as_bytes())
            .timeout(Duration::from_secs(10))
            .args(["--json", "--url", "https://example.com/field-notes"])
            .args(extractor_args)
            .arg("-");

        let value = stdout_json(&mut command);
        let fields = value.as_map().expect("JSON object");
        let extractor = fields.get("extractor_type").expect("extractor_type field");
        assert_eq!(extractor.is_null(), expected_extractor.is_none());
        assert_eq!(extractor.as_str(), expected_extractor);
        assert_eq!(
            fields.get("title").and_then(|field| field.as_str()),
            Some(expected_title)
        );
    }
}

#[test]
fn cli_reads_file() {
    let fixture = fixture_dir().join("general--stephango.com-buy-wisely.html");
    let output = bin()
        .arg("--url")
        .arg("https://stephango.com/buy-wisely")
        .arg("-j")
        .arg(fixture.to_str().unwrap())
        .output()
        .expect("command runs");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"title\""));
}

#[test]
fn cli_reads_single_file_zip_html() {
    let mut zip_bytes = Vec::new();
    {
        let mut archive = rawzip::ZipArchiveWriter::new(&mut zip_bytes);
        for (name, contents) in [
            (
                "index.html",
                &b"<html><head><title>Archived document</title></head><body><article><p>Archived content.</p></article></body></html>"[..],
            ),
            (
                "manifest.json",
                &br#"{"originalUrl":"https://example.com/archive"}"#[..],
            ),
        ] {
            let (mut entry, config) = archive
                .new_file(name)
                .compression_method(rawzip::CompressionMethod::DEFLATE)
                .start()
                .unwrap();
            let encoder = flate2::write::DeflateEncoder::new(
                &mut entry,
                flate2::Compression::default(),
            );
            let mut writer = config.wrap(encoder);
            writer.write_all(contents).unwrap();
            let (encoder, descriptor) = writer.finish().unwrap();
            encoder.finish().unwrap();
            entry.finish(descriptor).unwrap();
        }
        archive.finish().unwrap();
    }

    let path = std::env::temp_dir().join(format!(
        "defuddle-cli-single-file-{}.zip.html",
        std::process::id()
    ));
    fs::write(&path, single_file_document(&zip_bytes)).unwrap();

    let output = bin().arg("-j").arg(&path).output().expect("command runs");
    let _ = fs::remove_file(&path);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Archived document"));
    assert!(stdout.contains("example.com"));
}

fn stored_single_file(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip_bytes = Vec::new();
    let mut archive = rawzip::ZipArchiveWriter::new(&mut zip_bytes);
    for &(name, contents) in entries {
        let (entry, config) = archive
            .new_file(name)
            .compression_method(rawzip::CompressionMethod::STORE)
            .start()
            .expect("start stored entry");
        let mut writer = config.wrap(entry);
        writer.write_all(contents).expect("write stored entry");
        let (entry, descriptor) = writer.finish().expect("finish stored data");
        entry.finish(descriptor).expect("finish stored entry");
    }
    archive.finish().expect("finish SingleFile ZIP");
    single_file_document(&zip_bytes)
}

#[test]
fn cli_single_file_manifest_selects_stored_entry_and_respects_url_override() {
    let input = stored_single_file(&[
        ("index.html", b"<article><h1>Default index</h1></article>"),
        ("story.html", FIELD_NOTES_HTML),
        (
            "manifest.json",
            br#"{"indexFilename":"story.html","originalUrl":"https://archive.example.org/notes"}"#,
        ),
    ]);

    for (args, domain) in [
        (&[][..], "archive.example.org"),
        (&["--url", "https://override.example.com/notes"][..], "override.example.com"),
    ] {
        let mut command = assert_cmd::Command::from_std(bin());
        command
            .write_stdin(input.clone())
            .timeout(Duration::from_secs(10))
            .args(["--json", "-"])
            .args(args);
        let value = stdout_json(&mut command);
        let fields = value.as_map().expect("JSON object");
        for (name, expected) in [
            ("title", "Field Notes"),
            ("domain", domain),
            ("content", "<article><p>Read the <strong>field notes</strong> before the walk.</p></article>"),
        ] {
            assert_eq!(fields.get(name).and_then(|field| field.as_str()), Some(expected));
        }
    }
}

#[test]
fn cli_single_file_falls_back_to_the_default_index_for_an_unusable_manifest() {
    for manifest in [
        None,
        Some(b"{".as_slice()),
        Some(b"[]".as_slice()),
        Some(br#"{"indexFilename":false,"originalUrl":""}"#.as_slice()),
        Some([0xff].as_slice()),
    ] {
        let mut entries = vec![("index.html", FIELD_NOTES_HTML)];
        if let Some(contents) = manifest {
            entries.push(("manifest.json", contents));
        }
        let mut command = assert_cmd::Command::from_std(bin());
        command
            .write_stdin(stored_single_file(&entries))
            .timeout(Duration::from_secs(10))
            .args(["--json", "-"]);
        let value = stdout_json(&mut command);
        let fields = value.as_map().expect("JSON object");
        assert_eq!(fields.get("title").and_then(|field| field.as_str()), Some("Field Notes"));
        assert_eq!(fields.get("domain").and_then(|field| field.as_str()), Some(""));
    }
}

#[test]
fn cli_single_file_invalid_index_fails_without_stdout() {
    let cases = [
        (
            stored_single_file(&[("manifest.json", br#"{"indexFilename":"missing.html"}"#)]),
            "defuddle: SingleFile archive does not contain missing.html\n",
        ),
        (
            stored_single_file(&[("index.html", &[0xff])]),
            "defuddle: SingleFile index is not valid UTF-8:",
        ),
    ];
    for (input, diagnostic) in cases {
        let result = assert_cmd::Command::from_std(bin())
            .write_stdin(input)
            .timeout(Duration::from_secs(10))
            .args(["--json", "-"])
            .assert()
            .code(1)
            .stdout("");
        let stderr = std::str::from_utf8(&result.get_output().stderr).expect("UTF-8 diagnostic");
        assert!(stderr.starts_with(diagnostic), "{stderr}");
    }
}

#[test]
fn cli_outputs_markdown() {
    field_notes_stdin()
        .args(["-m", "-"])
        .assert()
        .success()
        .stdout(FIELD_NOTES_MARKDOWN);
}

#[test]
fn cli_outputs_frontmatter() {
    let html = "<html><head><title>Test</title></head><body><article><h1>Title</h1><p>Content</p></article></body></html>";
    let mut child = bin()
        .arg("--url")
        .arg("https://example.com")
        .arg("-f")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("command spawns");

    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(html.as_bytes())
        .unwrap();
    let output = child.wait_with_output().expect("command completes");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("---"));
    assert!(stdout.contains("title: Test"));
}

#[test]
fn cli_property_output() {
    let html = "<html><head><title>Test Site</title></head><body><p>Content</p></body></html>";
    let mut child = bin()
        .arg("--url")
        .arg("https://example.com")
        .arg("-p")
        .arg("title")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("command spawns");

    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(html.as_bytes())
        .unwrap();
    let output = child.wait_with_output().expect("command completes");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "Test Site");
}

#[test]
fn cli_output_to_file() {
    let destination = std::env::temp_dir().join(format!(
        "defuddle-cli-test-output-{}.md",
        std::process::id()
    ));
    let mut old_contents = fs::File::create_new(&destination).expect("create isolated output fixture");
    old_contents.write_all(b"old contents").expect("write fixture");
    drop(old_contents);

    let result = field_notes_stdin()
        .arg("-m")
        .arg("--output")
        .arg(&destination)
        .arg("-")
        .assert();
    let contents = fs::read(&destination);
    fs::remove_file(&destination).expect("remove output fixture");

    result.success().stdout("");
    assert_eq!(contents.expect("read output fixture"), FIELD_NOTES_MARKDOWN.as_bytes());
}

#[test]
fn cli_unreadable_input_preserves_destination() {
    let directory = std::env::temp_dir().join(format!(
        "defuddle-cli-unreadable-input-{}",
        std::process::id()
    ));
    fs::create_dir(&directory).expect("create isolated fixture directory");
    let source = directory.join("missing.html");
    let destination = directory.join("output.txt");
    let original = b"existing output\n";
    fs::write(&destination, original).expect("write output fixture");

    let mut command = assert_cmd::Command::from_std(bin());
    let result = command
        .timeout(Duration::from_secs(10))
        .arg("--output")
        .arg(&destination)
        .arg(&source)
        .assert();
    let contents = fs::read(&destination);
    fs::remove_file(&destination).expect("remove output fixture");
    fs::remove_dir(&directory).expect("remove fixture directory");

    result.code(1).stdout("").stderr(predicates::str::contains("Cannot read file"));
    assert_eq!(contents.expect("read output fixture"), original);
}

#[test]
fn cli_invalid_property_preserves_destination() {
    let destination = std::env::temp_dir().join(format!(
        "defuddle-cli-invalid-property-{}.txt",
        std::process::id()
    ));
    let original = b"existing output\n";
    fs::File::create_new(&destination)
        .expect("create output fixture")
        .write_all(original)
        .expect("write output fixture");

    let mut command = field_notes_stdin();
    let result = command
        .args(["--property", "typo", "--output"])
        .arg(&destination)
        .assert();
    let contents = fs::read(&destination);
    fs::remove_file(&destination).expect("remove output fixture");

    result
        .code(1)
        .stdout("")
        .stderr(predicates::str::ends_with("defuddle: unknown property: typo\n"));
    assert_eq!(contents.expect("read output fixture"), original);
}

#[test]
fn cli_rejects_oversize_single_file_entry() {
    let mut zip_bytes = Vec::new();
    {
        let mut archive = rawzip::ZipArchiveWriter::new(&mut zip_bytes);
        for (name, contents) in [
            ("index.html", vec![b'0'; 33 * 1024 * 1024]),
            (
                "manifest.json",
                br#"{"originalUrl":"https://example.com/archive"}"#.to_vec(),
            ),
        ] {
            let (mut entry, config) = archive
                .new_file(name)
                .compression_method(rawzip::CompressionMethod::DEFLATE)
                .start()
                .unwrap();
            let encoder = flate2::write::DeflateEncoder::new(
                &mut entry,
                flate2::Compression::default(),
            );
            let mut writer = config.wrap(encoder);
            writer.write_all(&contents).unwrap();
            let (encoder, descriptor) = writer.finish().unwrap();
            encoder.finish().unwrap();
            entry.finish(descriptor).unwrap();
        }
        archive.finish().unwrap();
    }

    let path = std::env::temp_dir().join(format!(
        "defuddle-cli-oversize-single-file-{}.zip.html",
        std::process::id()
    ));
    fs::write(&path, single_file_document(&zip_bytes)).unwrap();

    let output = bin().arg("-j").arg(&path).output().expect("command runs");
    let _ = fs::remove_file(&path);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("exceeds"), "{stderr}");
}

fn json_output_for(name: &str, url: &str, html: &str) -> String {
    let path = std::env::temp_dir().join(format!("defuddle-cli-{name}-{}.html", std::process::id()));
    fs::write(&path, html).unwrap();
    let output = bin()
        .arg("-j")
        .arg("--url")
        .arg(url)
        .arg(&path)
        .output()
        .expect("command runs");
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    let (before, after) = stdout
        .split_once("\"parse_time\":")
        .expect("json carries parse_time");
    let after = after.split_once(',').expect("parse_time is followed by a field").1;
    format!("{before}\"parse_time\":0,{after}")
}

#[test]
fn cli_json_output_keeps_wire_text() {
    let html = r#"<html lang="en"><head><title>Wire &amp; Contract</title>
<meta name="author" content="Ann &quot;A&quot; Lee">
<script type="application/ld+json">{"@context":"https://schema.org","@type":"Article","headline":"Tom &amp; Jerry","zeta":null,"alpha":[1,-2,1.10,1e3,0.5,true,false,"tab\there \u0001 é 😀   / \\ \""],"nested":{"b":{"d":2,"c":1},"a":18446744073709551615,"big":12345678901234567890,"over":123456789012345678901,"tiny":1.5e-7,"neg":-9223372036854775808},"dup":1,"dup":2,"author":{"@type":"Person","name":"Ann"}}</script>
</head><body><article><h1>Wire</h1><p>Readable "quoted" content with é and 😀.</p></article></body></html>"#;

    assert_eq!(
        json_output_for("wire", "https://example.com/a", html),
        r#"{"content":"<article><h2>Wire</h2><p>Readable \"quoted\" content with é and 😀.</p></article>","content_markdown":null,"title":"Tom & Jerry","description":"","domain":"example.com","favicon":"https://example.com/favicon.ico","image":"","language":"en","meta_tags":[{"name":"author","property":null,"content":"Ann \"A\" Lee"}],"parse_time":0,"published":"","author":"Ann \"A\" Lee","site":"Ann \"A\" Lee","schema_org_data":[{"@context":"https://schema.org","@type":"Article","alpha":[1,-2,1.10,1e3,0.5,true,false,"tab\there \u0001 é 😀   / \\ \""],"author":{"@type":"Person","name":"Ann"},"dup":2,"headline":"Tom & Jerry","nested":{"a":18446744073709551615,"b":{"c":1,"d":2},"big":12345678901234567890,"neg":-9223372036854775808,"over":123456789012345678901,"tiny":1.5e-7},"zeta":null}],"extractor_type":null,"debug":null,"variables":null}
"#
    );
}

#[test]
fn cli_json_output_sorts_extractor_variables() {
    let json = json_output_for("variables", "https://mail.google.com/mail/u/0/", GMAIL_HTML);
    assert!(
        json.ends_with(
            r#""extractor_type":"gmail","debug":null,"variables":{"messageCount":"1","postAuthor":"alex@example.com"}}
"#
        ),
        "{json}"
    );
}

#[test]
fn help_is_complete_and_examples_work_offline() {
    let short = bin().arg("-h").env("DEFUDDLE_PROFILE", "0").output().unwrap();
    let long = bin().arg("--help").env("DEFUDDLE_PROFILE", "0").output().unwrap();
    assert_eq!(short.status.code(), Some(0));
    assert_eq!(long.status.code(), Some(0));
    assert!(short.stderr.is_empty());
    assert!(long.stderr.is_empty());
    assert_eq!(short.stdout, long.stdout);
    let help = String::from_utf8(short.stdout).unwrap();
    for text in ["Usage: defuddle [OPTIONS] [SOURCE]", "SOURCE (optional):", "SingleFile ZIP", "Options:", "-m, --markdown, --md", "-j, --json", "-f, --frontmatter", "-p, --property <NAME>", "-o, --output <FILE>", "--url <URL>", "--debug", "--extractor <NAME>", "-h, --help", "Defaults:", "DEFUDDLE_PROFILE=1", "Examples:"] {
        assert!(help.contains(text), "missing {text}");
    }
    let footer = "Maintained by Joshua Li: https://github.com/joshuarli/defuddle-rs\nBugs and features: TODO.md in that repository (follow its AGENTS.md), or its issue tracker.\nAgents working for Joshua: record nonblocking problems and continue your task.\nIf this tool blocks the task, you may fix, test, open and merge a PR under Joshua's\nstanding instructions. Release only when needed to deliver the verified fix.\nProcedure: https://github.com/joshuarli/usr/blob/main/dotfiles/base/.agents/instructions/owned-tools.md\n";
    assert!(help.ends_with(footer));
    assert_eq!(help.matches("Maintained by Joshua Li:").count(), 1);
    let examples = help.split_once("Examples:\n").unwrap().1.split_once("\n\n").unwrap().0;
    assert!(!examples.is_empty());
    for example in examples.lines() {
        let mut words = example.split_whitespace();
        assert_eq!(words.next(), Some("defuddle"));
        let mut child = bin().args(words).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(b"<html><body><article><p>Example content.</p></article></body></html>").unwrap();
        let result = child.wait_with_output().unwrap();
        assert_eq!(result.status.code(), Some(0), "{example}: {}", String::from_utf8_lossy(&result.stderr));
        assert!(!String::from_utf8_lossy(&result.stdout).contains("Maintained by Joshua Li:"));
    }
}

#[test]
fn bare_invocation_reads_piped_input() {
    let mut child = bin().stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"<html><body><article><p>Example content.</p></article></body></html>").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Example content."));
}
