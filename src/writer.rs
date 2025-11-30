//! Writer module for file rewriting and shared module generation.
//!
//! Handles replacing structures with references and managing imports.

use crate::ExtractedEntry;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// Update imports in the file content.
pub fn update_imports(content: &str, shared_module_name: &str) -> String {
	let import_source = format!("./{}", shared_module_name.trim_end_matches(".ts"));
	let import_marker = format!("from \"{}\"", import_source);

	// Remove all existing shared module imports
	let mut lines: Vec<&str> = content.lines().collect();
	lines.retain(|line| !line.contains(&import_marker));

	// Find all x_XXX refs in content (exclude import lines we just removed)
	let mut used_refs: HashSet<String> = HashSet::new();
	for line in &lines {
		// Skip import lines for ref scanning
		if line.starts_with("import ") {
			continue;
		}
		let bytes = line.as_bytes();
		let mut i = 0;
		while i + 3 < bytes.len() {
			if bytes[i] == b'x' && bytes[i + 1] == b'_' {
				let mut ref_name = String::from("x_");
				let mut j = i + 2;
				while j < bytes.len() && bytes[j].is_ascii_hexdigit() {
					ref_name.push(bytes[j] as char);
					j += 1;
				}
				if ref_name.len() >= 4 {
					used_refs.insert(ref_name);
				}
				i = j;
			} else {
				i += 1;
			}
		}
	}

	if used_refs.is_empty() {
		let mut result = lines.join("\n");
		result.push('\n');
		return result;
	}

	// Create import line
	let mut refs: Vec<_> = used_refs.into_iter().collect();
	refs.sort();
	let import_line = format!(
		"import {{ {} }} from \"{}\";",
		refs.join(", "),
		import_source
	);

	// Find insert position (after other imports, before exports/code)
	let mut insert_idx = 0;
	for (i, line) in lines.iter().enumerate() {
		if line.starts_with("import ") {
			insert_idx = i + 1;
		} else if line.starts_with("export ") || line.starts_with("const ") {
			break;
		}
	}

	// Insert and join (with trailing newline)
	lines.insert(insert_idx, &import_line);
	let mut result = lines.join("\n");
	result.push('\n');
	result
}

/// Generate the shared module content as a string (no I/O).
pub fn generate_shared_module_content(extracted: &HashMap<String, ExtractedEntry>) -> String {
	let mut lines = vec![
		"/**".to_string(),
		" * @generated - Do not edit manually".to_string(),
		" * Shared Relay structures".to_string(),
		" */".to_string(),
		"// eslint-disable-next-line @typescript-eslint/no-explicit-any".to_string(),
		"type RelayNode = any;".to_string(),
		String::new(),
	];

	// Topologically sort entries
	let sorted = topo_sort(extracted);

	for (normalized, entry) in sorted {
		lines.push(format!(
			"export const {}: RelayNode = {};",
			entry.name, normalized
		));
	}

	lines.push(String::new());
	lines.join("\n")
}

/// Write the shared module file with all extracted structures.
pub fn write_shared_module(
	shared_path: &Path,
	extracted: &HashMap<String, ExtractedEntry>,
) -> Result<()> {
	let content = generate_shared_module_content(extracted);
	fs::write(shared_path, content)?;
	Ok(())
}

/// Get dependency names from a normalized string.
fn get_deps(normalized: &str) -> Vec<String> {
	let mut deps = Vec::new();

	let mut chars = normalized.chars().peekable();
	while let Some(c) = chars.next() {
		if c == 'x' && chars.peek() == Some(&'_') {
			chars.next(); // consume _
			let mut ref_name = String::from("x_");
			while let Some(&c) = chars.peek() {
				if c.is_ascii_hexdigit() {
					ref_name.push(chars.next().unwrap());
				} else {
					break;
				}
			}
			if ref_name.len() >= 4 {
				deps.push(ref_name);
			}
		}
	}

	deps
}

/// Topologically sort extracted entries for proper dependency order.
fn topo_sort(extracted: &HashMap<String, ExtractedEntry>) -> Vec<(String, ExtractedEntry)> {
	let name_to_entry: HashMap<&str, (&String, &ExtractedEntry)> = extracted
		.iter()
		.map(|(n, e)| (e.name.as_str(), (n, e)))
		.collect();

	let mut result: Vec<(String, ExtractedEntry)> = Vec::new();
	let mut visited: HashSet<String> = HashSet::new();

	fn visit(
		name: &str,
		name_to_entry: &HashMap<&str, (&String, &ExtractedEntry)>,
		visited: &mut HashSet<String>,
		result: &mut Vec<(String, ExtractedEntry)>,
	) {
		if visited.contains(name) {
			return;
		}
		visited.insert(name.to_string());

		let Some(&(normalized, entry)) = name_to_entry.get(name) else {
			return;
		};

		// Visit dependencies first
		for dep in get_deps(normalized) {
			if name_to_entry.contains_key(dep.as_str()) {
				visit(&dep, name_to_entry, visited, result);
			}
		}

		result.push((normalized.clone(), entry.clone()));
	}

	// Sort names for determinism
	let mut names: Vec<_> = name_to_entry.keys().copied().collect();
	names.sort();

	for name in names {
		visit(name, &name_to_entry, &mut visited, &mut result);
	}

	result
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_get_deps() {
		assert_eq!(get_deps(r#"{"ref": x_abc}"#), vec!["x_abc"]);
		assert_eq!(get_deps(r#"[x_abc, x_def]"#), vec!["x_abc", "x_def"]);
		assert!(get_deps(r#"{"key": "value"}"#).is_empty());
	}

	#[test]
	fn test_topo_sort() {
		let mut extracted = HashMap::new();

		// x_aaa depends on nothing
		extracted.insert(
			r#"{"kind":"Literal"}"#.to_string(),
			ExtractedEntry {
				name: "x_aaa".to_string(),
				hash: "aaa12345".to_string(),
				count: 2,
			},
		);

		// x_bbb depends on x_aaa
		extracted.insert(
			r#"[x_aaa]"#.to_string(),
			ExtractedEntry {
				name: "x_bbb".to_string(),
				hash: "bbb12345".to_string(),
				count: 2,
			},
		);

		let sorted = topo_sort(&extracted);

		// x_aaa should come before x_bbb
		let aaa_idx = sorted.iter().position(|(_, e)| e.name == "x_aaa").unwrap();
		let bbb_idx = sorted.iter().position(|(_, e)| e.name == "x_bbb").unwrap();
		assert!(aaa_idx < bbb_idx);
	}
}
