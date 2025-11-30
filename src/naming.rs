//! Name generator for deduplicated structures.
//!
//! Generates short, unique names in the format `x_XXX` where XXX is
//! the minimum number of hex characters from the hash needed to be unique.

use std::collections::HashSet;

/// Generator for unique short names.
#[derive(Debug, Default)]
pub struct NameGenerator {
	used: HashSet<String>,
}

impl NameGenerator {
	/// Create a new name generator.
	pub fn new() -> Self {
		Self {
			used: HashSet::new(),
		}
	}

	/// Generate the next unique name for a given hash.
	///
	/// Format: `x_XXX` where XXX is at least 3 hex chars, extended on collision.
	pub fn next(&mut self, hash: &str) -> String {
		// Start with 3 chars, extend if collision
		for len in 3..=hash.len() {
			let name = format!("x_{}", &hash[..len]);
			if !self.used.contains(&name) {
				self.used.insert(name.clone());
				return name;
			}
		}

		// Fallback: use full hash (shouldn't happen)
		let name = format!("x_{}", hash);
		self.used.insert(name.clone());
		name
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_next_generates_short_name() {
		let mut gen = NameGenerator::new();
		let name = gen.next("abcd1234");
		assert_eq!(name, "x_abc");
	}

	#[test]
	fn test_next_handles_collision() {
		let mut gen = NameGenerator::new();

		// First should get x_abc
		let name1 = gen.next("abcd1234");
		assert_eq!(name1, "x_abc");

		// Same prefix should extend
		let name2 = gen.next("abce5678");
		assert_eq!(name2, "x_abce");
	}

	#[test]
	fn test_next_extends_on_multiple_collisions() {
		let mut gen = NameGenerator::new();

		gen.next("abc12345"); // x_abc
		gen.next("abcd1234"); // x_abcd
		gen.next("abcde123"); // x_abcde

		let name = gen.next("abcdf789");
		assert_eq!(name, "x_abcdf");
	}
}
