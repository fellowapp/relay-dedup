//! Integration tests for relay-dedup

use pretty_assertions::assert_eq;
use relay_dedup::{Config, Deduplicator};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Get the output directory path
fn output_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("tests")
		.join("output")
}

/// Copy fixture files to tests/output/ for testing
fn setup_test_dir() -> PathBuf {
	let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("tests")
		.join("fixtures")
		.join("input");

	let output = output_dir();

	// Clean and recreate output directory
	if output.exists() {
		fs::remove_dir_all(&output).unwrap();
	}
	fs::create_dir_all(&output).unwrap();

	for entry in fs::read_dir(&fixtures_dir).unwrap() {
		let entry = entry.unwrap();
		let src = entry.path();
		let dst = output.join(entry.file_name());
		fs::copy(&src, &dst).unwrap();
	}

	output
}

/// Copy fixture files to a separate temp-like directory for comparison tests
fn setup_test_dir_copy(suffix: &str) -> PathBuf {
	let fixtures_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("tests")
		.join("fixtures")
		.join("input");

	let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("tests")
		.join(format!("output_{}", suffix));

	// Clean and recreate output directory
	if output.exists() {
		fs::remove_dir_all(&output).unwrap();
	}
	fs::create_dir_all(&output).unwrap();

	for entry in fs::read_dir(&fixtures_dir).unwrap() {
		let entry = entry.unwrap();
		let src = entry.path();
		let dst = output.join(entry.file_name());
		fs::copy(&src, &dst).unwrap();
	}

	output
}

#[test]
fn test_full_dedup_cycle() {
	let test_dir = setup_test_dir();

	let mut order_insensitive = HashSet::new();
	order_insensitive.insert("selections".to_string());
	order_insensitive.insert("args".to_string());
	order_insensitive.insert("argumentDefinitions".to_string());

	let config = Config {
		generated_dir: test_dir.clone(),
		shared_module_name: "__shared.ts".to_string(),
		min_occurrences: 2,
		order_insensitive_fields: order_insensitive,
		dry_run: false,
		verbose: true,
		max_passes: 50,
		compute_gzip: false,
	};

	let mut deduplicator = Deduplicator::new(config);
	let stats = deduplicator.run().unwrap();

	// Should have extracted some structures
	assert!(stats.total_extracted > 0, "Expected to extract structures");

	// Should have saved some bytes
	assert!(stats.raw_savings() > 0, "Expected raw size savings");

	// Shared module should exist
	let shared_path = test_dir.join("__shared.ts");
	assert!(shared_path.exists(), "Shared module should be created");

	// Shared module should have exports
	let shared_content = fs::read_to_string(&shared_path).unwrap();
	assert!(
		shared_content.contains("export const x_"),
		"Shared module should have exports"
	);

	// Files should have imports
	for entry in fs::read_dir(&test_dir).unwrap() {
		let entry = entry.unwrap();
		let path = entry.path();
		if path
			.file_name()
			.and_then(|n| n.to_str())
			.map(|n| n.ends_with(".graphql.ts"))
			.unwrap_or(false)
		{
			let content = fs::read_to_string(&path).unwrap();
			assert!(
				content.contains("from \"./__shared\""),
				"File {} should import from __shared",
				path.display()
			);
		}
	}
}

#[test]
fn test_deterministic_across_runs() {
	// Run dedup twice on the same input and verify identical output
	let test_dir1 = setup_test_dir_copy("run1");
	let test_dir2 = setup_test_dir_copy("run2");

	let mut order_insensitive = HashSet::new();
	order_insensitive.insert("selections".to_string());
	order_insensitive.insert("args".to_string());
	order_insensitive.insert("argumentDefinitions".to_string());

	// Run first time
	let config1 = Config {
		generated_dir: test_dir1.clone(),
		shared_module_name: "__shared.ts".to_string(),
		min_occurrences: 2,
		order_insensitive_fields: order_insensitive.clone(),
		dry_run: false,
		verbose: false,
		max_passes: 50,
		compute_gzip: false,
	};
	let mut deduplicator1 = Deduplicator::new(config1);
	deduplicator1.run().unwrap();

	// Run second time
	let config2 = Config {
		generated_dir: test_dir2.clone(),
		shared_module_name: "__shared.ts".to_string(),
		min_occurrences: 2,
		order_insensitive_fields: order_insensitive,
		dry_run: false,
		verbose: false,
		max_passes: 50,
		compute_gzip: false,
	};
	let mut deduplicator2 = Deduplicator::new(config2);
	deduplicator2.run().unwrap();

	// Compare all files
	for entry in fs::read_dir(&test_dir1).unwrap() {
		let entry = entry.unwrap();
		let name = entry.file_name();
		let content1 = fs::read_to_string(entry.path()).unwrap();
		let content2 = fs::read_to_string(test_dir2.join(&name)).unwrap();
		assert_eq!(
			content1,
			content2,
			"File {} differs between runs",
			name.to_string_lossy()
		);
	}

	// Cleanup temp dirs
	fs::remove_dir_all(&test_dir1).ok();
	fs::remove_dir_all(&test_dir2).ok();
}

