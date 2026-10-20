use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process;

#[cfg(feature = "alloc-profile")]
#[global_allocator]
static ALLOCATOR: dhat::Alloc = dhat::Alloc;

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
    let mut language: Option<String> = None;
    let mut debug = false;
    let mut extractor: Option<String> = None;
    let mut input_source: Option<String> = None;

    while let Some(arg) = parser.next().map_err(|e| e.to_string())? {
        match arg {
            lexopt::Arg::Short('m') | lexopt::Arg::Long("markdown") | lexopt::Arg::Long("md") => {
                markdown = true;
            }
            lexopt::Arg::Short('j') | lexopt::Arg::Long("json") => {
                json_output = true;
            }
            lexopt::Arg::Short('f') | lexopt::Arg::Long("frontmatter") => {
                frontmatter = true;
            }
            lexopt::Arg::Short('p') | lexopt::Arg::Long("property") => {
                property = Some(
                    parser
                        .value()
                        .map_err(|e| e.to_string())?
                        .into_string()
                        .map_err(|_| "invalid property value".to_string())?,
                );
            }
            lexopt::Arg::Short('o') | lexopt::Arg::Long("output") => {
                output_path = Some(PathBuf::from(
                    parser
                        .value()
                        .map_err(|e| e.to_string())?
                        .to_str()
                        .ok_or("invalid output path")?,
                ));
            }
            lexopt::Arg::Long("url") => {
                page_url = Some(
                    parser
                        .value()
                        .map_err(|e| e.to_string())?
                        .into_string()
                        .map_err(|_| "invalid url value".to_string())?,
                );
            }
            lexopt::Arg::Short('l') | lexopt::Arg::Long("lang") => {
                language = Some(
                    parser
                        .value()
                        .map_err(|e| e.to_string())?
                        .into_string()
                        .map_err(|_| "invalid lang value".to_string())?,
                );
            }
            lexopt::Arg::Long("debug") => {
                debug = true;
            }
            lexopt::Arg::Long("extractor") => {
                extractor = Some(
                    parser
                        .value()
                        .map_err(|e| e.to_string())?
                        .into_string()
                        .map_err(|_| "invalid extractor value".to_string())?,
                );
            }
            lexopt::Arg::Short('h') | lexopt::Arg::Long("help") => {
                print_help();
                return Ok(());
            }
            lexopt::Arg::Value(val) => {
                input_source = Some(
                    val.into_string()
                        .map_err(|_| "invalid source argument".to_string())?,
                );
            }
            _ => {
                return Err(format!("unexpected argument: {arg:?}"));
            }
        }
    }

    if let Some(ref source) = input_source
        && defuddle::content_utils::is_absolute_url(source)
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
        language,
        ..Default::default()
    };

    let parsed_page = defuddle::parse_html(&cli_input.source_html, options)
        .map_err(|e| format!("Parse error: {e}"))?;
    eprintln!(
        "defuddle: detected extractor: {}",
        parsed_page.extractor_type.as_deref().unwrap_or("generic")
    );

    let out: Box<dyn Write> = if let Some(ref path) = output_path {
        Box::new(fs::File::create(path).map_err(|e| format!("Cannot open output file: {e}"))?)
    } else {
        Box::new(io::stdout())
    };

    if let Some(ref prop) = property {
        let value = match prop.as_str() {
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
        };
        let mut out = out;
        writeln!(out, "{value}").map_err(|e| e.to_string())?;
        return Ok(());
    }

    if json_output {
        let mut out = out;
        let json = miniserde::json::to_string(&parsed_page);
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
            if atty::is(atty::Stream::Stdin) {
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
                let value = miniserde::json::from_str::<miniserde::json::Value>(&contents).ok()?;
                let object = match value {
                    miniserde::json::Value::Object(object) => object,
                    _ => return None,
                };
                let index_name = object
                    .get("indexFilename")
                    .and_then(|value| match value {
                        miniserde::json::Value::String(value) => Some(value.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "index.html".to_string());
                let archive_url = object.get("originalUrl").and_then(|value| match value {
                    miniserde::json::Value::String(value) if !value.is_empty() => {
                        Some(value.clone())
                    }
                    _ => None,
                });
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

fn read_archive_entry(
    archive: &rawzip::ZipSliceArchive<&[u8]>,
    wayfinder: rawzip::ZipArchiveEntryWayfinder,
    method: rawzip::CompressionMethod,
) -> Result<Vec<u8>, String> {
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
    let mut reader = entry.verifying_reader(decoder);
    let mut contents = Vec::new();
    reader
        .read_to_end(&mut contents)
        .map_err(|error| error.to_string())?;
    Ok(contents)
}

fn print_help() {
    eprintln!(
        r#"defuddle - HTML article extraction

Usage: defuddle [OPTIONS] [SOURCE]

SOURCE:
  <file>        Path to an HTML file
                A SingleFile ZIP archive is also accepted
  -             Read HTML from stdin
  (omitted)     Read HTML from stdin if not a TTY

Options:
  -m, --markdown, --md   Output Markdown instead of HTML
  -j, --json              Output JSON with metadata and content
  -f, --frontmatter       Output metadata frontmatter; add --markdown for the body
  -p, --property <name>   Output one value: title, author, description, domain,
                          favicon, image, language, published, site, or content
  -o, --output <file>     Write output to file instead of stdout
  --url <url>             Set document URL for metadata resolution
  -l, --lang <code>       Preferred language (BCP 47 tag)
  --debug                 Enable debug output
  --extractor <name>      Force a named site extractor (e.g. substack)
  -h, --help              Show this help
"#
    );
}
