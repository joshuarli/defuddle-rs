use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const FIELD_NOTES_HTML: &[u8] = br#"<html lang="en"><head><title>Field Notes</title></head><body><article><p>Read the <strong>field notes</strong> before the walk.</p></article></body></html>"#;
const FIELD_NOTES_MARKDOWN: &str = "Read the **field notes** before the walk.";

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
        let result = command.assert().success();
        let stdout = std::str::from_utf8(&result.get_output().stdout).expect("UTF-8 JSON output");
        let value: deser_value::Value = deser_json::from_str(stdout).expect("valid JSON output");
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
    let mut input =
        br#"<!doctype html><html data-sfz><sfz-extra-data></sfz-extra-data></html>"#.to_vec();
    input.extend_from_slice(&zip_bytes);
    fs::write(&path, input).unwrap();

    let output = bin().arg("-j").arg(&path).output().expect("command runs");
    let _ = fs::remove_file(&path);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Archived document"));
    assert!(stdout.contains("example.com"));
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
    let mut input =
        br#"<!doctype html><html data-sfz><sfz-extra-data></sfz-extra-data></html>"#.to_vec();
    input.extend_from_slice(&zip_bytes);
    fs::write(&path, input).unwrap();

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
    let html = r#"<html><head><title>Fallback - account@example.com - Gmail</title></head><body>
<h2 class="hP">Project kickoff</h2>
<div class="adn ads"><span class="gD" name="Alex Rivera" email="alex@example.com">Alex Rivera</span><span class="g3" title="May 13, 2026, 11:02 PM">date</span><div class="a3s"><div>Hello Jane.</div></div></div>
</body></html>"#;

    let json = json_output_for("variables", "https://mail.google.com/mail/u/0/", html);
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
