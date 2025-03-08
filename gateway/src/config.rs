use serde::de::DeserializeOwned;
use serde_path_to_error::{self, Error as PathError};
use toml_edit::{Document, Item, Table};

#[derive(Debug)]
pub enum ConfigError {
    Deserialization(String),
    TomlParse(String),
}

pub fn load_from_toml<T: DeserializeOwned>(toml_str: &str) -> Result<T, ConfigError> {
    // parse TOML into editable document first
    let doc = toml_str.parse::<Document>().map_err(|e| {
        ConfigError::TomlParse(format!("Invalid TOML syntax: {}", e))
    })?;

    // perform deserialization
    let mut deserializer = toml::Deserializer::new(toml_str);
    let result = serde_path_to_error::deserialize::<T, _>(&mut deserializer);

    match result {
        Ok(config) => Ok(config),
        Err(err) => {
            let original_path = err.path().to_string();
            let original_msg = err.to_string();

            // Try to enhance error message using TOML structure
            let enhanced_msg = enhance_error_message(&doc, &original_path, &original_msg)
                .unwrap_or(original_msg);

            Err(ConfigError::Deserialization(enhanced_msg))
        }
    }
}

fn enhance_error_message(
    doc: &Document,
    error_path: &str,
    original_msg: &str,
) -> Option<String> {
    // split error path into components (like "functions.foo" -> ["functions", "foo"])
    let path_segments: Vec<&str> = error_path.split('.').collect();

    // Navigate to the table where the error occurred
    let mut current = doc.as_table();
    for segment in &path_segments {
        current = match current.get(segment)?.as_table() {
            Some(t) => t,
            None => return None,
        };
    }

    // check if this table has a "variants" sub-table
    let variants = current.get("variants")?.as_table()?;
    if variants.is_empty() {
        return None;
    }

    // collect variant keys
    let variant_keys: Vec<&str> = variants.iter()
        .filter(|(_, item)| item.is_table())
        .map(|(k, _)| k.as_str())
        .collect();

    if variant_keys.is_empty() {
        return None;
    }

    // build error message?
    Some(format!(
        "{}\n\nHelp: The error occurred in a table that contains these variants:\n  - {}",
        original_msg,
        variant_keys.join("\n  - ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Config {
        functions: toml::Value,
    }

    #[test]
    fn test_variant_suggestion() {
        let toml = r#"
            [functions.my_func.variants]
            prod = { type = "aws" }
            staging = {}
        "#;

        let err = load_from_toml::<Config>(toml).unwrap_err();
        assert!(err.to_string().contains("variants"));
        assert!(err.to_string().contains("prod"));
        assert!(err.to_string().contains("staging"));
    }

    #[test]
    fn test_inline_table() {
        let toml = r#"
            [functions.my_func]
            variants = { prod = { type = "aws" }, staging = {} }
        "#;

        let err = load_from_toml::<Config>(toml).unwrap_err();
        assert!(err.to_string().contains("prod"));
    }

    #[test]
    fn test_commented_variants() {
        let toml = r#"
            [functions.my_func.variants]
            # prod = { type = "aws" }
            staging = {}
        "#;

        let err = load_from_toml::<Config>(toml).unwrap_err();
        assert!(!err.to_string().contains("prod"));
        assert!(err.to_string().contains("staging"));
    }
}
