use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process;

#[cfg(feature = "alloc-profile")]
#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

#[derive(Clone, Copy)]
enum OptionKind {
    Markdown, Json, Frontmatter, Property, Output, Url, Debug, Extractor, Help,
}

struct OptionSpec {
    kind: OptionKind,
    short: Option<char>,
    longs: &'static [&'static str],
    value: Option<&'static str>,
    description: &'static str,
}

const OPTIONS: &[OptionSpec] = &[
    OptionSpec { kind: OptionKind::Markdown, short: Some('m'), longs: &["markdown", "md"], value: None, description: "Output Markdown instead of HTML" },
    OptionSpec { kind: OptionKind::Json, short: Some('j'), longs: &["json"], value: None, description: "Output JSON with metadata and content" },
    OptionSpec { kind: OptionKind::Frontmatter, short: Some('f'), longs: &["frontmatter"], value: None, description: "Output metadata frontmatter; add --markdown for the body" },
    OptionSpec { kind: OptionKind::Property, short: Some('p'), longs: &["property"], value: Some("NAME"), description: "Output one value: title, author, description, domain, favicon, image, language, published, site, or content" },
    OptionSpec { kind: OptionKind::Output, short: Some('o'), longs: &["output"], value: Some("FILE"), description: "Write output to FILE instead of stdout" },
    OptionSpec { kind: OptionKind::Url, short: None, longs: &["url"], value: Some("URL"), description: "Set document URL for metadata and relative URL resolution (no fetch)" },
    OptionSpec { kind: OptionKind::Debug, short: None, longs: &["debug"], value: None, description: "Include the content selector in JSON output" },
    OptionSpec { kind: OptionKind::Extractor, short: None, longs: &["extractor"], value: Some("NAME"), description: "Force a site extractor: hackernews, substack, reddit, github, gmail, chatgpt, mastodon, or twitter" },
    OptionSpec { kind: OptionKind::Help, short: Some('h'), longs: &["help"], value: None, description: "Show this help" },
];

const EXAMPLES: &[&str] = &[
    "defuddle --markdown -",
    "defuddle --json --url https://example.com/article -",
];

const FOOTER: &str = "Maintained by Joshua Li: https://github.com/joshuarli/defuddle-rs\nBugs and features: TODO.md in that repository (follow its AGENTS.md), or its issue tracker.\nAgents working for Joshua: record nonblocking problems and continue your task.\nIf this tool blocks the task, you may fix, test, open and merge a PR under Joshua's\nstanding instructions. Release only when needed to deliver the verified fix.\nProcedure: https://github.com/joshuarli/usr/blob/main/dotfiles/base/.agents/instructions/owned-tools.md";

struct CliInput {
    source_html: String,
    page_url: Option<String>,
}

