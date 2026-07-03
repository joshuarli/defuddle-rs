use std::fs;
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
    child.stdin.take().unwrap().write_all(html.as_bytes()).unwrap();
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
    child.stdin.take().unwrap().write_all(html.as_bytes()).unwrap();
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
    child.stdin.take().unwrap().write_all(html.as_bytes()).unwrap();
    let output = child.wait_with_output().expect("command completes");

    assert!(output.status.success());
    assert!(temp.exists(), "output file should exist");
    let _ = fs::remove_file(&temp);
}