#[test]
fn test_dry_run_no_modifications() {
	let test_dir = setup_test_dir_copy("dry_run");

	// Capture original file contents
	let mut original_contents: Vec<(PathBuf, String)> = Vec::new();
	for entry in fs::read_dir(&test_dir).unwrap() {
		let entry = entry.unwrap();
		let content = fs::read_to_string(entry.path()).unwrap();
		original_contents.push((entry.path(), content));
	}

	let mut order_insensitive = HashSet::new();
	order_insensitive.insert("selections".to_string());

	let config = Config {
		generated_dir: test_dir.clone(),
		shared_module_name: "__shared.ts".to_string(),
		min_occurrences: 2,
		order_insensitive_fields: order_insensitive,
		dry_run: true, // DRY RUN
		verbose: false,
		max_passes: 50,
		compute_gzip: false,
	};

	let mut deduplicator = Deduplicator::new(config);
	let stats = deduplicator.run().unwrap();

	// Should still report stats
	assert!(stats.total_extracted > 0);

	// But no files should be modified
	for (path, original) in original_contents {
		let current = fs::read_to_string(&path).unwrap();
		assert_eq!(
			original,
			current,
			"File {} was modified in dry run",
			path.display()
		);
	}

	// Shared module should NOT exist
	let shared_path = test_dir.join("__shared.ts");
	assert!(
		!shared_path.exists(),
		"Shared module should not be created in dry run"
	);

	// Cleanup
	fs::remove_dir_all(&test_dir).ok();
}

#[test]
fn test_min_occurrences_respected() {
	let test_dir = setup_test_dir_copy("min_occ");

	let mut order_insensitive = HashSet::new();
	order_insensitive.insert("selections".to_string());

	// Set min_occurrences to 50 (higher than any structure appears)
	let config = Config {
		generated_dir: test_dir.clone(),
		shared_module_name: "__shared.ts".to_string(),
		min_occurrences: 50,
		order_insensitive_fields: order_insensitive,
		dry_run: false,
		verbose: false,
		max_passes: 50,
		compute_gzip: false,
	};

	let mut deduplicator = Deduplicator::new(config);
	let stats = deduplicator.run().unwrap();

	// Should extract nothing since no structure appears 50+ times
	assert_eq!(stats.total_extracted, 0);

	// Cleanup
	fs::remove_dir_all(&test_dir).ok();
}

#[test]
fn test_single_child_arrays_are_extracted() {
	// NOTE: Single-child arrays ARE now extracted (gives +11% more savings)
	let test_dir = setup_test_dir_copy("single_child");

	let mut order_insensitive = HashSet::new();
	order_insensitive.insert("selections".to_string());
	order_insensitive.insert("args".to_string());

	let config = Config {
		generated_dir: test_dir.clone(),
		shared_module_name: "__shared.ts".to_string(),
		min_occurrences: 2,
		order_insensitive_fields: order_insensitive,
		dry_run: false,
		verbose: false,
		max_passes: 50,
		compute_gzip: false,
	};

	let mut deduplicator = Deduplicator::new(config);
	let stats = deduplicator.run().unwrap();

	// Read the output file
	let file_one = fs::read_to_string(test_dir.join("FileOne.graphql.ts")).unwrap();
	let _shared = fs::read_to_string(test_dir.join("__shared.ts")).unwrap();

	// Single-child arrays ARE extracted now - file should have refs
	assert!(
		file_one.contains("x_"),
		"File should contain refs after deduplication"
	);

	// Structures were extracted
	assert!(
		stats.total_extracted > 0,
		"Should have extracted some structures"
	);

	// Cleanup
	fs::remove_dir_all(&test_dir).ok();
}

#[test]
fn test_unique_items_not_extracted() {
	let test_dir = setup_test_dir_copy("unique");

	let mut order_insensitive = HashSet::new();
	order_insensitive.insert("selections".to_string());
	order_insensitive.insert("args".to_string());

	let config = Config {
		generated_dir: test_dir.clone(),
		shared_module_name: "__shared.ts".to_string(),
		min_occurrences: 2,
		order_insensitive_fields: order_insensitive,
		dry_run: false,
		verbose: false,
		max_passes: 50,
		compute_gzip: false,
	};

	let mut deduplicator = Deduplicator::new(config);
	deduplicator.run().unwrap();

	// Read output files
	let file_one = fs::read_to_string(test_dir.join("FileOne.graphql.ts")).unwrap();
	let file_two = fs::read_to_string(test_dir.join("FileTwo.graphql.ts")).unwrap();
	let file_three = fs::read_to_string(test_dir.join("FileThree.graphql.ts")).unwrap();
	let shared = fs::read_to_string(test_dir.join("__shared.ts")).unwrap();

	// Unique items should stay inline in their respective files
	assert!(
		file_one.contains("unique_only_in_file_one"),
		"Unique items should stay in original file"
	);
	assert!(
		file_two.contains("unique_only_in_file_two"),
		"Unique items should stay in original file"
	);
	assert!(
		file_three.contains("unique_only_in_file_three"),
		"Unique items should stay in original file"
	);

	// Unique items should NOT appear in shared module
	assert!(
		!shared.contains("unique_only_in_file"),
		"Unique items should not be in shared module"
	);

	// Items appearing in all 3 files SHOULD be in shared
	assert!(
		shared.contains("id_field_in_all_3_files"),
		"Repeated items should be extracted to shared module"
	);

	// Cleanup
	fs::remove_dir_all(&test_dir).ok();
}
