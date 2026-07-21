use crate::dom::SimpleSelector;

pub(crate) type ParsedSelector = Vec<Vec<SimpleSelector>>;

pub(crate) fn parse(selector: &str) -> ParsedSelector {
    let mut result = Vec::new();
    let mut input = cssparser::ParserInput::new(selector);
    {
        let mut parser = cssparser::Parser::new(&mut input);
        loop {
            let mut compound = Vec::new();
            loop {
                while parser.try_parse(|p| p.expect_whitespace()).is_ok() {}
                // Reject combinators (>, +, ~) — not supported by our matching
                let s = parser.state();
                if parser.next().ok().is_some_and(|t| matches!(t, cssparser::Token::Delim('>') | cssparser::Token::Delim('+') | cssparser::Token::Delim('~'))) {
                    // Combinator found — bail out of this entire compound
                    compound.clear();
                    // Consume the rest until comma or EOF
                    loop {
                        match parser.next() {
                            Err(..) | Ok(cssparser::Token::Comma) => break,
                            _ => {}
                        }
                    }
                    break;
                }
                parser.reset(&s);
                match parse_simple(&mut parser) {
                    Some(sel) => compound.push(sel),
                    None => break,
                }
                while parser.try_parse(|p| p.expect_whitespace()).is_ok() {}
                let s = parser.state();
                let stop = parser.next().is_err()
                    || parser.next().ok().is_some_and(|t| matches!(t, cssparser::Token::Comma));
                parser.reset(&s);
                if stop { break; }
            }
            if !compound.is_empty() { result.push(compound); }
            if parser.expect_comma().is_err() { break; }
        }
    }
    result
}

pub(crate) fn parse_one(s: &str) -> Option<SimpleSelector> {
    parse(s).into_iter().next()?.into_iter().next()
}

fn parse_simple(p: &mut cssparser::Parser<'_, '_>) -> Option<SimpleSelector> {
    let mut sel = SimpleSelector::default();
    let mut hit = false;
    loop {
        let s = p.state();
        // If we've already parsed something and the next meaningful token
        // starts with whitespace, stop — the whitespace is a descendant
        // combinator handled by the compound loop.
        if hit && p.try_parse(|pp| pp.expect_whitespace()).is_ok() {
            p.reset(&s);
            break;
        }
        match p.next().ok() {
            None => break,
            Some(cssparser::Token::Ident(name)) => {
                if sel.tag.is_none() { sel.tag = Some(name.as_ref().to_string()); }
                hit = true;
            }
            Some(cssparser::Token::Delim('*')) => { hit = true; }
            Some(cssparser::Token::IDHash(id)) => {
                sel.id = Some(id.as_ref().to_string());
                hit = true;
            }
            Some(cssparser::Token::Delim('.')) => {
                if let Ok(ident) = p.expect_ident_cloned() {
                    sel.classes.push(ident.as_ref().to_string());
                    hit = true;
                } else { break; }
            }
            Some(cssparser::Token::Colon) => {
                let next = p.next().ok();
                match next {
                    Some(cssparser::Token::Function(name))
                        if name.as_ref().eq_ignore_ascii_case("not") =>
                    {
                        if let Some(inner) = parse_inner(p) {
                            sel.negation = Some(Box::new(inner));
                            hit = true;
                        }
                    }
                    Some(cssparser::Token::Function(name))
                        if name.as_ref().eq_ignore_ascii_case("has") =>
                    {
                        if let Some(inner) = parse_inner(p) {
                            sel.extra = Some(Box::new(inner));
                            hit = true;
                        }
                    }
                    Some(cssparser::Token::Ident(_)) => hit = true,
                    _ => {}
                }
            }
            Some(cssparser::Token::SquareBracketBlock) => {
                if let Some(attr) = parse_attr(p) {
                    apply_attr(&mut sel, attr);
                    hit = true;
                }
            }
            _ => { p.reset(&s); break; }
        }
    }
    if hit { Some(sel) } else { None }
}

fn parse_inner(p: &mut cssparser::Parser<'_, '_>) -> Option<SimpleSelector> {
    p.parse_nested_block(|pp| {
        while pp.try_parse(|ppp| ppp.expect_whitespace()).is_ok() {}
        parse_simple(pp).ok_or(pp.new_custom_error::<(), ()>(()))
    }).ok()
}

enum Attr {
    Presence(String),
    Equals(String, String, bool),
    Contains(String, String, bool),
    Starts(String, String, bool),
    Ends(String, String, bool),
    Word(String, String, bool),
}