fn main() {
    #[cfg(feature = "alloc-profile")]
    let _profiler = dhat::Profiler::builder().trim_backtraces(Some(30)).build();

    if let Err(e) = run() {
        eprintln!("defuddle: {e}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut parser = lexopt::Parser::from_env();
    let mut markdown = false;
    let mut json_output = false;
    let mut frontmatter = false;
    let mut property: Option<String> = None;
    let mut output_path: Option<PathBuf> = None;
    let mut page_url: Option<String> = None;
    let mut debug = false;
    let mut extractor: Option<String> = None;
    let mut input_source: Option<String> = None;

    while let Some(arg) = parser.next().map_err(|e| e.to_string())? {
        if let lexopt::Arg::Value(val) = arg {
            input_source = Some(val.into_string().map_err(|_| "invalid source argument".to_string())?);
            continue;
        }
        let spec = OPTIONS.iter().find(|spec| match &arg {
            lexopt::Arg::Short(short) => spec.short == Some(*short),
            lexopt::Arg::Long(long) => spec.longs.contains(long),
            lexopt::Arg::Value(_) => false,
        }).ok_or_else(|| format!("unexpected argument: {arg:?}"))?;
        match spec.kind {
            OptionKind::Markdown => markdown = true,
            OptionKind::Json => json_output = true,
            OptionKind::Frontmatter => frontmatter = true,
            OptionKind::Property => {
                property = Some(parser.value().map_err(|e| e.to_string())?.into_string().map_err(|_| "invalid property value".to_string())?);
            }
            OptionKind::Output => {
                output_path = Some(PathBuf::from(parser.value().map_err(|e| e.to_string())?.to_str().ok_or("invalid output path")?));
            }
            OptionKind::Url => {
                page_url = Some(parser.value().map_err(|e| e.to_string())?.into_string().map_err(|_| "invalid url value".to_string())?);
            }
            OptionKind::Debug => debug = true,
            OptionKind::Extractor => {
                extractor = Some(parser.value().map_err(|e| e.to_string())?.into_string().map_err(|_| "invalid extractor value".to_string())?);
            }
            OptionKind::Help => {
                print_help();
                return Ok(());
            }
        }
    }

    if let Some(ref source) = input_source
        && defuddle::is_absolute_url(source)
    {
        return Err(
                "The defuddle CLI does not fetch URLs. Pass a local HTML file path or pipe HTML via stdin.\n\
                 Use --url <url> to set the document URL for metadata and relative URL resolution."
                    .to_string(),
            );
    }

    let cli_input = read_cli_input(&input_source)?;
    if page_url.is_none() {
        page_url = cli_input.page_url;
    }

    let options = defuddle::DefuddleOptions {
        url: page_url,
        debug,
        extractor,
        separate_markdown: markdown,
        ..Default::default()
    };

    let parsed_page = defuddle::parse_html(&cli_input.source_html, options)
        .map_err(|e| format!("Parse error: {e}"))?;
    eprintln!(
        "defuddle: detected extractor: {}",
        parsed_page.extractor_type.as_deref().unwrap_or("generic")
    );

    let property_value = match property.as_deref() {
        Some(prop) => Some(match prop {
            "title" => &parsed_page.title,
            "author" => &parsed_page.author,
            "description" => &parsed_page.description,
            "domain" => &parsed_page.domain,
            "favicon" => &parsed_page.favicon,
            "image" => &parsed_page.image,
            "language" => &parsed_page.language,
            "published" => &parsed_page.published,
            "site" => &parsed_page.site,
            "content" => &parsed_page.content,
            other => return Err(format!("unknown property: {other}")),
        }),
        None => None,
    };

    let out: Box<dyn Write> = if let Some(ref path) = output_path {
        Box::new(fs::File::create(path).map_err(|e| format!("Cannot open output file: {e}"))?)
    } else {
        Box::new(io::stdout())
    };

    if let Some(value) = property_value {
        let mut out = out;
        writeln!(out, "{value}").map_err(|e| e.to_string())?;
        return Ok(());
    }

    if json_output {
        let mut out = out;
        let json = deser_json::to_string(&parsed_page).map_err(|e| e.to_string())?;
        writeln!(out, "{json}").map_err(|e| e.to_string())?;
        return Ok(());
    }

    if frontmatter {
        let mut out = out;
        writeln!(out, "---").map_err(|e| e.to_string())?;
        writeln!(out, "title: {}", parsed_page.title).map_err(|e| e.to_string())?;
        writeln!(out, "author: {}", parsed_page.author).map_err(|e| e.to_string())?;
        if !parsed_page.description.is_empty() {
            writeln!(out, "description: {}", parsed_page.description)
                .map_err(|e| e.to_string())?;
        }
        writeln!(out, "site: {}", parsed_page.site).map_err(|e| e.to_string())?;
        if !parsed_page.published.is_empty() {
            writeln!(out, "published: {}", parsed_page.published).map_err(|e| e.to_string())?;
        }
        writeln!(out, "language: {}", parsed_page.language).map_err(|e| e.to_string())?;
        writeln!(out, "domain: {}", parsed_page.domain).map_err(|e| e.to_string())?;
        writeln!(out, "---").map_err(|e| e.to_string())?;
        if markdown && let Some(ref md) = parsed_page.content_markdown {
            write!(out, "{md}").map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    let mut out = out;
    if markdown {
        if let Some(ref md) = parsed_page.content_markdown {
            write!(out, "{md}").map_err(|e| e.to_string())?;
        }
    } else {
        write!(out, "{}", parsed_page.content).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn read_cli_input(input_source: &Option<String>) -> Result<CliInput, String> {
    let bytes = match input_source {
        Some(path) if path == "-" || path.is_empty() => {
            let mut buf = Vec::new();
            io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| format!("Cannot read stdin: {e}"))?;
            buf
        }
        Some(path) => fs::read(path).map_err(|e| format!("Cannot read file {path}: {e}"))?,
        None => {
            if io::stdin().is_terminal() {
                return Err("No input provided. Pass an HTML file path, `-` for stdin, or pipe HTML to stdin.".to_string());
            }
            let mut buf = Vec::new();
            io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| format!("Cannot read stdin: {e}"))?;
            buf
        }
    };

    if is_single_file_cli_archive(&bytes) {
        return read_single_file_cli_archive(&bytes);
    }

    let source_html =
        String::from_utf8(bytes).map_err(|e| format!("Input is not valid UTF-8: {e}"))?;
    Ok(CliInput {
        source_html,
        page_url: None,
    })
}

fn is_single_file_cli_archive(bytes: &[u8]) -> bool {
    bytes
        .windows(b"data-sfz".len())
        .any(|window| window == b"data-sfz")
        && bytes
            .windows(b"sfz-extra-data".len())
            .any(|window| window == b"sfz-extra-data")
        && bytes
            .windows(4)
            .any(|window| window == b"PK\x03\x04" || window == b"PK\x05\x06")
}

fn read_single_file_cli_archive(bytes: &[u8]) -> Result<CliInput, String> {
    let archive = rawzip::ZipArchive::from_slice(bytes)
        .map_err(|e| format!("Cannot read SingleFile ZIP archive: {e}"))?;

    let (index_name, archive_url) = {
        let manifest = find_archive_entry(&archive, "manifest.json")
            .ok()
            .flatten()
            .and_then(|(wayfinder, method)| {
                let bytes = read_archive_entry(&archive, wayfinder, method).ok()?;
                let contents = String::from_utf8(bytes).ok()?;
                let value = deser_json::from_str::<deser_value::Value>(&contents).ok()?;
                let object = value.as_map()?;
                let index_name = object
                    .get("indexFilename")
                    .and_then(|value| value.as_str())
                    .map_or_else(|| "index.html".to_string(), str::to_string);
                let archive_url = object
                    .get("originalUrl")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                Some((index_name, archive_url))
            });

        manifest.unwrap_or_else(|| ("index.html".to_string(), None))
    };

    let (wayfinder, method) = find_archive_entry(&archive, &index_name)
        .map_err(|e| format!("Cannot read SingleFile ZIP archive: {e}"))?
        .ok_or_else(|| format!("SingleFile archive does not contain {index_name}"))?;
    let html_bytes = read_archive_entry(&archive, wayfinder, method)
        .map_err(|e| format!("Cannot read {index_name} from SingleFile archive: {e}"))?;
    let source_html = String::from_utf8(html_bytes)
        .map_err(|e| format!("SingleFile index is not valid UTF-8: {e}"))?;

    Ok(CliInput {
        source_html,
        page_url: archive_url,
    })
}

fn find_archive_entry(
    archive: &rawzip::ZipSliceArchive<&[u8]>,
    name: &str,
) -> Result<Option<(rawzip::ZipArchiveEntryWayfinder, rawzip::CompressionMethod)>, rawzip::Error>
{
    let mut entries = archive.entries();
    while let Some(entry) = entries.next_entry()? {
        if entry.file_path().as_ref() == name.as_bytes() {
            return Ok(Some((entry.wayfinder(), entry.compression_method())));
        }
    }
    Ok(None)
}

const MAX_ARCHIVE_ENTRY_BYTES: u64 = 32 * 1024 * 1024;

fn read_archive_entry(
    archive: &rawzip::ZipSliceArchive<&[u8]>,
    wayfinder: rawzip::ZipArchiveEntryWayfinder,
    method: rawzip::CompressionMethod,
) -> Result<Vec<u8>, String> {
    if wayfinder.uncompressed_size_hint() > MAX_ARCHIVE_ENTRY_BYTES {
        return Err(format!(
            "archive entry exceeds {MAX_ARCHIVE_ENTRY_BYTES} byte limit"
        ));
    }
    let entry = archive
        .get_entry(wayfinder)
        .map_err(|error| error.to_string())?;
    let data = entry.data();
    let decoder: Box<dyn Read + '_> = if method == rawzip::CompressionMethod::STORE {
        Box::new(data)
    } else if method == rawzip::CompressionMethod::DEFLATE {
        Box::new(flate2::read::DeflateDecoder::new(data))
    } else {
        return Err(format!("unsupported compression method: {method:?}"));
    };
    let reader = entry.verifying_reader(decoder);
    let mut contents = Vec::new();
    reader
        .take(MAX_ARCHIVE_ENTRY_BYTES + 1)
        .read_to_end(&mut contents)
        .map_err(|error| error.to_string())?;
    if contents.len() as u64 > MAX_ARCHIVE_ENTRY_BYTES {
        return Err(format!(
            "archive entry exceeds {MAX_ARCHIVE_ENTRY_BYTES} byte limit"
        ));
    }
    Ok(contents)
}

fn print_help() {
    println!("defuddle - HTML article extraction\n\nUsage: defuddle [OPTIONS] [SOURCE]\n\nSOURCE (optional):\n  HTML file or SingleFile ZIP archive; - reads stdin. If omitted, reads stdin\n  when it is not a terminal. Absolute URLs are not fetched.\n\nOptions:");
    for spec in OPTIONS {
        let mut names = Vec::new();
        if let Some(short) = spec.short {
            names.push(format!("-{short}"));
        }
        names.extend(spec.longs.iter().map(|long| format!("--{long}")));
        let mut label = names.join(", ");
        if let Some(value) = spec.value {
            label.push_str(&format!(" <{value}>"));
        }
        println!("  {label}\n      {}", spec.description);
    }
    println!("\nDefaults: cleaned HTML to stdout; extractor selected automatically.\nEnvironment: DEFUDDLE_PROFILE=1 enables phase timings.\n\nExamples:");
    for example in EXAMPLES {
        println!("  {example}");
    }
    println!("\n{FOOTER}");
}

#[cfg(test)]
mod tests {
    use super::OPTIONS;

    #[test]
    fn option_names_are_unique() {
        let mut shorts = Vec::new();
        let mut longs = Vec::new();
        for spec in OPTIONS {
            if let Some(short) = spec.short {
                assert!(!shorts.contains(&short));
                shorts.push(short);
            }
            for long in spec.longs {
                assert!(!longs.contains(&long));
                longs.push(long);
            }
        }
    }
}
