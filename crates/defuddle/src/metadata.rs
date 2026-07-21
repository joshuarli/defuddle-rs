use std::sync::OnceLock;

use miniserde::json::Value;

use crate::dom::{Document, NodeId};
use crate::schema::{as_str, is_container};
use crate::utils::count_words;
use rustc_hash::FxHashSet;

type MetaTags = [(Option<String>, Option<String>, Option<String>)];

static SEPARATOR_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static DASH_SEPARATOR_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static DAY_MONTH_YEAR_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static MONTH_DAY_YEAR_RE: OnceLock<regex_lite::Regex> = OnceLock::new();
static AUTHOR_URL_RE: OnceLock<regex_lite::Regex> = OnceLock::new();

#[derive(Default)]
pub struct Metadata {
    pub title: String,
    pub description: String,
    pub domain: String,
    pub favicon: String,
    pub image: String,
    pub language: String,
    pub published: String,
    pub author: String,
    pub site: String,
}

pub fn extract_metadata(
    doc: &Document,
    schema_org_data: &[Value],
    meta_tags: &MetaTags,
    url: &str,
) -> Metadata {
    let (domain, resolved_url) = extract_domain_and_url(doc, meta_tags, schema_org_data, url);
    let site_name = get_site_name(schema_org_data, meta_tags);
    let (title, detected_site_name) = clean_title(
        &get_best_title(doc, schema_org_data, meta_tags, &domain, &site_name),
        &site_name,
    );
    let author = get_author(doc, schema_org_data, meta_tags);
    let author_as_site = if !author.is_empty() && !author.contains(',') {
        author.as_str()
    } else {
        ""
    };
    let has_canonical = !doc.query_selector_all("link[rel=\"canonical\"]").is_empty();
    let site = if !site_name.is_empty() {
        site_name
    } else if !detected_site_name.is_empty() {
        detected_site_name
    } else if !author_as_site.is_empty() {
        author_as_site.to_string()
    } else if domain.starts_with("research.") {
        domain.clone()
    } else if has_canonical {
        domain.clone()
    } else {
        String::new()
    };

    Metadata {
        title,
        description: get_description(schema_org_data, meta_tags),
        domain,
        favicon: get_favicon(doc, &resolved_url, meta_tags),
        image: get_image(schema_org_data, meta_tags),
        language: get_language(doc, schema_org_data, meta_tags),
        published: get_published(doc, schema_org_data, meta_tags, &resolved_url),
        author,
        site,
    }
}

fn extract_domain_and_url(
    doc: &Document,
    meta_tags: &MetaTags,
    schema_org_data: &[Value],
    default_url: &str,
) -> (String, String) {
    let url = if !default_url.is_empty() {
        default_url.to_string()
    } else {
        get_meta_content(meta_tags, "property", "og:url")
            .or_else(|| get_meta_content(meta_tags, "property", "twitter:url"))
            .or_else(|| get_schema_property_str(schema_org_data, "url"))
            .or_else(|| get_schema_property_str(schema_org_data, "mainEntityOfPage.url"))
            .or_else(|| get_schema_property_str(schema_org_data, "mainEntity.url"))
            .or_else(|| get_schema_property_str(schema_org_data, "WebSite.url"))
            .or_else(|| {
                let canonicals = doc.query_selector_all("link[rel=\"canonical\"]");
                canonicals
                    .first()
                    .and_then(|id| doc.get_attr(*id, "href"))
                    .map(|s| s.to_string())
            })
            .unwrap_or_default()
    };

    let domain = if !url.is_empty() {
        crate::utils::extract_domain(&url)
    } else {
        String::new()
    };

    // Fall back to base tag
    if domain.is_empty() {
        let base_tags = doc.query_selector_all("base[href]");
        if let Some(base_id) = base_tags.first()
            && let Some(base_href) = doc.get_attr(*base_id, "href")
        {
            return (crate::utils::extract_domain(base_href), base_href.to_string());
        }
    }

    (domain, url)
}