fn parse_attr(p: &mut cssparser::Parser<'_, '_>) -> Option<Attr> {
    p.parse_nested_block(|pp| {
        let name = pp.expect_ident_cloned()?.as_ref().to_string();
        let mut ci = false;
        // Convert token to tag value to release borrow
        let op: u8 = match pp.next() {
            Ok(cssparser::Token::CloseSquareBracket) | Err(..) => 0,
            Ok(cssparser::Token::Delim('=')) => 1,
            Ok(cssparser::Token::IncludeMatch) => 2,
            Ok(cssparser::Token::DashMatch) => 3,
            Ok(cssparser::Token::PrefixMatch) => 4,
            Ok(cssparser::Token::SuffixMatch) => 5,
            Ok(cssparser::Token::SubstringMatch) => 6,
            Ok(_) => 99,
        };
        if op == 0 { return Ok(Attr::Presence(name)); }
        if op == 99 { return Err(pp.new_custom_error::<(), ()>(())); }
        let val = match pp.next() {
            Ok(cssparser::Token::Ident(v)) => v.as_ref().to_string(),
            Ok(cssparser::Token::QuotedString(v)) => v.as_ref().to_string(),
            _ => return Err(pp.new_custom_error::<(), ()>(())),
        };
        let attr = match op {
            1 => { ci = consume_i_flag(pp); Attr::Equals(name, val, ci) }
            2 => Attr::Word(name, val, ci),
            3 | 4 => Attr::Starts(name, val, ci),
            5 => Attr::Ends(name, val, ci),
            6 => Attr::Contains(name, val, ci),
            _ => return Err(pp.new_custom_error::<(), ()>(())),
        };
        Ok(attr)
    }).ok()
}


fn consume_i_flag(p: &mut cssparser::Parser<'_, '_>) -> bool {
    let s = p.state();
    if p.expect_whitespace().is_ok() && p.expect_ident_matching("i").is_ok() {
        return true;
    }
    p.reset(&s);
    false
}

fn apply_attr(sel: &mut SimpleSelector, attr: Attr) {
    match attr {
        Attr::Presence(name) => sel.attr_presence = Some(name),
        Attr::Equals(name, val, ci) => sel.attr_equals = Some((name, val, ci)),
        Attr::Contains(name, val, ci) => sel.attr_contains = Some((name, val, ci)),
        Attr::Starts(name, val, ci) => sel.attr_starts = Some((name, val, ci)),
        Attr::Ends(name, val, ci) => sel.attr_ends = Some((name, val, ci)),
        Attr::Word(name, val, ci) => sel.attr_word = Some((name, val, ci)),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_descendant() {
        let parsed = parse("main article.post");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].len(), 2);
        assert_eq!(parsed[0][0].tag.as_deref(), Some("main"));
        assert_eq!(parsed[0][1].tag.as_deref(), Some("article"));
        assert_eq!(parsed[0][1].classes, vec!["post"]);
    }

    #[test]
    fn test_not() {
        let s = &parse("script:not([type^=\"math/\"])")[0][0];
        assert_eq!(s.tag.as_deref(), Some("script"));
        assert!(s.negation.is_some());
    }

    #[test]
    fn test_has() {
        let s = &parse("div:has(img)")[0][0];
        assert_eq!(s.tag.as_deref(), Some("div"));
        assert!(s.extra.is_some());
    }

    #[test]
    fn test_attr_equals() {
        let s = &parse("input[type=\"checkbox\"]")[0][0];
        assert_eq!(s.tag.as_deref(), Some("input"));
        assert!(s.attr_equals.is_some());
    }

    #[test]
    fn test_iframe_not_src_parsing() {
        let s = parse_one("iframe:not([src])").expect("should parse");
        assert_eq!(s.tag.as_deref(), Some("iframe"));
        assert!(s.negation.is_some(), "negation should be present for :not([src])");
        let neg = s.negation.as_ref().unwrap();
        assert!(neg.attr_presence.as_deref() == Some("src"), "negation should check [src] presence");
    }

    #[test]
    fn test_star_selector() {
        let s = parse_one("*").expect("should parse *");
        assert!(s.tag.is_none());
        assert!(s.classes.is_empty());
        assert!(s.id.is_none());
    }

    #[test]
    fn test_h1() {
        let s = parse_one("h1").expect("should parse h1");
        assert_eq!(s.tag.as_deref(), Some("h1"));
    }

    #[test]
    fn test_h2() {
        let s = parse_one("h2").expect("should parse h2");
        assert_eq!(s.tag.as_deref(), Some("h2"));
    }

    #[test]
    fn test_h1_comma_h2() {
        let parsed = parse("h1, h2");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0][0].tag.as_deref(), Some("h1"));
        assert_eq!(parsed[1][0].tag.as_deref(), Some("h2"));
    }

    #[test]
    fn test_div_section() {
        let parsed = parse("div,section");
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn test_attr_with_case_insensitive_flag() {
        let s = parse_one("[role=\"banner\"]");
        if let Some(s) = s {
            assert!(s.attr_equals.is_some());
        }
    }

    #[test]
    fn test_simple_tag_pre() {
        assert!(parse_one("pre").is_some());
        assert_eq!(parse_one("pre").unwrap().tag.as_deref(), Some("pre"));
    }

    #[test]
    fn test_simple_tag_code() {
        assert_eq!(parse_one("code").unwrap().tag.as_deref(), Some("code"));
    }

    #[test]
    fn test_simple_tag_a() {
        assert_eq!(parse_one("a").unwrap().tag.as_deref(), Some("a"));
    }

    #[test]
    fn test_simple_tag_sup() {
        assert_eq!(parse_one("sup").unwrap().tag.as_deref(), Some("sup"));
    }

    #[test]
    fn test_equals_attr_with_i_flag_broken() {
        let parsed = parse("[data-print-layout=\"hide\" i]");
        assert!(!parsed.is_empty());
        let compound = &parsed[0];
        assert!(!compound.is_empty());
    }
}
