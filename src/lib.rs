//! Relay Artifact Deduplication Library
//!
//! Provides functionality to deduplicate Relay-generated artifact files by
//! extracting repeated structures into a shared module.

pub mod naming;
pub mod normalize;
pub mod relay_config;
pub mod tree;
pub mod writer;

use anyhow::Result;
use flate2::read::GzEncoder;
use flate2::Compression;
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use naming::NameGenerator;
use tree::FileTree;
use writer::write_shared_module;

/// Configuration for the deduplication process
#[derive(Debug, Clone)]
pub struct Config {
	/// Path to the __generated__ directory
	pub generated_dir: PathBuf,
	/// Name of the shared module file (default: __shared.ts)
	pub shared_module_name: String,
	/// Minimum occurrences to extract (default: 2)
	pub min_occurrences: usize,
	/// Fields where array order doesn't matter
	pub order_insensitive_fields: HashSet<String>,
	/// Whether to actually write files
	pub dry_run: bool,
	/// Whether to print verbose output
	pub verbose: bool,
	/// Maximum number of passes to run
	pub max_passes: usize,
	/// Whether to compute gzipped sizes
	pub compute_gzip: bool,
}

impl Default for Config {
	fn default() -> Self {
		let mut order_insensitive = HashSet::new();
		order_insensitive.insert("selections".to_string());
		order_insensitive.insert("args".to_string());
		order_insensitive.insert("argumentDefinitions".to_string());

		Self {
			generated_dir: PathBuf::new(),
			shared_module_name: "__shared.ts".to_string(),
			min_occurrences: 2,
			order_insensitive_fields: order_insensitive,
			dry_run: false,
			verbose: false,
			max_passes: 50,
			compute_gzip: false,
		}
	}
}

/// Statistics about the deduplication process
#[derive(Debug, Default)]
pub struct Stats {
	pub raw_before: u64,
	pub raw_after: u64,
	pub gzipped_before: u64,
	pub gzipped_after: u64,
	pub total_extracted: usize,
	pub passes: usize,
}

impl Stats {
	pub fn raw_savings(&self) -> i64 {
		self.raw_before as i64 - self.raw_after as i64
	}

	pub fn gzipped_savings(&self) -> i64 {
		self.gzipped_before as i64 - self.gzipped_after as i64
	}

	pub fn raw_savings_percent(&self) -> f64 {
		if self.raw_before == 0 {
			0.0
		} else {
			(self.raw_savings() as f64 / self.raw_before as f64) * 100.0
		}
	}

	pub fn gzipped_savings_percent(&self) -> f64 {
		if self.gzipped_before == 0 {
			0.0
		} else {
			(self.gzipped_savings() as f64 / self.gzipped_before as f64) * 100.0
		}
	}
}

/// Entry representing an extracted structure
#[derive(Debug, Clone)]
pub struct ExtractedEntry {
	pub name: String,
	pub hash: String,
	pub count: usize,
}

/// Timing stats for profiling
#[derive(Debug, Default)]
pub struct TimingStats {
	pub file_read: Duration,
	pub tree_parse: Duration,
	pub find_leaves: Duration,
	pub mark_extracted: Duration,
	pub serialize: Duration,
	pub gzip: Duration,
	pub file_write: Duration,
}

/// Main deduplication engine
pub struct Deduplicator {
	config: Config,
	/// Map from normalized content to extracted entry
	extracted: HashMap<String, ExtractedEntry>,
	/// Name generator for short names
	name_generator: NameGenerator,
	/// Tree representation of each file (parse once, mutate in place)
	trees: BTreeMap<PathBuf, FileTree>,
	/// Timing stats
	pub timing: TimingStats,
}

impl Deduplicator {
	pub fn new(config: Config) -> Self {
		Self {
			config,
			extracted: HashMap::new(),
			name_generator: NameGenerator::new(),
			trees: BTreeMap::new(),
			timing: TimingStats::default(),
		}
	}

