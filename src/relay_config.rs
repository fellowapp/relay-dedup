//! Relay configuration detection and validation.
//!
//! Finds and validates relay.config.json or package.json with relay config.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Result of finding and parsing relay config
#[derive(Debug, Clone)]
pub struct RelayConfig {
	/// The artifact directory from config (if specified)
	pub artifact_directory: Option<PathBuf>,
	/// Path to the config file that was found
	pub config_path: PathBuf,
}

/// Find relay config by searching upward from a starting directory.
///
/// Searches for:
/// 1. `relay.config.json` in the directory or any parent
/// 2. `package.json` with a `"relay"` key in the directory or any parent
///
/// Returns `None` if no config is found.
pub fn find_relay_config(start_dir: &Path) -> Option<RelayConfig> {
	for dir in start_dir.ancestors() {
		// Check for relay.config.json
		let relay_config_path = dir.join("relay.config.json");
		if relay_config_path.exists() {
			if let Ok(content) = fs::read_to_string(&relay_config_path) {
				if let Ok(json) = serde_json::from_str::<Value>(&content) {
					let artifact_dir = json
						.get("artifactDirectory")
						.and_then(|v| v.as_str())
						.map(|s| dir.join(s));

					return Some(RelayConfig {
						artifact_directory: artifact_dir,
						config_path: relay_config_path,
					});
				}
			}
		}

		// Check for package.json with "relay" key
		let package_json_path = dir.join("package.json");
		if package_json_path.exists() {
			if let Ok(content) = fs::read_to_string(&package_json_path) {
				if let Ok(json) = serde_json::from_str::<Value>(&content) {
					if let Some(relay) = json.get("relay") {
						let artifact_dir = relay
							.get("artifactDirectory")
							.and_then(|v| v.as_str())
							.map(|s| dir.join(s));

						return Some(RelayConfig {
							artifact_directory: artifact_dir,
							config_path: package_json_path,
						});
					}
				}
			}
		}
	}

	None
}

/// Validate that required feature flags are set in relay config.
///
/// Required flags:
/// - `disable_deduping_common_structures_in_artifacts`: must be `{ "kind": "enabled" }`
///
/// Returns `Ok(())` if valid, `Err` with detailed message if not.
pub fn validate_relay_config(config_path: &Path) -> Result<()> {
	let content = fs::read_to_string(config_path)
		.with_context(|| format!("Failed to read {}", config_path.display()))?;

	let json: Value = serde_json::from_str(&content)
		.with_context(|| format!("Failed to parse {}", config_path.display()))?;

	// If this is package.json, look under "relay" key
	let config = if config_path
		.file_name()
		.map(|n| n == "package.json")
		.unwrap_or(false)
	{
		json.get("relay")
			.ok_or_else(|| anyhow::anyhow!("No 'relay' key found in package.json"))?
	} else {
		&json
	};

	let feature_flags = config.get("featureFlags");

	// Check disable_deduping_common_structures_in_artifacts
	let dedup_disabled = feature_flags
		.and_then(|ff| ff.get("disable_deduping_common_structures_in_artifacts"))
		.and_then(|v| v.get("kind"))
		.and_then(|v| v.as_str())
		== Some("enabled");

	if !dedup_disabled {
		bail!(
			r#"
ERROR: Relay's built-in deduplication must be disabled.

Add the following to your relay config ({}):

  "featureFlags": {{
    "disable_deduping_common_structures_in_artifacts": {{ "kind": "enabled" }},
    "enforce_fragment_alias_where_ambiguous": {{ "kind": "disabled" }}
  }}

The first flag is REQUIRED - Relay's dedup conflicts with this tool.
The second flag works around a Relay bug where any feature flags enable strict alias checking.
"#,
			config_path.display()
		);
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use tempfile::tempdir;

	#[test]
	fn test_find_relay_config() {
		let temp = tempdir().unwrap();

		// No config → None
		assert!(find_relay_config(temp.path()).is_none());

		// relay.config.json works
		fs::write(
			temp.path().join("relay.config.json"),
			r#"{ "artifactDirectory": "./src/__generated__" }"#,
		)
		.unwrap();

		let config = find_relay_config(temp.path()).unwrap();
		assert!(config.config_path.ends_with("relay.config.json"));
		assert_eq!(
			config.artifact_directory,
			Some(temp.path().join("./src/__generated__"))
		);
	}

	#[test]
	fn test_find_relay_config_in_package_json() {
		let temp = tempdir().unwrap();

		fs::write(
			temp.path().join("package.json"),
			r#"{ "relay": { "artifactDirectory": "./gen" } }"#,
		)
		.unwrap();

		let config = find_relay_config(temp.path()).unwrap();
		assert!(config.config_path.ends_with("package.json"));
	}

	#[test]
	fn test_validate_relay_config() {
		let temp = tempdir().unwrap();
		let path = temp.path().join("relay.config.json");

		// Missing flags → error
		fs::write(&path, r#"{ "artifactDirectory": "./src" }"#).unwrap();
		assert!(validate_relay_config(&path).is_err());

		// Valid flags → ok
		fs::write(
			&path,
			r#"{
				"featureFlags": {
					"disable_deduping_common_structures_in_artifacts": { "kind": "enabled" }
				}
			}"#,
		)
		.unwrap();
		assert!(validate_relay_config(&path).is_ok());
	}
}
