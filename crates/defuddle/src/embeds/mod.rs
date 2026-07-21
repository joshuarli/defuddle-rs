pub(crate) mod twitter;
pub(crate) mod youtube;

pub(crate) fn iframe_target_url(src: &str) -> Option<String> {
    youtube::target_url(src).or_else(|| twitter::target_url(src))
}

pub(crate) fn iframe_to_markdown(src: &str) -> Option<String> {
    iframe_target_url(src).map(|url| format!("[]({url})"))
}