	/// Run the full deduplication process
	pub fn run(&mut self) -> Result<Stats> {
		let mut stats = Stats::default();

		// Load all files and build trees (parse ONCE)
		self.load_files()?;

		// Calculate initial size
		let (raw, gzipped) = self.calculate_size();
		stats.raw_before = raw;
		stats.gzipped_before = gzipped;

		if self.config.verbose {
			println!("Relay Artifact Deduplication");
			println!("============================");
			println!("\nStarting size:");
			println!("  Raw:     {}", format_bytes(raw));
			println!("  Gzipped: {}", format_bytes(gzipped));
		}

		// Run passes until no more extractions
		loop {
			stats.passes += 1;

			if self.config.verbose {
				println!("\n--- Pass {} ---", stats.passes);
			}

			let extracted = self.run_pass()?;

			if self.config.verbose {
				println!("  Extracted: {}", extracted);
			}

			if extracted == 0 {
				break;
			}

			if stats.passes >= self.config.max_passes {
				if self.config.verbose {
					println!("  Max passes ({}) reached", self.config.max_passes);
				}
				break;
			}
		}

		stats.total_extracted = self.extracted.len();

		// Write all files to disk once at the end
		if !self.config.dry_run {
			self.write_all_files()?;
		}

		// Calculate final size
		let (raw, gzipped) = self.calculate_size();
		stats.raw_after = raw;
		stats.gzipped_after = gzipped;

		if self.config.verbose {
			println!("\n============================");
			println!("Total extracted: {}", stats.total_extracted);
			println!("\nRaw size:");
			println!("  Before:  {}", format_bytes(stats.raw_before));
			println!("  After:   {}", format_bytes(stats.raw_after));
			println!(
				"  Savings: {} ({:.1}%)",
				format_bytes_signed(stats.raw_savings()),
				stats.raw_savings_percent()
			);
			println!("\nGzipped size:");
			println!("  Before:  {}", format_bytes(stats.gzipped_before));
			println!("  After:   {}", format_bytes(stats.gzipped_after));
			println!(
				"  Savings: {} ({:.1}%)",
				format_bytes_signed(stats.gzipped_savings()),
				stats.gzipped_savings_percent()
			);
		}

		Ok(stats)
	}

	/// Load all .graphql.ts files and build tree representations
	fn load_files(&mut self) -> Result<()> {
		self.trees.clear();

		// Collect paths first (sequential - fast)
		let paths: Vec<PathBuf> = fs::read_dir(&self.config.generated_dir)?
			.filter_map(|e| e.ok())
			.map(|e| e.path())
			.filter(|p| {
				p.file_name()
					.and_then(|n| n.to_str())
					.map(|n| n.ends_with(".graphql.ts"))
					.unwrap_or(false)
			})
			.collect();

		// Parallel read and parse
		let order_insensitive = &self.config.order_insensitive_fields;
		let results: Vec<_> = paths
			.par_iter()
			.map(|path| {
				let t_read = Instant::now();
				let content = fs::read_to_string(path).ok()?;
				let read_time = t_read.elapsed();

				let t_parse = Instant::now();
				let tree = FileTree::new(content, order_insensitive);
				let parse_time = t_parse.elapsed();

				Some((path.clone(), tree, read_time, parse_time))
			})
			.collect();

		// Collect results and timing (sequential - fast)
		for result in results.into_iter().flatten() {
			let (path, tree, read_time, parse_time) = result;
			self.timing.file_read += read_time;
			self.timing.tree_parse += parse_time;
			self.trees.insert(path, tree);
		}

		Ok(())
	}

	/// Calculate total size (raw and gzipped) by serializing trees
	fn calculate_size(&mut self) -> (u64, u64) {
		let shared_module_name = &self.config.shared_module_name;
		let compute_gzip = self.config.compute_gzip;

		// Parallel: serialize and optionally gzip each tree
		let results: Vec<_> = self
			.trees
			.par_iter_mut()
			.map(|(_, tree)| {
				let t_ser = Instant::now();
				let content = tree.serialize();
				let content = writer::update_imports(&content, shared_module_name);
				let serialize_time = t_ser.elapsed();

				let bytes = content.as_bytes();
				let raw_size = bytes.len() as u64;

				let (gzip_size, gzip_time) = if compute_gzip {
					let t_gz = Instant::now();
					let mut encoder = GzEncoder::new(bytes, Compression::default());
					let mut compressed = Vec::new();
					let _ = encoder.read_to_end(&mut compressed);
					(compressed.len() as u64, t_gz.elapsed())
				} else {
					(0, Duration::ZERO)
				};

				(raw_size, gzip_size, serialize_time, gzip_time)
			})
			.collect();

		// Sum up results and timing
		let (mut raw, mut gzipped) = (0u64, 0u64);
		for (r, g, ser_time, gz_time) in results {
			raw += r;
			gzipped += g;
			self.timing.serialize += ser_time;
			self.timing.gzip += gz_time;
		}

		// Include shared module (single file, not parallelized)
		if !self.extracted.is_empty() {
			let shared = self.generate_shared_module_content();
			let bytes = shared.as_bytes();
			raw += bytes.len() as u64;

			if compute_gzip {
				let t = Instant::now();
				let mut encoder = GzEncoder::new(bytes, Compression::default());
				let mut compressed = Vec::new();
				let _ = encoder.read_to_end(&mut compressed);
				self.timing.gzip += t.elapsed();
				gzipped += compressed.len() as u64;
			}
		}

		(raw, gzipped)
	}