fn get_meta_content(meta_tags: &MetaTags, attr: &str, value: &str) -> Option<String> {
    meta_tags
        .iter()
        .find(|(name, property, content)| {
            let attr_val = if attr == "name" {
                name.as_deref()
            } else {
                property.as_deref()
            };
            attr_val.is_some_and(|av| av.eq_ignore_ascii_case(value)) && content.is_some()
        })
        .and_then(|(_, _, content)| content.clone())
}

fn get_meta_contents(meta_tags: &MetaTags, attr: &str, value: &str) -> Vec<String> {
    meta_tags
        .iter()
        .filter_map(|(name, property, content)| {
            let attr_val = if attr == "name" {
                name.as_deref()
            } else {
                property.as_deref()
            };
            if attr_val.is_some_and(|av| av.eq_ignore_ascii_case(value)) {
                content.clone()
            } else {
                None
            }
        })
        .collect()
}

fn is_placeholder_value(s: &str) -> bool {
    if s.contains('{') || s.contains('}') || s.starts_with('#') {
        return true;
    }
    !s.chars().any(|c| c.is_alphanumeric())
}

fn first_valid<T: Into<String>>(results: Vec<Option<T>>) -> String {
    for v in results.into_iter().flatten() {
        let s: String = v.into();
        if !s.is_empty() && !is_placeholder_value(&s) {
            return s;
        }
    }
    String::new()
}

fn get_site_name(schema_org_data: &[Value], meta_tags: &MetaTags) -> String {
    let candidate = first_valid(vec![
        get_schema_property_str(schema_org_data, "publisher.name"),
        get_meta_content(meta_tags, "property", "og:site_name"),
        get_meta_content(meta_tags, "name", "og:site_name"),
        get_schema_property_str(schema_org_data, "WebSite.name"),
        get_schema_property_str(schema_org_data, "sourceOrganization.name"),
        get_meta_content(meta_tags, "name", "copyright"),
        get_schema_property_str(schema_org_data, "copyrightHolder.name"),
        get_schema_property_str(schema_org_data, "isPartOf.name"),
        get_meta_content(meta_tags, "name", "application-name"),
    ]);

    if count_words(&candidate) > 6 {
        return String::new();
    }

    candidate
}

fn get_schema_property_str(data: &[Value], property: &str) -> Option<String> {
    let results = get_schema_property(data, property);
    if results.is_empty() {
        None
    } else {
        Some(results.join(", "))
    }
}

fn get_schema_property(data: &[Value], property: &str) -> Vec<String> {
    let parts: Vec<&str> = property.split('.').collect();
    let mut results = Vec::new();
    for item in data {
        search_schema(item, &parts, "", true, &mut results);
    }
    if results.is_empty() {
        for item in data {
            search_schema(item, &parts, "", false, &mut results);
        }
    }
    let mut seen = FxHashSet::default();
    results.retain(|r| !r.is_empty() && seen.insert(r.clone()));
    results
}

fn search_schema(
    value: &Value,
    props: &[&str],
    _full_path: &str,
    is_exact: bool,
    results: &mut Vec<String>,
) {
    if props.is_empty() {
        match value {
            Value::String(s) => {
                results.push(s.clone());
            }
            Value::Object(obj) => {
                if let Some(name) = obj.get("name").and_then(as_str) {
                    results.push(name.to_string());
                }
            }
            _ => {}
        }
        return;
    }

    match value {
        Value::String(_) if props.is_empty() => {
            results.push(as_str(value).unwrap_or("").to_string());
        }
        Value::Array(arr) => {
            let current = props[0];
            if current.starts_with('[')
                && current.ends_with(']')
                && let Ok(idx) = current[1..current.len() - 1].parse::<usize>()
            {
                if let Some(item) = arr.get(idx) {
                    search_schema(item, &props[1..], _full_path, is_exact, results);
                }
                return;
            }
            for item in arr {
                search_schema(item, props, _full_path, is_exact, results);
            }
        }
        Value::Object(obj) => {
            let current = props[0];
            if let Some(child) = obj.get(current) {
                search_schema(child, &props[1..], current, true, results);
            } else if !is_exact {
                for val in obj.values() {
                    if is_container(val) {
                        search_schema(val, props, _full_path, false, results);
                    }
                }
            }
        }
        _ => {}
    }
}

