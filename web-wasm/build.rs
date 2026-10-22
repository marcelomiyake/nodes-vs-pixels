//! Generates Rust sources from the shared contract, design tokens and copy, so
//! the canvas app cannot drift from the React app or the backend.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs};

use serde_json::{Map, Value};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result {
    for input in ["../proto", "../design/tokens.json", "../spec/copy.json"] {
        println!("cargo:rerun-if-changed={input}");
    }
    let out = PathBuf::from(env::var("OUT_DIR")?);

    let descriptors = protox::compile(["signup/v1/signup.proto"], ["../proto"])?;
    prost_build::Config::new().compile_fds(descriptors)?;

    let tokens: Value = serde_json::from_str(&fs::read_to_string("../design/tokens.json")?)?;
    fs::write(out.join("tokens.rs"), tokens_source(&tokens)?)?;

    let copy: Value = serde_json::from_str(&fs::read_to_string("../spec/copy.json")?)?;
    fs::write(out.join("copy.rs"), copy_source(&copy)?)?;
    Ok(())
}

fn constant(key: &str) -> String {
    key.replace('-', "_").to_uppercase()
}

fn object<'a>(value: &'a Value, key: &str) -> Result<&'a Map<String, Value>> {
    value[key]
        .as_object()
        .ok_or_else(|| format!("`{key}` must be an object").into())
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("`{key}` must be a string").into())
}

fn number(value: &Value, key: &str) -> Result<f64> {
    value[key]
        .as_f64()
        .ok_or_else(|| format!("`{key}` must be a number").into())
}

fn tokens_source(tokens: &Value) -> Result<String> {
    let mut src = String::from("// Generated from design/tokens.json by build.rs.\n");
    let font = &tokens["font"];
    writeln!(
        src,
        "pub const FONT_FAMILY: &str = {:?};",
        text(font, "family")?
    )?;
    writeln!(
        src,
        "pub const FONT_FALLBACK: &str = {:?};",
        text(font, "fallback")?
    )?;
    writeln!(
        src,
        "pub const FONT_FILE: &str = {:?};",
        text(font, "file")?
    )?;

    src.push_str("pub mod color {\n");
    for (name, value) in object(tokens, "color")? {
        let color = value.as_str().ok_or("colors must be strings")?;
        writeln!(src, "    pub const {}: &str = {color:?};", constant(name))?;
    }
    src.push_str("}\npub mod text {\n    use crate::style::TextStyle;\n");
    for (name, style) in object(tokens, "text")? {
        writeln!(
            src,
            "    pub const {}: TextStyle = TextStyle {{ size: {:?}, line_height: {:?}, weight: {} }};",
            constant(name),
            number(style, "size")?,
            number(style, "lineHeight")?,
            style["weight"].as_u64().ok_or("weights must be integers")?,
        )?;
    }
    src.push_str("}\npub mod shadow {\n    use crate::style::Shadow;\n");
    for (name, shadow) in object(tokens, "shadow")? {
        writeln!(
            src,
            "    pub const {}: Shadow = Shadow {{ x: {:?}, y: {:?}, blur: {:?}, color: {:?} }};",
            constant(name),
            number(shadow, "x")?,
            number(shadow, "y")?,
            number(shadow, "blur")?,
            text(shadow, "color")?,
        )?;
    }
    src.push('}');
    for group in ["radius", "spacing", "container", "breakpoint"] {
        writeln!(src, "\npub mod {group} {{")?;
        for (name, value) in object(tokens, group)? {
            let value = value.as_f64().ok_or("sizes must be numbers")?;
            writeln!(src, "    pub const {}: f64 = {value:?};", constant(name))?;
        }
        src.push('}');
    }
    src.push('\n');
    Ok(src)
}

fn copy_source(copy: &Value) -> Result<String> {
    let mut src = String::from("// Generated from spec/copy.json by build.rs.\n");
    for key in [
        "brand",
        "title",
        "subtitle",
        "terms",
        "submit",
        "submitting",
        "networkError",
    ] {
        writeln!(
            src,
            "pub const {}: &str = {:?};",
            constant(&snake(key)),
            text(copy, key)?
        )?;
    }
    let success = &copy["success"];
    writeln!(
        src,
        "pub const SUCCESS_TITLE: &str = {:?};",
        text(success, "title")?
    )?;
    writeln!(
        src,
        "pub const SUCCESS_BODY: &str = {:?};",
        text(success, "body")?
    )?;

    src.push_str("\n/// Copy for a text field, by its `spec/copy.json` key.\n");
    src.push_str("#[must_use]\npub fn field(key: &str) -> Option<crate::style::FieldCopy> {\n    match key {\n");
    for (key, field) in object(copy, "fields")? {
        let hint = field["hint"].as_str();
        writeln!(
            src,
            "        {key:?} => Some(crate::style::FieldCopy {{ label: {:?}, placeholder: {:?}, input_type: {:?}, autocomplete: {:?}, hint: {hint:?} }}),",
            text(field, "label")?,
            text(field, "placeholder")?,
            text(field, "type")?,
            text(field, "autocomplete")?,
        )?;
    }
    src.push_str("        _ => None,\n    }\n}\n");

    src.push_str("\n/// The message for a violation, by field and code key.\n");
    src.push_str("#[must_use]\npub fn violation(field: &str, code: &str) -> Option<&'static str> {\n    match (field, code) {\n");
    for (field, messages) in object(copy, "violations")? {
        for (code, message) in messages.as_object().ok_or("violations must be objects")? {
            let message = message.as_str().ok_or("messages must be strings")?;
            writeln!(src, "        ({field:?}, {code:?}) => Some({message:?}),")?;
        }
    }
    src.push_str("        _ => None,\n    }\n}\n");
    Ok(src)
}

fn snake(key: &str) -> String {
    key.chars().fold(String::new(), |mut out, c| {
        if c.is_ascii_uppercase() {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
        out
    })
}
