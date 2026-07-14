use miniserde::json::{Array, Value};

use crate::dom::Document;

use crate::utils::decode_html_entities;

pub fn extract_schema_org_data(doc: &Document) -> Vec<Value> {
    let scripts: Vec<_> = doc.query_selector_all("script[type=\"application/ld+json\"]");

    let mut items = Vec::new();
    for script_id in scripts {
        let text = doc.text_content(script_id);
        let text = text.trim();
        if text.is_empty() {
            continue;
        }

        let json_content = clean_json_content(text);
        match miniserde::json::from_str::<Value>(&json_content) {
            Ok(data) => {
                if let Some(graph) = object_value(&data, "@graph").and_then(as_array) {
                    for item in graph {
                        items.push(decode_value_strings(item.clone()));
                    }
                } else {
                    items.push(decode_value_strings(data));
                }
            }
            Err(_e) => {
                // Schema.org data parsing failures are non-fatal
            }
        }
    }

    items
}

pub(crate) fn as_array(value: &Value) -> Option<&Array> {
    match value {
        Value::Array(array) => Some(array),
        _ => None,
    }
}

pub(crate) fn object_value<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(object) => object.get(key),
        _ => None,
    }
}

pub(crate) fn as_str(value: &Value) -> Option<&str> {
    match value {
        Value::String(string) => Some(string),
        _ => None,
    }
}

pub(crate) fn is_container(value: &Value) -> bool {
    matches!(value, Value::Array(_) | Value::Object(_))
}

fn clean_json_content(content: &str) -> String {
    let content = content
        .lines()
        .filter(|line| !line.trim().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    let content = if content.starts_with("/*") {
        let end = content.rfind("*/").map(|i| i + 2).unwrap_or(content.len());
        content[end..].trim().to_string()
    } else {
        content.to_string()
    };

    if let Some(inner) = content
        .strip_prefix("<![CDATA[")
        .and_then(|rest| rest.strip_suffix("]]>"))
    {
        return inner.to_string();
    }

    content
}

fn decode_value_strings(value: Value) -> Value {
    match value {
        Value::String(s) => Value::String(decode_html_entities(&s)),
        Value::Array(arr) => Value::Array(arr.into_iter().map(decode_value_strings).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, decode_value_strings(v)))
                .collect(),
        ),
        other => other,
    }
}

pub fn collect_meta_tags(doc: &Document) -> Vec<(Option<String>, Option<String>, Option<String>)> {
    let meta_ids = doc.query_selector_all("meta");
    let mut tags = Vec::new();

    for id in meta_ids {
        let name = doc.get_attr(id, "name").map(|s| s.to_string());
        let property = doc.get_attr(id, "property").map(|s| s.to_string());
        let content = doc.get_attr(id, "content").map(|s| s.to_string());

        if content.is_some() {
            let decoded = content.map(|c| decode_html_entities(&c));
            tags.push((name, property, decoded));
        }
    }

    tags
}