fn get_best_title(
    doc: &Document,
    schema_org_data: &[Value],
    meta_tags: &MetaTags,
    domain: &str,
    site_name: &str,
) -> String {
    let candidates: Vec<String> = [
        get_meta_content(meta_tags, "property", "og:title"),
        get_meta_content(meta_tags, "name", "twitter:title"),
        get_schema_property_str(schema_org_data, "headline"),
        get_meta_content(meta_tags, "name", "title"),
        get_meta_content(meta_tags, "name", "sailthru.title"),
        doc.query_selector_all("title")
            .first()
            .map(|id| doc.text_content(*id).trim().to_string()),
        doc.query_selector_all("h1")
            .first()
            .map(|id| doc.text_content(*id).trim().to_string()),
    ]
    .into_iter()
    .flatten()
    .filter(|c| !c.is_empty() && !is_placeholder_value(c))
    .collect();

    if candidates.is_empty() {
        return String::new();
    }

    if domain.eq_ignore_ascii_case("chatgpt.com")
        && let Some(title) = candidates
            .iter()
            .find(|candidate| candidate.eq_ignore_ascii_case("chatgpt"))
    {
        return title.clone();
    }

    let author_meta = get_meta_content(meta_tags, "property", "author")
        .or_else(|| get_meta_content(meta_tags, "name", "author"))
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default();

    let site_norm = site_name.trim().to_lowercase();
    let domain_norm: String = domain
        .rsplit_once('.')
        .map(|x| x.0)
        .unwrap_or(domain)
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();

    for candidate in &candidates {
        if !is_site_identifier(candidate, &author_meta, &site_norm, &domain_norm) {
            return candidate.clone();
        }
    }

    candidates.into_iter().next().unwrap_or_default()
}

fn is_site_identifier(
    candidate: &str,
    author_norm: &str,
    site_norm: &str,
    domain_norm: &str,
) -> bool {
    let norm = candidate.trim().to_lowercase();

    if !author_norm.is_empty() && norm == author_norm {
        return true;
    }
    if !site_norm.is_empty() && norm == site_norm {
        return true;
    }
    if !domain_norm.is_empty() {
        let candidate_norm: String = norm.chars().filter(|c| c.is_alphanumeric()).collect();
        if candidate_norm == domain_norm {
            return true;
        }
    }
    false
}

fn clean_title(title: &str, site_name: &str) -> (String, String) {
    if title.is_empty() {
        return (String::new(), String::new());
    }

    let title = title.trim();
    let separators = "|\\-–—/·";

    if !site_name.is_empty()
        && site_name.to_lowercase() != title.to_lowercase()
        && count_words(site_name) <= 6
    {
        let _site_name_lower = site_name.to_lowercase();

        let escaped = regex_lite::escape(site_name);
        let patterns = [
            format!(r"\s*[{}]\s*{}\s*$", separators, escaped),
            format!(r"^\s*{}\s*[{}]\s*", escaped, separators),
        ];

        for pattern in &patterns {
            let re = regex_lite::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .unwrap();

            if re.is_match(title) {
                let cleaned = re.replace(title, "").trim().to_string();
                return (cleaned, site_name.to_string());
            }
        }
    }

    let (result, detected) = try_separator_split(title, separators, false);
    if let Some(detected) = detected {
        return (result, detected);
    }

    (title.to_string(), String::new())
}

