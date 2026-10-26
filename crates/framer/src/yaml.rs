//! A small YAML writer for generated files such as GitHub workflows.
//!
//! It only writes YAML, always in block style, and keeps map keys in the order
//! they were inserted so generated files read the way they were declared.

use std::fmt::Write;

/// A YAML document.
#[derive(Clone, Debug, PartialEq)]
pub enum Yaml {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Seq(Vec<Yaml>),
    Map(Vec<(String, Yaml)>),
}

/// Flow-style sequences (`[a, b]`) longer than this are written one item per line.
const MAX_FLOW_WIDTH: usize = 60;

impl Yaml {
    pub fn map() -> Self {
        Self::Map(Vec::new())
    }

    /// Adds `key: value` to a map. Does nothing to other kinds of value.
    pub fn with(mut self, key: impl Into<String>, value: impl Into<Yaml>) -> Self {
        if let Self::Map(entries) = &mut self {
            entries.push((key.into(), value.into()));
        }
        self
    }

    /// Adds `key: value` only when the value is present.
    pub fn with_some<T: Into<Yaml>>(self, key: impl Into<String>, value: Option<T>) -> Self {
        match value {
            Some(value) => self.with(key, value),
            None => self,
        }
    }

    /// Writes the document, ending with a newline.
    pub fn to_yaml_string(&self) -> String {
        let mut out = String::new();
        match self {
            Self::Map(entries) if !entries.is_empty() => write_map(&mut out, entries, 0, false),
            Self::Seq(items) if !items.is_empty() => write_seq(&mut out, items, 0),
            scalar => {
                out.push_str(&scalar_text(scalar, false));
                out.push('\n');
            }
        }
        out
    }
}

impl From<&str> for Yaml {
    fn from(value: &str) -> Self {
        Self::Str(value.to_owned())
    }
}

impl From<String> for Yaml {
    fn from(value: String) -> Self {
        Self::Str(value)
    }
}

impl From<&String> for Yaml {
    fn from(value: &String) -> Self {
        Self::Str(value.clone())
    }
}

impl From<bool> for Yaml {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i64> for Yaml {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<u32> for Yaml {
    fn from(value: u32) -> Self {
        Self::Int(value.into())
    }
}

impl<T: Into<Yaml>> From<Vec<T>> for Yaml {
    fn from(values: Vec<T>) -> Self {
        Self::Seq(values.into_iter().map(Into::into).collect())
    }
}

fn pad(out: &mut String, indent: usize) {
    out.extend(std::iter::repeat_n(' ', indent));
}

fn write_map(out: &mut String, entries: &[(String, Yaml)], indent: usize, first_inline: bool) {
    for (index, (key, value)) in entries.iter().enumerate() {
        if !(first_inline && index == 0) {
            pad(out, indent);
        }
        out.push_str(&key_text(key));
        out.push(':');
        write_after_indicator(out, value, indent + 2);
    }
}

fn write_seq(out: &mut String, items: &[Yaml], indent: usize) {
    for item in items {
        pad(out, indent);
        out.push('-');
        match item {
            Yaml::Map(entries) if !entries.is_empty() => {
                out.push(' ');
                write_map(out, entries, indent + 2, true);
            }
            other => write_after_indicator(out, other, indent + 2),
        }
    }
}

/// Writes a value that follows `key:` or `-`, with nested blocks at `indent`.
fn write_after_indicator(out: &mut String, value: &Yaml, indent: usize) {
    match value {
        Yaml::Map(entries) if !entries.is_empty() => {
            out.push('\n');
            write_map(out, entries, indent, false);
        }
        Yaml::Seq(items) if !items.is_empty() => {
            if let Some(flow) = flow_seq(items) {
                out.push(' ');
                out.push_str(&flow);
                out.push('\n');
            } else {
                out.push('\n');
                write_seq(out, items, indent);
            }
        }
        Yaml::Null => out.push('\n'),
        Yaml::Str(text) if text.contains('\n') && !text.starts_with([' ', '\t', '\n']) => {
            write_block_scalar(out, text, indent);
        }
        scalar => {
            out.push(' ');
            out.push_str(&scalar_text(scalar, false));
            out.push('\n');
        }
    }
}

fn write_block_scalar(out: &mut String, text: &str, indent: usize) {
    let body = text.trim_end_matches('\n');
    let chomping = match text.len() - body.len() {
        0 => "-",
        1 => "",
        _ => "+",
    };
    out.push_str(" |");
    out.push_str(chomping);
    out.push('\n');
    for line in body.split('\n') {
        if !line.is_empty() {
            pad(out, indent);
            out.push_str(line);
        }
        out.push('\n');
    }
    for _ in 1..text.len() - body.len() {
        out.push('\n');
    }
}

/// `[a, b]` when every item is a short single-line scalar.
fn flow_seq(items: &[Yaml]) -> Option<String> {
    let mut parts = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Yaml::Str(text) if text.contains('\n') => return None,
            Yaml::Str(_) | Yaml::Bool(_) | Yaml::Int(_) => parts.push(scalar_text(item, true)),
            _ => return None,
        }
    }
    let flow = format!("[{}]", parts.join(", "));
    (flow.len() <= MAX_FLOW_WIDTH).then_some(flow)
}

