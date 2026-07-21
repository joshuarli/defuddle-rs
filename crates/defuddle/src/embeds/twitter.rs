pub(crate) fn target_url(src: &str) -> Option<String> {
    if src
        .to_ascii_lowercase()
        .starts_with("https://platform.twitter.com/embed")
    {
        let id = src
            .split("id=")
            .nth(1)
            .and_then(|value| value.split('&').next())
            .filter(|id| !id.is_empty())?;
        return Some(format!("https://x.com/i/status/{id}"));
    }

    let lower = src.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("https://x.com/") {
        let mut parts = rest.split('/');
        if let (Some(user), Some("status"), Some(status)) =
            (parts.next(), parts.next(), parts.next())
        {
            return Some(format!("https://x.com/{user}/status/{status}"));
        }
    }

    None
}
