//! The schemas compiled into `im-core`.
//!
//! Keeping the definitions in the repository as plain TOML files means the
//! documentation, the tests and the shipped defaults can never drift apart:
//! `im schema show <id>` prints exactly the text embedded here.

use crate::error::{Error, Result};

use super::Schema;

/// Every built-in schema as `(id, TOML source)`.
pub const SOURCES: &[(&str, &str)] = &[
    ("pinyin", include_str!("../../data/schemas/pinyin.toml")),
    (
        "double_pinyin",
        include_str!("../../data/schemas/double_pinyin.toml"),
    ),
    ("xiaohe", include_str!("../../data/schemas/xiaohe.toml")),
];

/// Identifiers of the built-in schemas.
pub fn ids() -> impl Iterator<Item = &'static str> {
    SOURCES.iter().map(|(id, _)| *id)
}

/// The TOML source of a built-in schema.
pub fn source(id: &str) -> Option<&'static str> {
    SOURCES
        .iter()
        .find(|(builtin, _)| *builtin == id)
        .map(|(_, source)| *source)
}

/// Parse a built-in schema.
pub fn get(id: &str) -> Result<Schema> {
    let source = source(id).ok_or_else(|| Error::NotFound {
        kind: "schema",
        name: id.to_owned(),
        searched: ids().collect::<Vec<_>>().join(", "),
    })?;
    Schema::parse(source).map_err(|e| Error::Schema {
        id: id.to_owned(),
        message: match e {
            Error::Schema { message, .. } => message,
            other => other.to_string(),
        },
    })
}

/// Parse every built-in schema.
pub fn all() -> Vec<Schema> {
    SOURCES.iter().filter_map(|(id, _)| get(id).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_parseable() {
        let mut seen = Vec::new();
        for id in ids() {
            assert!(!seen.contains(&id), "duplicate built-in schema {id}");
            seen.push(id);
            assert!(source(id).is_some());
        }
        assert_eq!(all().len(), SOURCES.len());
    }

    #[test]
    fn unknown_ids_are_reported() {
        assert!(matches!(get("nope"), Err(Error::NotFound { .. })));
    }
}