fn try_separator_split(
    title: &str,
    _separators: &str,
    _suffix_only: bool,
) -> (String, Option<String>) {
    let sep_re = SEPARATOR_RE.get_or_init(|| regex_lite::Regex::new(r"\s+[-–—|/·]\s+").unwrap());

    let positions: Vec<(usize, usize)> = sep_re
        .find_iter(title)
        .map(|m| (m.start(), m.end()))
        .collect();

    if positions.is_empty() {
        return (String::new(), None);
    }

    if let Some(&(start, end)) = positions.last() {
        let suffix_title = title[..start].trim();
        let suffix_site = title[end..].trim();
        let stw = count_words(suffix_title);
        let ssw = count_words(suffix_site);
        if ssw <= 3 && stw >= 2 && stw >= ssw * 2 {
            return (suffix_title.to_string(), Some(suffix_site.to_string()));
        }
    }

    if !_suffix_only && let Some(&(start, end)) = positions.first() {
        let prefix_site = title[..start].trim();
        let prefix_title = title[end..].trim();
        let ptw = count_words(prefix_title);
        let psw = count_words(prefix_site);
        if psw <= 3 && ptw >= 2 && ptw >= psw * 2 {
            return (prefix_title.to_string(), Some(prefix_site.to_string()));
        }
    }

    // Dash-only suffix check with stricter guards
    let dash_re = DASH_SEPARATOR_RE.get_or_init(|| regex_lite::Regex::new(r"\s+[-–—]\s+").unwrap());
    if let Some(m) = dash_re.find_iter(title).last() {
        let suffix_title = title[..m.start()].trim();
        let suffix_site = title[m.end()..].trim();
        let stw = count_words(suffix_title);
        let ssw = count_words(suffix_site);
        if ssw <= 2 && stw >= 2 && stw > ssw {
            return (suffix_title.to_string(), Some(suffix_site.to_string()));
        }
    }

    (String::new(), None)
}

fn get_description(schema_org_data: &[Value], meta_tags: &MetaTags) -> String {
    first_valid(vec![
        get_meta_content(meta_tags, "name", "description"),
        get_meta_content(meta_tags, "property", "description"),
        get_meta_content(meta_tags, "property", "og:description"),
        get_schema_property_str(schema_org_data, "description"),
        get_meta_content(meta_tags, "name", "twitter:description"),
        get_meta_content(meta_tags, "name", "sailthru.description"),
    ])
}

fn get_image(schema_org_data: &[Value], meta_tags: &MetaTags) -> String {
    get_meta_content(meta_tags, "property", "og:image")
        .or_else(|| get_meta_content(meta_tags, "name", "twitter:image"))
        .or_else(|| get_schema_property_str(schema_org_data, "image.url"))
        .or_else(|| get_meta_content(meta_tags, "name", "sailthru.image.full"))
        .unwrap_or_default()
}

fn get_language(doc: &Document, schema_org_data: &[Value], meta_tags: &MetaTags) -> String {
    let html_ids = doc.query_selector_all("html");
    if let Some(html_id) = html_ids.first()
        && let Some(lang) = doc.get_attr(*html_id, "lang")
        && !lang.trim().is_empty()
    {
        return normalize_lang_code(lang.trim());
    }

    let content_lang = get_meta_content(meta_tags, "name", "content-language")
        .or_else(|| get_meta_content(meta_tags, "property", "og:locale"));
    if let Some(lang) = content_lang {
        return normalize_lang_code(&lang);
    }

    let http_equiv = doc.query_selector_all("meta[http-equiv=\"Content-Language\" i]");
    if let Some(meta_id) = http_equiv.first()
        && let Some(content) = doc.get_attr(*meta_id, "content")
    {
        return normalize_lang_code(content.trim());
    }

    let schema_lang = get_schema_property_str(schema_org_data, "inLanguage");
    if let Some(lang) = schema_lang {
        return normalize_lang_code(&lang);
    }

    String::new()
}

fn normalize_lang_code(code: &str) -> String {
    code.replace('_', "-")
}

fn get_favicon(doc: &Document, base_url: &str, meta_tags: &MetaTags) -> String {
    if let Some(icon) = get_meta_content(meta_tags, "property", "og:image:favicon") {
        return icon;
    }

    let icon_links = doc.query_selector_all("link[rel='icon']");
    if let Some(id) = icon_links.first()
        && let Some(href) = doc.get_attr(*id, "href")
    {
        return href.to_string();
    }

    let shortcut_links = doc.query_selector_all("link[rel='shortcut icon']");
    if let Some(id) = shortcut_links.first()
        && let Some(href) = doc.get_attr(*id, "href")
    {
        return href.to_string();
    }

    if base_url.starts_with("http") && !base_url.is_empty() {
        let base = if base_url.starts_with("http://") || base_url.starts_with("https://") {
            base_url.to_string()
        } else {
            format!("https://{}", base_url)
        };
        if let Some(slash) = base[8..].find('/') {
            let origin = &base[..8 + slash];
            return format!("{}/favicon.ico", origin);
        }
        return format!("{}/favicon.ico", base);
    }

    String::new()
}

