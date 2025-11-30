//! Normalization module for consistent structure comparison.
//!
//! Handles whitespace stripping and array element sorting for order-insensitive fields.

/// Strip all non-essential whitespace from content (outside string literals).
fn strip_whitespace(content: &str) -> String {
	let mut result = String::with_capacity(content.len());
	let mut in_string = false;
	let mut escape = false;

	for c in content.chars() {
		if escape {
			result.push(c);
			escape = false;
			continue;
		}
		if c == '\\' {
			result.push(c);
			escape = true;
			continue;
		}
		if c == '"' {
			result.push(c);
			in_string = !in_string;
			continue;
		}
		if in_string {
			result.push(c);
			continue;
		}
		// Skip whitespace outside strings
		if c.is_whitespace() {
			continue;
		}
		result.push(c);
	}

	result
}

/// Normalize content for comparison.
///
/// - Strips whitespace
/// - For arrays in order-insensitive fields: sorts elements
/// - For objects: tries to sort keys (if valid JSON), else just strips whitespace
pub fn normalize(content: &str, can_sort_array: bool) -> String {
	let stripped = strip_whitespace(content);

	if stripped.starts_with('[') && can_sort_array {
		normalize_array(&stripped)
	} else if stripped.starts_with('{') {
		// Try to sort object keys (like TS does via JSON.parse)
		normalize_object(&stripped)
	} else {
		stripped
	}
}

/// Normalize an object by sorting its keys.
/// Matches TS behavior: try JSON.parse, if it fails (due to x_ refs, v0/v1 refs,
/// or any non-JSON identifier), just return the stripped content as-is.
fn normalize_object(content: &str) -> String {
	// Try to parse as JSON and sort keys - exactly like TS does
	// If parsing fails for any reason (refs like x_abc, v0, v1, etc.), return as-is
	match serde_json::from_str::<serde_json::Value>(content) {
		Ok(serde_json::Value::Object(map)) => {
			let mut keys: Vec<_> = map.keys().collect();
			keys.sort();
			let pairs: Vec<String> = keys
				.iter()
				.map(|k| format!("\"{}\":{}", k, map.get(*k).unwrap()))
				.collect();
			format!("{{{}}}", pairs.join(","))
		}
		_ => content.to_string(),
	}
}

/// Normalize an array by sorting its elements.
fn normalize_array(content: &str) -> String {
	let inner = &content[1..content.len() - 1];
	if inner.is_empty() {
		return "[]".to_string();
	}

	// Split carefully (not inside nested structures)
	let mut elements = split_array_elements(inner);
	elements.sort();
	format!("[{}]", elements.join(","))
}

/// Split array elements, respecting nested structures.
fn split_array_elements(inner: &str) -> Vec<String> {
	let mut elements = Vec::new();
	let mut depth = 0;
	let mut current = String::new();
	let mut in_string = false;
	let mut escape = false;

	for c in inner.chars() {
		if escape {
			current.push(c);
			escape = false;
			continue;
		}
		if c == '\\' {
			current.push(c);
			escape = true;
			continue;
		}
		if c == '"' {
			current.push(c);
			in_string = !in_string;
			continue;
		}
		if in_string {
			current.push(c);
			continue;
		}

		match c {
			'{' | '[' => {
				depth += 1;
				current.push(c);
			}
			'}' | ']' => {
				depth -= 1;
				current.push(c);
			}
			',' if depth == 0 => {
				let trimmed = current.trim().to_string();
				if !trimmed.is_empty() {
					elements.push(trimmed);
				}
				current.clear();
			}
			_ => {
				current.push(c);
			}
		}
	}

	let trimmed = current.trim().to_string();
	if !trimmed.is_empty() {
		elements.push(trimmed);
	}

	elements
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_strip_whitespace() {
		assert_eq!(strip_whitespace("{ }"), "{}");
		assert_eq!(strip_whitespace("[ 1 , 2 ]"), "[1,2]");
		assert_eq!(
			strip_whitespace(r#"{ "key" : "value with spaces" }"#),
			r#"{"key":"value with spaces"}"#
		);
	}

	#[test]
	fn test_normalize_array_no_sort() {
		assert_eq!(normalize("[3, 1, 2]", false), "[3,1,2]");
	}

	#[test]
	fn test_normalize_array_with_sort() {
		assert_eq!(normalize("[3, 1, 2]", true), "[1,2,3]");
		assert_eq!(normalize(r#"["c", "a", "b"]"#, true), r#"["a","b","c"]"#);
	}

	#[test]
	fn test_normalize_object_sorts_keys() {
		// Objects ARE sorted (keys sorted alphabetically) - matches TS JSON.parse behavior
		assert_eq!(normalize(r#"{"z": 1, "a": 2}"#, false), r#"{"a":2,"z":1}"#);
	}

	#[test]
	fn test_normalize_object_with_refs_no_sort() {
		// Objects with refs (x_abc, v0, v1) can't be parsed as JSON, so just strip whitespace
		assert_eq!(
			normalize(r#"{"z": x_abc, "a": 2}"#, false),
			r#"{"z":x_abc,"a":2}"#
		);
		assert_eq!(
			normalize(r#"{"items": [v0, v1]}"#, false),
			r#"{"items":[v0,v1]}"#
		);
	}

	#[test]
	fn test_normalize_empty() {
		assert_eq!(normalize("[]", false), "[]");
		assert_eq!(normalize("{}", false), "{}");
	}

	#[test]
	fn test_split_array_elements() {
		assert_eq!(split_array_elements("1, 2, 3"), vec!["1", "2", "3"]);
		assert_eq!(
			split_array_elements(r#"{"a": 1}, {"b": 2}"#),
			vec![r#"{"a": 1}"#, r#"{"b": 2}"#]
		);
	}
}
