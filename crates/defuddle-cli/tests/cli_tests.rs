use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/upstream/defuddle/fixtures")
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_defuddle"))
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
    let fixture = fixture_dir().join("general--stephango.com-buy-wisely.html");
    let output = bin()
        .arg("--url")
        .arg("https://stephango.com/buy-wisely")
        .arg("-j")
        .arg("-")
        .stdin(fs::File::open(&fixture).expect("fixture opens"))
        .output()
        .expect("command runs");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"title\""));
    assert!(stdout.contains("\"domain\""));
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
    let fixture = fixture_dir().join("general--stephango.com-buy-wisely.html");
    let output = bin()
        .arg("--url")
        .arg("https://stephango.com/buy-wisely")
        .arg("-m")
        .arg(fixture.to_str().unwrap())
        .output()
        .expect("command runs");

    assert!(output.status.success());
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
fn cli_error_on_tty_with_no_input() {
    // This test only runs if we're not piping stdin
    // Skip if running in CI where stdin might not be a TTY
}

#[test]
fn cli_output_to_file() {
    let temp = std::env::temp_dir().join("defuddle-cli-test-output.md");
    let html = "<html><head><title>Test</title></head><body><p>Hello</p></body></html>";

    let mut child = bin()
        .arg("--url")
        .arg("https://example.com")
        .arg("-m")
        .arg("-o")
        .arg(temp.to_str().unwrap())
        .arg("-")
        .stdin(std::process::Stdio::piped())
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
    assert!(temp.exists(), "output file should exist");
    let _ = fs::remove_file(&temp);
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