fn get_published(
    doc: &Document,
    schema_org_data: &[Value],
    meta_tags: &MetaTags,
    url: &str,
) -> String {
    let result = first_valid(vec![
        get_schema_property_str(schema_org_data, "datePublished"),
        get_meta_content(meta_tags, "name", "publishDate"),
        get_meta_content(meta_tags, "property", "article:published_time"),
        doc.query_selector_all("abbr[itemprop=\"datePublished\"]")
            .first()
            .and_then(|id| doc.get_attr(*id, "title").map(|s| s.to_string())),
        get_time_element(doc, url),
        get_meta_content(meta_tags, "name", "sailthru.date"),
    ]);

    if !result.is_empty() {
        return result;
    }

    let h1_ids = doc.query_selector_all("h1");
    if let Some(h1_id) = h1_ids.first() {
        let found = first_valid(vec![
            scan_siblings_for_date(doc, *h1_id, true),
            scan_siblings_for_date(doc, *h1_id, false),
        ]);
        if !found.is_empty() {
            return found;
        }
    }

    String::new()
}

fn scan_siblings_for_date(doc: &Document, h1_id: NodeId, forward: bool) -> Option<String> {
    let mut sibling = if forward {
        doc.next_element_sibling(h1_id)
    } else {
        doc.prev_element_sibling(h1_id)
    };

    for _ in 0..3 {
        if let Some(id) = sibling {
            let text = doc.text_content(id).trim().to_string();
            if let Some(parsed) = parse_date_text(&text) {
                return Some(parsed);
            }

            for child_id in doc.children(id).to_vec() {
                if doc.tag_name_is_any(child_id, &["p", "time"]) {
                    let child_text = doc.text_content(child_id).trim().to_string();
                    if let Some(parsed) = parse_date_text(&child_text) {
                        return Some(parsed);
                    }
                }
            }

            sibling = if forward {
                doc.next_element_sibling(id)
            } else {
                doc.prev_element_sibling(id)
            };
        } else {
            break;
        }
    }

    None
}

