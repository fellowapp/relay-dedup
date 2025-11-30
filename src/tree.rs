//! Tree-based file representation for efficient multi-pass deduplication.
//!
//! Instead of repeatedly parsing strings, we build a tree once and mutate it.

use crate::normalize::normalize;
use std::collections::HashSet;

/// A node in the structure tree
#[derive(Debug, Clone)]
pub struct Node {
	pub start: usize,
	pub end: usize,
	pub is_array: bool,
	pub parent: Option<usize>,        // parent node index (for O(1) lookup)
	pub children: Vec<usize>,         // indices of child nodes
	pub extracted_as: Option<String>, // ref name if extracted (e.g., "x_abc")
	pub normalized: Option<String>,   // cached normalized form
}

/// Tree representation of a file's structure
#[derive(Debug)]
pub struct FileTree {
	pub original: String,
	pub nodes: Vec<Node>,
	pub root_nodes: Vec<usize>,       // top-level structure indices
	serialized_cache: Option<String>, // cached serialization
}

impl FileTree {
	/// Build a tree from file content (parse once)
	pub fn new(content: String, order_insensitive_fields: &HashSet<String>) -> Self {
		let mut nodes: Vec<Node> = Vec::new();
		let mut stack: Vec<usize> = Vec::new();
		let mut root_nodes: Vec<usize> = Vec::new();

		// Find where imports end
		let import_end = Self::find_import_end(&content);

		let bytes = content.as_bytes();
		let mut in_string = false;
		let mut escape = false;

		for (i, &c) in bytes.iter().enumerate().skip(import_end) {
			if escape {
				escape = false;
				continue;
			}
			if c == b'\\' {
				escape = true;
				continue;
			}
			if c == b'"' {
				in_string = !in_string;
				continue;
			}
			if in_string {
				continue;
			}

			if c == b'{' || c == b'[' {
				let node_idx = nodes.len();
				let parent_idx = stack.last().copied();
				nodes.push(Node {
					start: i,
					end: 0,
					is_array: c == b'[',
					parent: parent_idx,
					children: Vec::new(),
					extracted_as: None,
					normalized: None,
				});

				if let Some(parent_idx) = parent_idx {
					nodes[parent_idx].children.push(node_idx);
				} else {
					root_nodes.push(node_idx);
				}
				stack.push(node_idx);
			} else if c == b'}' || c == b']' {
				if let Some(node_idx) = stack.pop() {
					nodes[node_idx].end = i + 1;
				}
			}
		}

		let mut tree = FileTree {
			original: content,
			nodes,
			root_nodes,
			serialized_cache: None,
		};

		// Pre-compute normalized forms for valid leaves
		tree.compute_normalized_forms(order_insensitive_fields);

		tree
	}

	fn find_import_end(content: &str) -> usize {
		let mut pos = 0;
		for line in content.lines() {
			let trimmed = line.trim();
			if trimmed.is_empty()
				|| trimmed.starts_with("//")
				|| trimmed.starts_with("/*")
				|| trimmed.starts_with("*")
				|| trimmed.starts_with("*/")
				|| trimmed.starts_with("import ")
			{
				pos += line.len() + 1;
			} else {
				break;
			}
		}
		pos
	}

	/// Pre-compute normalized forms for nodes that could be leaves
	fn compute_normalized_forms(&mut self, order_insensitive_fields: &HashSet<String>) {
		for i in 0..self.nodes.len() {
			// Only compute for potential leaves (no children, valid content)
			if !self.nodes[i].children.is_empty() {
				continue;
			}
			if self.nodes[i].end == 0 {
				continue;
			}

			let content = &self.original[self.nodes[i].start..self.nodes[i].end];
			if content.len() < 15 {
				continue;
			}

			if !self.is_valid_leaf_content(content) {
				continue;
			}

			let can_sort = self.nodes[i].is_array
				&& self.is_order_insensitive(self.nodes[i].start, order_insensitive_fields);
			let normalized = normalize(content, can_sort);
			self.nodes[i].normalized = Some(normalized);
		}
	}

	/// Check if content is a valid leaf (no invalid identifiers)
	fn is_valid_leaf_content(&self, content: &str) -> bool {
		let inner = &content[1..content.len() - 1];
		let inner = inner.trim();

		if inner.is_empty() {
			return false;
		}

		let mut in_string = false;
		let mut escape = false;
		let mut ident = String::new();

		for c in inner.chars() {
			if escape {
				escape = false;
				continue;
			}
			if c == '\\' {
				escape = true;
				continue;
			}
			if c == '"' {
				in_string = !in_string;
				continue;
			}
			if in_string {
				continue;
			}

			if c.is_ascii_alphanumeric() || c == '_' {
				ident.push(c);
			} else if !ident.is_empty() {
				if !Self::is_valid_identifier(&ident) {
					return false;
				}
				ident.clear();
			}
		}

		ident.is_empty() || Self::is_valid_identifier(&ident)
	}

	fn is_valid_identifier(ident: &str) -> bool {
		// Our refs: x_XXX (3+ hex) or _XXXXXXXX (8 hex)
		if ident.starts_with("x_") && ident.len() >= 4 {
			return ident[2..].chars().all(|c| c.is_ascii_hexdigit());
		}
		if ident.starts_with('_') && ident.len() == 9 {
			return ident[1..].chars().all(|c| c.is_ascii_hexdigit());
		}
		matches!(ident, "null" | "true" | "false") || ident.chars().all(|c| c.is_ascii_digit())
	}