	/// Run a single pass of deduplication
	fn run_pass(&mut self) -> Result<usize> {
		// Parallel: collect all leaves from all trees
		let t = Instant::now();
		let leaves_by_file: Vec<_> = self
			.trees
			.par_iter()
			.map(|(path, tree)| (path.clone(), tree.find_leaves()))
			.collect();

		// Merge counts (sequential - fast)
		let mut counts: HashMap<String, usize> = HashMap::new();
		for (_, leaves) in &leaves_by_file {
			for (_, normalized) in leaves {
				*counts.entry(normalized.clone()).or_insert(0) += 1;
			}
		}
		self.timing.find_leaves += t.elapsed();

		// Find structures to extract (sequential - required for deterministic naming)
		let mut to_extract: HashMap<String, String> = HashMap::new();

		let mut normalized_list: Vec<_> = counts
			.iter()
			.filter(|(normalized, &count)| {
				count >= self.config.min_occurrences && !self.extracted.contains_key(*normalized)
			})
			.collect();
		normalized_list.sort_by_key(|(normalized, _)| *normalized);

		for (normalized, &count) in normalized_list {
			let hash = hash_string(normalized);
			let name = self.name_generator.next(&hash);
			to_extract.insert(normalized.clone(), name.clone());
			self.extracted
				.insert(normalized.clone(), ExtractedEntry { name, hash, count });
		}

		if to_extract.is_empty() {
			return Ok(0);
		}

		// Parallel: mark nodes as extracted in trees
		let t = Instant::now();
		let leaves_map: HashMap<PathBuf, Vec<(usize, String)>> =
			leaves_by_file.into_iter().collect();
		let order_insensitive = &self.config.order_insensitive_fields;

		self.trees.par_iter_mut().for_each(|(path, tree)| {
			if let Some(leaves) = leaves_map.get(path) {
				for (node_idx, normalized) in leaves {
					if let Some(ref_name) = to_extract.get(normalized) {
						tree.mark_extracted(*node_idx, ref_name.clone(), order_insensitive);
					}
				}
			}
		});
		self.timing.mark_extracted += t.elapsed();

		Ok(to_extract.len())
	}

	/// Generate the shared module content
	fn generate_shared_module_content(&self) -> String {
		writer::generate_shared_module_content(&self.extracted)
	}

	/// Write all files to disk (serialize trees)
	fn write_all_files(&mut self) -> Result<()> {
		let shared_module_name = &self.config.shared_module_name;

		// Parallel: serialize and write each tree
		let results: Vec<_> = self
			.trees
			.par_iter_mut()
			.map(|(path, tree)| {
				let t_ser = Instant::now();
				let content = tree.serialize();
				let content = writer::update_imports(&content, shared_module_name);
				let serialize_time = t_ser.elapsed();

				let t_write = Instant::now();
				let write_result = fs::write(path, content);
				let write_time = t_write.elapsed();

				(write_result, serialize_time, write_time)
			})
			.collect();

		// Check for errors and accumulate timing
		for (result, ser_time, write_time) in results {
			result?;
			self.timing.serialize += ser_time;
			self.timing.file_write += write_time;
		}

		// Write shared module (single file, not parallelized)
		if !self.extracted.is_empty() {
			let shared_path = self
				.config
				.generated_dir
				.join(&self.config.shared_module_name);
			write_shared_module(&shared_path, &self.extracted)?;
		}

		Ok(())
	}
}

/// Hash a string using MD5 and return full 32 hex chars
pub fn hash_string(s: &str) -> String {
	use md5::{Digest, Md5};
	let mut hasher = Md5::new();
	hasher.update(s.as_bytes());
	let result = hasher.finalize();
	format!("{:x}", result)
}

/// Format bytes as human-readable string
pub fn format_bytes(bytes: u64) -> String {
	if bytes >= 1024 * 1024 {
		format!("{:.2} MB", bytes as f64 / 1024.0 / 1024.0)
	} else {
		format!("{} KB", (bytes as f64 / 1024.0).round() as u64)
	}
}

/// Format bytes with sign (for savings that can be negative)
pub fn format_bytes_signed(bytes: i64) -> String {
	let abs = bytes.unsigned_abs();
	let formatted = if abs >= 1024 * 1024 {
		format!("{:.2} MB", abs as f64 / 1024.0 / 1024.0)
	} else {
		format!("{} KB", (abs as f64 / 1024.0).round() as u64)
	};
	if bytes < 0 {
		format!("-{}", formatted)
	} else {
		formatted
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_hash_string() {
		let hash = hash_string("test");
		assert_eq!(hash.len(), 32);
		assert_eq!(hash, hash_string("test"));
		assert_ne!(hash, hash_string("test2"));
	}

	#[test]
	fn test_format_bytes() {
		assert_eq!(format_bytes(256), "0 KB");
		assert_eq!(format_bytes(512), "1 KB");
		assert_eq!(format_bytes(1024), "1 KB");
		assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
		assert_eq!(format_bytes(2 * 1024 * 1024 + 512 * 1024), "2.50 MB");
	}
}