fn get_time_element(doc: &Document, url: &str) -> Option<String> {
    let time_ids = doc.query_selector_all("time");
    for id in time_ids {
        if is_linked_to_other_page(doc, id, url) {
            continue;
        }
        if let Some(datetime) = doc.get_attr(id, "datetime") {
            let dt = datetime.trim().to_string();
            if !dt.is_empty() {
                return Some(dt);
            }
        }
        let text = doc.text_content(id).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

fn is_linked_to_other_page(doc: &Document, el: NodeId, page_url: &str) -> bool {
    if page_url.is_empty() {
        return false;
    }

    // The lightweight selector parser does not support attribute-presence
    // selectors reliably, so find the nearest anchor without `[href]`.
    let anchor = doc.closest(el, "a");
    if anchor.is_none() {
        return false;
    }
    let anchor = anchor.unwrap();

    let href = doc
        .get_attr(anchor, "href")
        .map(|s| s.to_string())
        .unwrap_or_default();
    if href.is_empty() || href.starts_with('#') {
        return false;
    }

    let target_url = resolve_url(&href, page_url);
    let current_url = page_url.to_string();

    let norm = |u: &str| u.trim_end_matches('/').to_string();

    if let (Ok(target), Ok(current)) = (parse_url(&target_url), parse_url(&current_url)) {
        if target.origin() != current.origin() {
            return false;
        }
        if norm(target.path()) != norm(current.path()) {
            return true;
        }
    }

    false
}

fn parse_url(url: &str) -> Result<ParsedUrl, ()> {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or(())?;

    let (host_port, path_rest) = stripped.split_once('/').unwrap_or((stripped, ""));

    let (host, _port) = host_port.split_once(':').unwrap_or((host_port, ""));
    let origin = format!("https://{}", host);

    Ok(ParsedUrl {
        origin,
        path: format!("/{}", path_rest),
    })
}

struct ParsedUrl {
    origin: String,
    path: String,
}

impl ParsedUrl {
    fn origin(&self) -> &str {
        &self.origin
    }
    fn path(&self) -> &str {
        &self.path
    }
}

fn resolve_url(href: &str, base: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        let without_query = href.split(['?', '#']).next().unwrap_or(href);
        if without_query.matches('/').count() == 2 && !href.contains(['?', '#']) {
            return format!("{href}/");
        }
        return href.to_string();
    }

    let base = base.trim_end_matches('/');
    if href.starts_with('/') {
        let origin_end = base
            .find("://")
            .map(|i| {
                base[i + 3..]
                    .find('/')
                    .map(|j| i + 3 + j)
                    .unwrap_or(base.len())
            })
            .unwrap_or(base.len());
        format!("{}{}", &base[..origin_end], href)
    } else {
        let base_dir = if let Some(slash) = base.rfind('/') {
            &base[..slash]
        } else {
            base
        };
        format!("{}/{}", base_dir, href)
    }
}

const MONTH_MAP: &[(&str, &str)] = &[
    ("january", "01"),
    ("february", "02"),
    ("march", "03"),
    ("april", "04"),
    ("may", "05"),
    ("june", "06"),
    ("july", "07"),
    ("august", "08"),
    ("september", "09"),
    ("october", "10"),
    ("november", "11"),
    ("december", "12"),
];

pub(crate) fn parse_date_text(text: &str) -> Option<String> {
    // "26 February 2025" or "Wednesday, 26 February 2025"
    let re1 = DAY_MONTH_YEAR_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)\b(\d{1,2})\s+(January|February|March|April|May|June|July|August|September|October|November|December)\s+(\d{4})\b"
        ).unwrap()
    });
    if let Some(caps) = re1.captures(text) {
        let day = format!("{:02}", caps[1].parse::<u32>().unwrap_or(0));
        let month = get_month_number(&caps[2]);
        return Some(format!("{}-{}-{}T00:00:00+00:00", &caps[3], month, day));
    }

    // "February 26, 2025"
    let re2 = MONTH_DAY_YEAR_RE.get_or_init(|| {
        regex_lite::Regex::new(
            r"(?i)\b(January|February|March|April|May|June|July|August|September|October|November|December)\s+(\d{1,2}),?\s+(\d{4})\b"
        ).unwrap()
    });
    if let Some(caps) = re2.captures(text) {
        let month = get_month_number(&caps[1]);
        let day = format!("{:02}", caps[2].parse::<u32>().unwrap_or(0));
        return Some(format!("{}-{}-{}T00:00:00+00:00", &caps[3], month, day));
    }

    None
}

fn get_month_number(name: &str) -> &str {
    let lower = name.to_lowercase();
    MONTH_MAP
        .iter()
        .find(|(month, _)| lower == *month)
        .map(|(_, num)| *num)
        .unwrap_or("01")
}

fn get_author(doc: &Document, schema_org_data: &[Value], meta_tags: &MetaTags) -> String {
    let author_str = first_valid(vec![
        get_meta_content(meta_tags, "name", "sailthru.author"),
        get_meta_content(meta_tags, "property", "article:author"),
        get_meta_content(meta_tags, "property", "author"),
        get_meta_content(meta_tags, "name", "author"),
        get_meta_content(meta_tags, "name", "byl"),
        get_meta_content(meta_tags, "name", "authorList"),
    ]);

    if !author_str.is_empty() {
        let cleaned = clean_author_string(&author_str);
        if !cleaned.is_empty() {
            return cleaned;
        }
    }

    let citation_authors = get_meta_contents(meta_tags, "name", "citation_author")
        .into_iter()
        .filter(|s| !is_placeholder_value(s))
        .collect::<Vec<_>>();
    if !citation_authors.is_empty() {
        let formatted: Vec<String> = citation_authors
            .into_iter()
            .map(|s| {
                if s.contains(',')
                    && let Some((last, first)) = s.split_once(',')
                {
                    return format!("{} {}", first.trim(), last.trim());
                }
                s
            })
            .collect();
        return formatted.join(", ");
    }

    let dc_authors = get_meta_contents(meta_tags, "property", "dc.creator")
        .into_iter()
        .filter(|s| !is_placeholder_value(s))
        .collect::<Vec<_>>();
    if !dc_authors.is_empty() {
        return dc_authors.join(", ");
    }

    let schema_authors = get_schema_property_str(schema_org_data, "author.name");
    if let Some(authors) = schema_authors {
        let parts: Vec<String> = authors
            .split(',')
            .map(|p| p.trim().trim_end_matches(',').to_string())
            .filter(|p| !p.is_empty() && !is_placeholder_value(p))
            .collect();
        if !parts.is_empty() {
            let mut seen = FxHashSet::default();
            let unique: Vec<String> = parts
                .into_iter()
                .filter(|p| seen.insert(p.clone()))
                .take(10)
                .collect();
            return unique.join(", ");
        }
    }

    get_author_from_dom(doc, meta_tags)
}