	fn is_order_insensitive(&self, pos: usize, fields: &HashSet<String>) -> bool {
		// Look backwards for field name
		let bytes = self.original.as_bytes();
		let mut i = pos.saturating_sub(1);

		// Skip whitespace
		while i > 0 && bytes[i].is_ascii_whitespace() {
			i -= 1;
		}
		// Should be colon
		if i == 0 || bytes[i] != b':' {
			return false;
		}
		i -= 1;
		// Skip whitespace
		while i > 0 && bytes[i].is_ascii_whitespace() {
			i -= 1;
		}
		// Should be quote
		if i == 0 || bytes[i] != b'"' {
			return false;
		}
		i -= 1;
		// Read field name backwards
		let mut name = String::new();
		while i > 0 && bytes[i] != b'"' {
			name.insert(0, bytes[i] as char);
			i -= 1;
		}

		fields.contains(&name)
	}

	/// Find current leaves (nodes where all children are extracted OR no children)
	pub fn find_leaves(&self) -> Vec<(usize, String)> {
		let mut leaves = Vec::new();

		for (idx, node) in self.nodes.iter().enumerate() {
			// Skip already extracted
			if node.extracted_as.is_some() {
				continue;
			}
			// Must have normalized form (valid leaf)
			let Some(normalized) = &node.normalized else {
				continue;
			};

			// All children must be extracted
			let all_children_extracted = node
				.children
				.iter()
				.all(|&child_idx| self.nodes[child_idx].extracted_as.is_some());

			if all_children_extracted {
				leaves.push((idx, normalized.clone()));
			}
		}

		leaves
	}

	/// Mark a node as extracted
	pub fn mark_extracted(
		&mut self,
		node_idx: usize,
		ref_name: String,
		order_insensitive_fields: &HashSet<String>,
	) {
		self.nodes[node_idx].extracted_as = Some(ref_name);
		self.serialized_cache = None; // invalidate cache

		// Parent might now be a valid leaf - recompute its normalized form
		self.update_parent_normalized(node_idx, order_insensitive_fields);
	}

	/// After extracting a child, parent might become a valid leaf
	fn update_parent_normalized(
		&mut self,
		child_idx: usize,
		order_insensitive_fields: &HashSet<String>,
	) {
		// O(1) parent lookup
		let Some(parent_idx) = self.nodes[child_idx].parent else {
			return;
		};

		// Recompute normalized form for parent if not already extracted
		if self.nodes[parent_idx].extracted_as.is_some() {
			return;
		}

		// Check if all children are now extracted
		let all_extracted = self.nodes[parent_idx]
			.children
			.iter()
			.all(|&c| self.nodes[c].extracted_as.is_some());

		if !all_extracted {
			return;
		}

		let content = self.get_current_content(parent_idx);
		if content.len() >= 15 && self.is_valid_leaf_content(&content) {
			let can_sort = self.nodes[parent_idx].is_array
				&& self
					.is_order_insensitive(self.nodes[parent_idx].start, order_insensitive_fields);
			let normalized = normalize(&content, can_sort);
			self.nodes[parent_idx].normalized = Some(normalized);
		}
	}

	/// Get current content of a node (with children replaced by refs)
	fn get_current_content(&self, node_idx: usize) -> String {
		let node = &self.nodes[node_idx];
		let original = &self.original[node.start..node.end];

		if node.children.is_empty() {
			return original.to_string();
		}

		// Build content with child refs substituted
		let mut result = String::new();
		let mut last_end = node.start;

		// Sort children by start position
		let mut children: Vec<_> = node.children.to_vec();
		children.sort_by_key(|&idx| self.nodes[idx].start);

		for child_idx in children {
			let child = &self.nodes[child_idx];

			// Add content before this child
			result.push_str(&self.original[last_end..child.start]);

			// Add ref or recurse
			if let Some(ref ref_name) = child.extracted_as {
				result.push_str(ref_name);
			} else {
				result.push_str(&self.get_current_content(child_idx));
			}

			last_end = child.end;
		}

		// Add remaining content
		result.push_str(&self.original[last_end..node.end]);

		result
	}

	/// Serialize the tree back to a string (cached)
	pub fn serialize(&mut self) -> String {
		if let Some(ref cached) = self.serialized_cache {
			return cached.clone();
		}

		let result = self.serialize_uncached();
		self.serialized_cache = Some(result.clone());
		result
	}

	/// Serialize without caching
	fn serialize_uncached(&self) -> String {
		let mut result = String::with_capacity(self.original.len());
		let mut last_end = 0;

		// Collect all top-level and extracted nodes, sorted by position
		let mut replacements: Vec<(usize, usize, &str)> = Vec::new();

		fn collect_replacements<'a>(
			tree: &'a FileTree,
			node_idx: usize,
			replacements: &mut Vec<(usize, usize, &'a str)>,
		) {
			let node = &tree.nodes[node_idx];
			if let Some(ref ref_name) = node.extracted_as {
				replacements.push((node.start, node.end, ref_name.as_str()));
			} else {
				// Recurse into children
				for &child_idx in &node.children {
					collect_replacements(tree, child_idx, replacements);
				}
			}
		}

		for &root_idx in &self.root_nodes {
			collect_replacements(self, root_idx, &mut replacements);
		}

		replacements.sort_by_key(|(start, _, _)| *start);

		for (start, end, ref_name) in replacements {
			result.push_str(&self.original[last_end..start]);
			result.push_str(ref_name);
			last_end = end;
		}
		result.push_str(&self.original[last_end..]);

		result
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_build_tree() {
		let content = r#"const x = {"a": 1, "b": [{"c": 2}]};"#.to_string();
		let tree = FileTree::new(content, &HashSet::new());

		assert!(!tree.nodes.is_empty());
	}

	#[test]
	fn test_find_leaves() {
		let content = r#"const x = {"kind": "Field", "name": "id"};"#.to_string();
		let tree = FileTree::new(content, &HashSet::new());

		let leaves = tree.find_leaves();
		assert!(!leaves.is_empty());
	}
}