fn scalar_text(value: &Yaml, in_flow: bool) -> String {
    match value {
        Yaml::Null => "null".into(),
        Yaml::Bool(value) => value.to_string(),
        Yaml::Int(value) => value.to_string(),
        Yaml::Str(text) if is_plain(text, in_flow) && !resolves_to_non_string(text) => text.clone(),
        Yaml::Str(text) => quoted(text),
        Yaml::Seq(_) => "[]".into(),
        Yaml::Map(_) => "{}".into(),
    }
}

fn key_text(key: &str) -> String {
    // Keys like `on` stay unquoted: GitHub reads them as strings, and quoting
    // them would surprise anyone reading the generated file.
    if is_plain(key, false) {
        key.to_owned()
    } else {
        quoted(key)
    }
}

/// Whether `text` can be written without quotes and still read back as the same text.
fn is_plain(text: &str, in_flow: bool) -> bool {
    let Some(first) = text.chars().next() else {
        return false;
    };
    if text.trim() != text || text.contains(['\n', '\r', '\t']) {
        return false;
    }
    if "-?:,[]{}#&*!|>'\"%@`".contains(first) {
        return false;
    }
    if text.contains(": ") || text.contains(" #") || text.ends_with(':') {
        return false;
    }
    !(in_flow && text.contains([',', '[', ']', '{', '}']))
}

/// Whether a plain scalar would be read as a boolean, null or number.
fn resolves_to_non_string(text: &str) -> bool {
    const RESERVED: &[&str] = &[
        "~", "null", "true", "false", "yes", "no", "on", "off", "y", "n", ".inf", "-.inf", "+.inf",
        ".nan",
    ];
    let lower = text.to_ascii_lowercase();
    if RESERVED.contains(&lower.as_str()) {
        return true;
    }
    let numeric = text
        .chars()
        .all(|c| c.is_ascii_digit() || "_.+-eE".contains(c));
    numeric && text.chars().any(|c| c.is_ascii_digit())
        || lower.starts_with("0x")
        || lower.starts_with("0o")
        || text.parse::<f64>().is_ok()
}

fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn writes_nested_maps_and_sequences() {
        let doc = Yaml::map()
            .with("name", "CI")
            .with(
                "on",
                Yaml::map()
                    .with("push", Yaml::map().with("branches", vec!["main"]))
                    .with("pull_request", Yaml::Null),
            )
            .with(
                "steps",
                vec![
                    Yaml::map().with("uses", "actions/checkout@v6"),
                    Yaml::map().with("name", "Test").with("run", "cargo test"),
                ],
            );

        assert_eq!(
            doc.to_yaml_string(),
            indoc! {"
                name: CI
                on:
                  push:
                    branches: [main]
                  pull_request:
                steps:
                  - uses: actions/checkout@v6
                  - name: Test
                    run: cargo test
            "}
        );
    }

    #[test]
    fn quotes_strings_that_would_change_meaning() {
        for (text, expected) in [
            ("0", "\"0\""),
            ("1.98", "\"1.98\""),
            ("true", "\"true\""),
            ("On", "\"On\""),
            ("", "\"\""),
            ("a: b", "\"a: b\""),
            ("*star", "\"*star\""),
            ("say \"hi\"", "say \"hi\""),
            ("${{ matrix.os }}", "${{ matrix.os }}"),
            ("cargo test --workspace", "cargo test --workspace"),
        ] {
            assert_eq!(scalar_text(&Yaml::from(text), false), expected, "{text}");
        }
    }

    #[test]
    fn writes_multiline_strings_as_block_scalars() {
        let doc = Yaml::map()
            .with("run", "cargo build\ncargo test\n")
            .with("script", "one\n\ntwo");

        assert_eq!(
            doc.to_yaml_string(),
            "run: |\n  cargo build\n  cargo test\nscript: |-\n  one\n\n  two\n"
        );
    }

    #[test]
    fn long_or_complex_sequences_use_block_style() {
        let long: Vec<String> = (0..20).map(|i| format!("item-{i}")).collect();
        let doc = Yaml::map().with("long", long).with("flow", vec!["a,b"]);

        let text = doc.to_yaml_string();

        assert!(text.starts_with("long:\n  - item-0\n"));
        assert!(text.ends_with("flow: [\"a,b\"]\n"));
    }

    #[test]
    fn empty_collections_are_written_inline() {
        let doc = Yaml::map()
            .with("map", Yaml::map())
            .with("seq", Vec::<String>::new());

        assert_eq!(doc.to_yaml_string(), "map: {}\nseq: []\n");
    }
}
