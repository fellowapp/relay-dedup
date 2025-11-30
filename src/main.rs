//! Relay Artifact Deduplication CLI
//!
//! A tool to deduplicate Relay-generated artifact files by extracting
//! repeated structures into a shared module.

use anyhow::{bail, Result};
use clap::Parser;
use relay_dedup::relay_config::{find_relay_config, validate_relay_config};
use relay_dedup::{Config, Deduplicator};
use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "relay-dedup")]
#[command(author = "Fellow <engineering@fellow.app>")]
#[command(version)]
#[command(about = "Deduplicate Relay-generated artifact files", long_about = None)]
struct Args {
	/// Path to the __generated__ directory (optional if relay config has artifactDirectory)
	#[arg(value_name = "GENERATED_DIR")]
	generated_dir: Option<PathBuf>,

	/// Shared module filename
	#[arg(short, long, default_value = "__shared.ts")]
	output: String,

	/// Show what would change without writing files
	#[arg(short = 'n', long)]
	dry_run: bool,

	/// Print detailed progress and statistics
	#[arg(short, long)]
	verbose: bool,

	/// Minimum occurrences to extract a structure
	#[arg(long, default_value = "2")]
	min_occurrences: usize,

	/// Comma-separated list of order-insensitive field names
	#[arg(long, default_value = "selections,args,argumentDefinitions")]
	order_insensitive: String,

	/// Maximum number of passes to run
	#[arg(long, default_value = "50")]
	max_passes: usize,

	/// Show gzipped size savings in output
	#[arg(long)]
	show_gzip: bool,

	/// Show timing breakdown
	#[arg(long)]
	show_timing: bool,

	/// Skip relay config validation (use with caution)
	#[arg(long)]
	skip_config_check: bool,
}

fn main() -> Result<()> {
	let args = Args::parse();

	// Find relay config
	let cwd = env::current_dir()?;
	let relay_config = find_relay_config(&cwd);

	// Determine the generated directory
	let generated_dir = if let Some(dir) = args.generated_dir.clone() {
		// CLI arg provided - use it
		dir
	} else if let Some(ref config) = relay_config {
		// No CLI arg - try to get from relay config
		config.artifact_directory.clone().ok_or_else(|| {
			anyhow::anyhow!(
				"No GENERATED_DIR specified and relay config ({}) has no artifactDirectory.\n\
				 Either specify a directory: relay-dedup ./src/__generated__\n\
				 Or add artifactDirectory to your relay config.",
				config.config_path.display()
			)
		})?
	} else {
		// No CLI arg and no relay config
		bail!(
			"No GENERATED_DIR specified and no relay config found.\n\
			 Usage: relay-dedup <GENERATED_DIR>\n\
			 Example: relay-dedup ./src/__generated__"
		);
	};

	// Validate relay config (unless skipped)
	if !args.skip_config_check {
		if let Some(ref config) = relay_config {
			validate_relay_config(&config.config_path)?;
		} else {
			eprintln!(
				"Warning: No relay config found (relay.config.json or package.json with 'relay' key).\n\
				 Make sure Relay's built-in deduplication is disabled:\n\
				 featureFlags.disable_deduping_common_structures_in_artifacts = {{ \"kind\": \"enabled\" }}\n"
			);
		}
	}

	// Verify directory exists
	if !generated_dir.exists() {
		bail!(
			"Generated directory does not exist: {}\n\
			 Run relay-compiler first to generate artifacts.",
			generated_dir.display()
		);
	}

	// Parse order-insensitive fields
	let order_insensitive_fields: HashSet<String> = args
		.order_insensitive
		.split(',')
		.map(|s| s.trim().to_string())
		.filter(|s| !s.is_empty())
		.collect();

	// Compute gzip if we need to display it (verbose always shows gzip, or explicit --show-gzip)
	let compute_gzip = args.verbose || args.show_gzip;

	let config = Config {
		generated_dir,
		shared_module_name: args.output,
		min_occurrences: args.min_occurrences,
		order_insensitive_fields,
		dry_run: args.dry_run,
		verbose: args.verbose,
		max_passes: args.max_passes,
		compute_gzip,
	};

	let start_time = Instant::now();
	let mut deduplicator = Deduplicator::new(config);
	let stats = deduplicator.run()?;
	let total_time = start_time.elapsed();

	let time_str = format!("{:.2}s", total_time.as_secs_f64());

	// Always print summary (even if not verbose)
	if !args.verbose {
		if args.show_gzip {
			println!(
				"Extracted {} structures, saved {} raw ({:.1}%), {} gzipped ({:.1}%) in {}",
				stats.total_extracted,
				relay_dedup::format_bytes_signed(stats.raw_savings()),
				stats.raw_savings_percent(),
				relay_dedup::format_bytes_signed(stats.gzipped_savings()),
				stats.gzipped_savings_percent(),
				time_str
			);
		} else {
			println!(
				"Extracted {} structures, saved {} raw ({:.1}%) in {}",
				stats.total_extracted,
				relay_dedup::format_bytes_signed(stats.raw_savings()),
				stats.raw_savings_percent(),
				time_str
			);
		}
	} else {
		// Verbose mode prints its own detailed output, just add total time
		println!("\nTotal time: {}", time_str);
	}

	// Print timing breakdown if requested
	if args.show_timing {
		let t = &deduplicator.timing;
		let total_io = t.file_read.as_secs_f64() + t.file_write.as_secs_f64();
		let mut total_cpu = t.tree_parse.as_secs_f64()
			+ t.find_leaves.as_secs_f64()
			+ t.mark_extracted.as_secs_f64()
			+ t.serialize.as_secs_f64();
		if compute_gzip {
			total_cpu += t.gzip.as_secs_f64();
		}

		eprintln!("\n=== Timing breakdown ===");
		eprintln!("I/O:");
		eprintln!(
			"  file_read:      {:>7.1}ms",
			t.file_read.as_secs_f64() * 1000.0
		);
		eprintln!(
			"  file_write:     {:>7.1}ms",
			t.file_write.as_secs_f64() * 1000.0
		);
		eprintln!("  --- total I/O:  {:>7.1}ms", total_io * 1000.0);
		eprintln!("CPU:");
		eprintln!(
			"  tree_parse:     {:>7.1}ms",
			t.tree_parse.as_secs_f64() * 1000.0
		);
		eprintln!(
			"  find_leaves:    {:>7.1}ms",
			t.find_leaves.as_secs_f64() * 1000.0
		);
		eprintln!(
			"  mark_extracted: {:>7.1}ms",
			t.mark_extracted.as_secs_f64() * 1000.0
		);
		eprintln!(
			"  serialize:      {:>7.1}ms",
			t.serialize.as_secs_f64() * 1000.0
		);
		if compute_gzip {
			eprintln!("  gzip:           {:>7.1}ms", t.gzip.as_secs_f64() * 1000.0);
		}
		eprintln!("  --- total CPU:  {:>7.1}ms", total_cpu * 1000.0);
	}

	Ok(())
}
