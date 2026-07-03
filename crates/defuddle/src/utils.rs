pub fn count_words(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }

    let mut cjk_count = 0;
    let mut word_count = 0;
    let mut in_word = false;

    for ch in text.chars() {
        match ch {
            '\u{3040}'..='\u{309F}' | // Hiragana
            '\u{30A0}'..='\u{30FF}' | // Katakana
            '\u{3400}'..='\u{4DBF}' | // CJK Extension A
            '\u{4E00}'..='\u{9FFF}' | // CJK Unified Ideographs
            '\u{F900}'..='\u{FAFF}' | // CJK Compatibility Ideographs
            '\u{AC00}'..='\u{D7AF}'   // Korean Hangul
            => {
                cjk_count += 1;
                in_word = false;
            }
            c if c == '\u{0000}' || (c <= ' ' && c != '\0') => {
                in_word = false;
            }
            _ => {
                if !in_word {
                    word_count += 1;
                    in_word = true;
                }
            }
        }
    }

    cjk_count + word_count
}

/// Canonicalize text for title/heading comparison: normalize smart quotes,
/// dashes, ellipses, and whitespace; lowercase.
pub fn normalize_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\u{00A0}' => result.push(' '),
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => result.push('\''),
            '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' => result.push('-'),
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => result.push('"'),
            '\u{2026}' => result.push_str("..."),
            _ => result.push(ch),
        }
    }

    let mut out = String::with_capacity(result.len());
    let mut prev_space = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }

    out.trim().to_lowercase()
}

/// Decode common HTML entities in a string.
pub fn decode_html_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();

    while i < chars.len() {
        if chars[i] == '&' {
            if let Some(semi) = chars[i..].iter().position(|&c| c == ';') {
                let entity = &chars[i..i + semi + 1];
                match entity.iter().collect::<String>().as_str() {
                    "&amp;" => {
                        result.push('&');
                        i += 5;
                        continue;
                    }
                    "&lt;" => {
                        result.push('<');
                        i += 4;
                        continue;
                    }
                    "&gt;" => {
                        result.push('>');
                        i += 4;
                        continue;
                    }
                    "&quot;" => {
                        result.push('"');
                        i += 6;
                        continue;
                    }
                    "&apos;" => {
                        result.push('\'');
                        i += 6;
                        continue;
                    }
                    "&nbsp;" => {
                        result.push(' ');
                        i += 6;
                        continue;
                    }
                    ent if ent.starts_with("&#x") => {
                        if let Ok(codepoint) =
                            u32::from_str_radix(&ent[3..ent.len() - 1].to_lowercase(), 16)
                            && let Some(c) = char::from_u32(codepoint)
                        {
                            result.push(c);
                        }
                        i += ent.len();
                        continue;
                    }
                    ent if ent.starts_with("&#") => {
                        if let Ok(codepoint) = ent[2..ent.len() - 1].parse::<u32>()
                            && let Some(c) = char::from_u32(codepoint)
                        {
                            result.push(c);
                        }
                        i += ent.len();
                        continue;
                    }
                    _ => {
                        result.push('&');
                        i += 1;
                        continue;
                    }
                }
            } else {
                result.push('&');
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Escape HTML special characters.
pub fn escape_html(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            _ => result.push(ch),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_words_english() {
        assert_eq!(count_words("hello world"), 2);
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("one"), 1);
        assert_eq!(count_words("  hello   world  "), 2);
    }

    #[test]
    fn test_count_words_cjk() {
        assert_eq!(count_words("日本語のテスト"), 7);
        assert_eq!(count_words("你好世界"), 4);
        assert_eq!(count_words("Hello 你好 World"), 4);
    }

    #[test]
    fn test_normalize_text() {
        let result = normalize_text("Hello World\u{2014}A Test\u{00A0}Case");
        assert_eq!(result, "hello world-a test case");

        let result = normalize_text("\u{201C}quoted\u{201D}");
        assert_eq!(result, "\"quoted\"");

        let result = normalize_text("Title\u{2026}");
        assert_eq!(result, "title...");
    }

    #[test]
    fn test_decode_html_entities() {
        assert_eq!(decode_html_entities("&amp;"), "&");
        assert_eq!(decode_html_entities("&lt;div&gt;"), "<div>");
        assert_eq!(decode_html_entities("&quot;hello&quot;"), "\"hello\"");
        assert_eq!(decode_html_entities("&apos;test&apos;"), "'test'");
        assert_eq!(decode_html_entities("&nbsp;space"), " space");
        assert_eq!(decode_html_entities("&#60;"), "<");
        assert_eq!(decode_html_entities("&#x3E;"), ">");
        assert_eq!(decode_html_entities("no entities"), "no entities");
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<div>"), "&lt;div&gt;");
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html("\"quoted\""), "&quot;quoted&quot;");
    }
}
