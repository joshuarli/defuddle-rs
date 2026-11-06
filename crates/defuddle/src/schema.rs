use deser::Context;
use deser::de::DuplicateKeys;
use deser_json::DeserializerConfig;
use deser_value::{Kind, Map, Seq, Value};

use crate::dom::Document;

use crate::content_utils::decode_html_entities;

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
        if let Ok(data) = parse_json_value(&json_content) {
            if let Some(graph) = object_value(&data, "@graph").and_then(as_array) {
                for item in graph {
                    items.push(decode_value_strings(item.clone()));
                }
            } else {
                items.push(decode_value_strings(data));
            }
        }
    }

    items
}

pub(crate) fn parse_json_value(text: &str) -> Result<Value, deser::Error> {
    DeserializerConfig::builder()
        .context(Context::with(DuplicateKeys::Last))
        .build()
        .from_str(text)
}

pub(crate) fn as_array(value: &Value) -> Option<&Seq> {
    value.as_seq()
}

pub(crate) fn object_value<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value.as_map().and_then(|object| object.get(key))
}

pub(crate) fn as_str(value: &Value) -> Option<&str> {
    value.as_str()
}

pub(crate) fn is_container(value: &Value) -> bool {
    value.as_seq().is_some() || value.as_map().is_some()
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
    match value.into_kind() {
        Kind::Str(text) => Value::from(decode_html_entities(&text)),
        Kind::Seq(items) => Value::from(
            items
                .into_vec()
                .into_iter()
                .map(decode_value_strings)
                .collect::<Seq>(),
        ),
        Kind::Map(object) => {
            let mut entries: Vec<(String, Value)> = object
                .into_iter()
                .filter_map(|(key, value)| {
                    key.as_str()
                        .map(|key| (key.to_string(), decode_value_strings(value)))
                })
                .collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            Value::from(entries.into_iter().collect::<Map>())
        }
        other => Value::from(other),
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