fn get_author_from_dom(doc: &Document, meta_tags: &MetaTags) -> String {
    let rel_author_ids = doc.query_selector_all("a[rel~=\"author\"], address[rel~=\"author\"]");
    if !rel_author_ids.is_empty() && rel_author_ids.len() <= 3 {
        let mut names = Vec::new();
        for id in &rel_author_ids {
            let text = get_visible_text(doc, *id);
            let lower = text.to_lowercase();
            if !text.is_empty()
                && text.len() < 100
                && lower != "author"
                && lower != "authors"
                && !is_placeholder_value(&text)
            {
                names.push(text);
            }
        }
        use rustc_hash::FxHashSet;
        let unique: FxHashSet<_> = names.into_iter().collect();
        let unique: Vec<_> = unique.into_iter().collect();
        if !unique.is_empty() {
            return unique.join(", ");
        }
    }

    let mut collected = Vec::new();
    let mut add = |value: &str| {
        for part in value.split(',') {
            let cleaned = part.trim().trim_end_matches(',').trim();
            let lower = cleaned.to_lowercase();
            if !cleaned.is_empty()
                && lower != "author"
                && lower != "authors"
                && !is_placeholder_value(cleaned)
            {
                collected.push(cleaned.to_string());
            }
        }
    };

    let itemprop_ids = doc.query_selector_all("[itemprop=\"author\"]");
    for id in itemprop_ids {
        add(&get_author_name(doc, id));
    }

    let author_class_ids = doc.query_selector_all(".author");
    if author_class_ids.len() <= 3 {
        for id in author_class_ids {
            add(&get_author_name(doc, id));
        }
    }

    let author_href_ids = doc.query_selector_all("[href*=\"/author/\"]");
    if author_href_ids.len() <= 3 {
        for id in author_href_ids {
            add(&get_author_name(doc, id));
        }
    }

    let authors_a_ids = doc.query_selector_all(".authors a");
    if authors_a_ids.len() <= 3 {
        for id in authors_a_ids {
            add(&get_author_name(doc, id));
        }
    }

    if !collected.is_empty() {
        let mut seen = FxHashSet::default();
        let mut unique: Vec<String> = collected
            .into_iter()
            .filter(|a| {
                if seen.contains(a) {
                    return false;
                }
                seen.insert(a.clone());
                true
            })
            .collect();

        if unique.len() > 1 {
            let set_copy: FxHashSet<String> = unique.iter().cloned().collect();
            unique.retain(|a| !set_copy.iter().any(|b| b != a && a.contains(b)));
        }

        unique.truncate(10);
        if !unique.is_empty() {
            return unique.join(", ");
        }
    }

    // Author near h1
    let h1_ids = doc.query_selector_all("h1");
    if let Some(h1_id) = h1_ids.first() {
        let author_meta_str = get_meta_content(meta_tags, "property", "author")
            .or_else(|| get_meta_content(meta_tags, "name", "author"))
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_default();

        let author_norm = author_meta_str.trim().to_lowercase();
        if let Some(result) = find_author_near_h1(doc, *h1_id, &author_norm) {
            return result;
        }
    }

    String::new()
}

