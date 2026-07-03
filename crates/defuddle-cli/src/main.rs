use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process;

fn main() {
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
    let mut output: Option<PathBuf> = None;
    let mut url: Option<String> = None;
    let mut lang: Option<String> = None;
    let mut debug = false;
    let mut source: Option<String> = None;

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
                property = Some(parser.value().map_err(|e| e.to_string())?.into_string().map_err(|_| "invalid property value".to_string())?);
            }
            lexopt::Arg::Short('o') | lexopt::Arg::Long("output") => {
                output = Some(PathBuf::from(parser.value().map_err(|e| e.to_string())?.to_str().ok_or("invalid output path")?));
            }
            lexopt::Arg::Long("url") => {
                url = Some(parser.value().map_err(|e| e.to_string())?.into_string().map_err(|_| "invalid url value".to_string())?);
            }
            lexopt::Arg::Short('l') | lexopt::Arg::Long("lang") => {
                lang = Some(parser.value().map_err(|e| e.to_string())?.into_string().map_err(|_| "invalid lang value".to_string())?);
            }
            lexopt::Arg::Long("debug") => {
                debug = true;
            }
            lexopt::Arg::Short('h') | lexopt::Arg::Long("help") => {
                print_help();
                return Ok(());
            }
            lexopt::Arg::Value(val) => {
                source = Some(val.into_string().map_err(|_| "invalid source argument".to_string())?);
            }
            _ => {
                return Err(format!("unexpected argument: {arg:?}"));
            }
        }
    }

    if let Some(ref src) = source {
        if src.starts_with("https://") || src.starts_with("http://") {
            return Err(
                "The defuddle CLI does not fetch URLs. Pass a local HTML file path or pipe HTML via stdin.\n\
                 Use --url <url> to set the document URL for metadata and relative URL resolution."
                    .to_string(),
            );
        }
    }

    let html = read_input(&source)?;

    let options = defuddle::DefuddleOptions {
        url,
        debug,
        separate_markdown: true,
        language: lang,
        ..Default::default()
    };

    let result = defuddle::parse_html(&html, options)
        .map_err(|e| format!("Parse error: {e}"))?;

    let out: Box<dyn Write> = if let Some(ref path) = output {
        Box::new(fs::File::create(path).map_err(|e| format!("Cannot open output file: {e}"))?)
    } else {
        Box::new(io::stdout())
    };

    if let Some(ref prop) = property {
        let value = match prop.as_str() {
            "title" => &result.title,
            "author" => &result.author,
            "description" => &result.description,
            "domain" => &result.domain,
            "favicon" => &result.favicon,
            "image" => &result.image,
            "language" => &result.language,
            "published" => &result.published,
            "site" => &result.site,
            "content" => &result.content,
            other => return Err(format!("unknown property: {other}")),
        };
        let mut out = out;
        writeln!(out, "{value}").map_err(|e| e.to_string())?;
        return Ok(());
    }

    if json_output {
        let mut out = out;
        let json = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
        writeln!(out, "{json}").map_err(|e| e.to_string())?;
        return Ok(());
    }

    if frontmatter {
        let mut out = out;
        writeln!(out, "---").map_err(|e| e.to_string())?;
        writeln!(out, "title: {}", &result.title).map_err(|e| e.to_string())?;
        writeln!(out, "author: {}", &result.author).map_err(|e| e.to_string())?;
        if !result.description.is_empty() {
            writeln!(out, "description: {}", &result.description).map_err(|e| e.to_string())?;
        }
        writeln!(out, "site: {}", &result.site).map_err(|e| e.to_string())?;
        if !result.published.is_empty() {
            writeln!(out, "published: {}", &result.published).map_err(|e| e.to_string())?;
        }
        writeln!(out, "language: {}", &result.language).map_err(|e| e.to_string())?;
        writeln!(out, "domain: {}", &result.domain).map_err(|e| e.to_string())?;
        writeln!(out, "---").map_err(|e| e.to_string())?;
        if markdown {
            if let Some(ref md) = result.content_markdown {
                write!(out, "{md}").map_err(|e| e.to_string())?;
            }
        }
        return Ok(());
    }

    let mut out = out;
    if markdown {
        if let Some(ref md) = result.content_markdown {
            write!(out, "{md}").map_err(|e| e.to_string())?;
        }
    } else {
        write!(out, "{}", result.content).map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn read_input(source: &Option<String>) -> Result<String, String> {
    match source {
        Some(path) if path == "-" || path.is_empty() => {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| format!("Cannot read stdin: {e}"))?;
            Ok(buf)
        }
        Some(path) => {
            fs::read_to_string(path)
                .map_err(|e| format!("Cannot read file {path}: {e}"))
        }
        None => {
            if atty::is(atty::Stream::Stdin) {
                return Err("No input provided. Pass an HTML file path, `-` for stdin, or pipe HTML to stdin.".to_string());
            }
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| format!("Cannot read stdin: {e}"))?;
            Ok(buf)
        }
    }
}

fn print_help() {
    eprintln!(
        r#"defuddle - HTML article extraction

Usage: defuddle [OPTIONS] [SOURCE]

SOURCE:
  <file>        Path to an HTML file
  -             Read HTML from stdin
  (omitted)     Read HTML from stdin if not a TTY

Options:
  -m, --markdown, --md   Output Markdown instead of HTML
  -j, --json              Output JSON with metadata and content
  -f, --frontmatter       Output YAML frontmatter + Markdown
  -p, --property <name>   Output a single metadata property
  -o, --output <file>     Write output to file instead of stdout
  --url <url>             Set document URL for metadata resolution
  -l, --lang <code>       Preferred language (BCP 47 tag)
  --debug                 Enable debug output
  -h, --help              Show this help
"#
    );
}