fn find_author_near_h1(doc: &Document, h1_id: NodeId, _author_norm: &str) -> Option<String> {
    // Check siblings of h1 for date-adjacent author names
    let mut sibling = doc.next_element_sibling(h1_id);
    for _ in 0..3 {
        if let Some(id) = sibling {
            let text = doc.text_content(id).trim().to_string();
            let child_has_date = doc
                .query_selector_all_in_subtree(id, "p,time")
                .iter()
                .any(|child| parse_date_text(&doc.text_content(*child)).is_some());
            if parse_date_text(&text).is_some() || child_has_date {
                let links = doc.query_selector_all("a");
                let contained_links: Vec<_> =
                    links.into_iter().filter(|l| doc.contains(id, *l)).collect();
                if contained_links.len() == 1 {
                    let link_text = doc.text_content(contained_links[0]).trim().to_string();
                    if !link_text.is_empty()
                        && link_text.len() < 100
                        && parse_date_text(&link_text).is_none()
                    {
                        return Some(link_text);
                    }
                }
                for child in doc.children(id).to_vec() {
                    if doc.tag_name(child).eq_ignore_ascii_case("p") {
                        let child_text = doc.text_content(child).trim().to_string();
                        if !child_text.is_empty() && parse_date_text(&child_text).is_none() {
                            return Some(child_text);
                        }
                    }
                }
            }
            sibling = doc.next_element_sibling(id);
        } else {
            break;
        }
    }

    // Search for "By ..." bylines near h1: check siblings of h1
    // and siblings of its ancestor containers (up to 3 levels)
    let mut byline_scope = Some(h1_id);
    for _ in 0..3 {
        if let Some(id) = byline_scope {
            let mut prev = doc.prev_element_sibling(id);
            for _ in 0..3 {
                if let Some(pid) = prev {
                    if let Some(byline) = extract_byline(doc, pid) {
                        return Some(byline);
                    }
                    prev = doc.prev_element_sibling(pid);
                } else {
                    break;
                }
            }

            let mut next = doc.next_element_sibling(id);
            for _ in 0..3 {
                if let Some(nid) = next {
                    if let Some(byline) = extract_byline(doc, nid) {
                        return Some(byline);
                    }
                    next = doc.next_element_sibling(nid);
                } else {
                    break;
                }
            }

            byline_scope = doc.parent(id);
        } else {
            break;
        }
    }

    None
}

fn extract_byline(doc: &Document, el: NodeId) -> Option<String> {
    let text = doc.text_content(el).trim().to_string();
    if !text.is_empty() && text.len() < 50 {
        if let Some(author) = text.strip_prefix("By ") {
            let author = author.trim();
            if !author.is_empty() && author.chars().next().map_or(false, |c| c.is_uppercase()) {
                return Some(author.to_string());
            }
        }
        if let Some(author) = text.strip_prefix("by ") {
            let author = author.trim();
            if !author.is_empty() && author.chars().next().map_or(false, |c| c.is_uppercase()) {
                return Some(author.to_string());
            }
        }
    }
    None
}

fn clean_author_string(s: &str) -> String {
    let s = s.trim();

    let s = s.strip_prefix("By ").unwrap_or(s);
    let s = s.strip_prefix("by ").unwrap_or(s);
    let s = s.trim();

    // Remove URLs (including surrounding parentheses) — ported from upstream
    let url_re = AUTHOR_URL_RE.get_or_init(|| regex_lite::Regex::new(r"\(?\s*https?://\S+\s*\)?").unwrap());
    let s = url_re.replace_all(s, "").to_string();

    if s.starts_with("https://") || s.starts_with("http://") {
        return s.to_string();
    }

    s.replace(" and ", ", ")
        .trim_end_matches(&['-', '\u{2013}', '\u{2014}', '|'][..])
        .trim()
        .to_string()
}

fn get_visible_text(doc: &Document, node: NodeId) -> String {
    doc.text_content(node)
        .replace(char::is_whitespace, " ")
        .trim()
        .to_string()
}

fn get_author_name(doc: &Document, node: NodeId) -> String {
    let text = get_visible_text(doc, node);
    if text.is_empty() {
        return String::new();
    }
    // Prefer a short child span/a/p
    for child in doc.children(node) {
        let child_text = doc.text_content(*child).trim().to_string();
        if child_text.len() >= 2 && child_text.len() <= 50 && child_text != text {
            return child_text;
        }
    }
    if text.len() <= 100 {
        text
    } else {
        String::new()
    }
}
