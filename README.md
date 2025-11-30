# relay-dedup

A high-performance tool to deduplicate Relay-generated artifact files, reducing bundle size by 60-70%.

## Installation

```bash
# pnpm
pnpm add github:fellowapp/relay-dedup

# npm
npm install github:fellowapp/relay-dedup

# yarn
yarn add github:fellowapp/relay-dedup
```

The postinstall script automatically downloads the correct pre-built binary for your platform (macOS/Linux, x64/ARM64).

## Usage

```bash
# Run on your __generated__ directory
npx relay-dedup ./src/__generated__

# Dry run (see what would change without writing)
npx relay-dedup ./src/__generated__ --dry-run

# Verbose output with timing
npx relay-dedup ./src/__generated__ --verbose
```

### Recommended: Combined Build Script

Add this to your `package.json` to run deduplication automatically after Relay compilation:

```json
{
  "scripts": {
    "relay:compile": "relay-compiler",
    "relay:dedup": "relay-dedup ./src/__generated__",
    "relay": "pnpm relay:compile && pnpm relay:dedup"
  }
}
```

Now `pnpm relay` handles both compilation and deduplication in one command.

### Required Relay Configuration

You **must** disable Relay's built-in deduplication, which conflicts with this tool. Add these feature flags to your `relay.config.json`:

```json
{
  "featureFlags": {
    "disable_deduping_common_structures_in_artifacts": { "kind": "enabled" },
    "enforce_fragment_alias_where_ambiguous": { "kind": "disabled" }
  }
}
```

| Flag                                              | Why                                                                                                                                      |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `disable_deduping_common_structures_in_artifacts` | **Required.** Relay has its own dedup that produces different (less optimal) output. Must be disabled.                                   |
| `enforce_fragment_alias_where_ambiguous`          | Workaround for a Relay bug where defining _any_ feature flags enables strict alias checking. May not be needed in future Relay versions. |

The CLI will automatically detect your relay config and validate these flags. If they're not set correctly, it will error with instructions.

## What It Does

Relay generates a `.graphql.ts` file for every GraphQL operation and fragment in your codebase. These files contain JSON-like structures that describe your queries. The problem? **Massive duplication.**

A typical field selection like `{ "kind": "ScalarField", "name": "id", "storageKey": null }` might appear thousands of times across your generated files. This tool:

1. **Scans** all `.graphql.ts` files in your `__generated__` directory
2. **Identifies** repeated structures (objects and arrays appearing 2+ times)
3. **Extracts** them into a shared module (`__shared.ts`)
4. **Replaces** each occurrence with a reference to the shared export

### Before

```typescript
// UserQuery.graphql.ts
const node = {
  kind: "Field",
  name: "user",
  selections: [
    { kind: "ScalarField", name: "id", storageKey: null },
    { kind: "ScalarField", name: "name", storageKey: null },
  ],
};
```

### After

```typescript
// __shared.ts
export const x_abc = { kind: "ScalarField", name: "id", storageKey: null };
export const x_def = { kind: "ScalarField", name: "name", storageKey: null };

// UserQuery.graphql.ts
import { x_abc, x_def } from "./__shared";
const node = {
  kind: "Field",
  name: "user",
  selections: [x_abc, x_def],
};
```

## How It Works

### The Algorithm

1. **Parse Once**: Build a tree representation of each file's structure (objects/arrays)
2. **Find Leaves**: Identify "leaf" nodes (structures with no un-extracted children)
3. **Normalize**: Create a canonical form for comparison (strip whitespace, sort keys/elements where order doesn't matter)
4. **Count**: Track occurrences of each normalized structure across all files
5. **Extract**: Structures appearing ≥2 times get a short name (`x_abc`) and move to `__shared.ts`
6. **Repeat**: Previous parents may now be leaves—run multiple passes until no more extractions

### Multi-Pass Extraction

The key insight is that extraction is hierarchical. Consider:

```javascript
{
  "selections": [
    { "kind": "ScalarField", "name": "id" },
    { "kind": "ScalarField", "name": "name" }
  ]
}
```

**Pass 1**: Extract the inner objects → `[x_id, x_name]`

**Pass 2**: Now the entire `selections` array might match other files → extract it too

This cascading extraction is why we see 60-70% reduction instead of 20-30%.

### Order-Insensitive Normalization

Some arrays have semantic ordering that matters (e.g., `LinkedField.selections` determines render order). Others don't:

- `selections` - field order in a selection set doesn't affect semantics
- `args` - argument order doesn't matter
- `argumentDefinitions` - variable definition order doesn't matter

For these fields, array elements are sorted before comparison. This catches duplicates that differ only in element order.

### Name Generation

Extracted structures get short, deterministic names based on their content hash:

```
x_abc  → first 3 hex chars of MD5
x_abcd → if x_abc is taken, use 4 chars
x_abcde → and so on...
```

Names are deterministic (same content → same name) and stable across runs.

## CLI Options

```
relay-dedup [OPTIONS] [GENERATED_DIR]

Arguments:
  [GENERATED_DIR]           Path to the __generated__ directory
                            (optional if relay config has artifactDirectory)

Options:
  -o, --output <FILE>       Shared module filename [default: __shared.ts]
  -n, --dry-run             Show what would change without writing files
  -v, --verbose             Print detailed progress and statistics
      --min-occurrences <N> Minimum occurrences to extract [default: 2]
      --order-insensitive   Comma-separated field names where array order
                            doesn't matter [default: selections,args,argumentDefinitions]
      --max-passes <N>      Maximum extraction passes [default: 50]
      --show-gzip           Show gzipped size savings
      --show-timing         Show timing breakdown
      --skip-config-check   Skip relay config validation (use with caution)
  -h, --help                Print help
  -V, --version             Print version
```

### Relay Config Detection

The CLI automatically searches for relay configuration by looking upward from the current directory for:

1. `relay.config.json`
2. `package.json` with a `"relay"` key

If found, it:

- **Validates** the required feature flags are set (errors if not)
- **Uses** `artifactDirectory` as the default if no directory is specified

If not found, it prints a warning and requires you to specify the directory explicitly.

## Performance

Tested on a real-world codebase with 1,668 Relay artifacts:

| Metric                 | Value               |
| ---------------------- | ------------------- |
| Structures extracted   | 8,855               |
| Raw size reduction     | 71% (16.7 MB saved) |
| Gzipped size reduction | ~40%                |
| Execution time         | 0.35s               |

The Rust implementation is ~6x faster than the equivalent TypeScript version.

## How It Integrates with Your Bundle

The deduplication is purely at the source level—Relay's runtime behavior is unchanged. The generated code:

1. Imports shared structures from `__shared.ts`
2. References them by variable name instead of inline literals
3. Produces identical runtime behavior

Your bundler (webpack, esbuild, vite) handles the rest. The shared module gets bundled once, and references become pointer lookups instead of duplicated object literals.

## Platform Support

Pre-built binaries are available for:

- macOS (Apple Silicon / ARM64)
- macOS (Intel / x64)
- Linux (x64, statically linked)
- Linux (ARM64, statically linked)

Linux binaries use musl for static linking—they work on any Linux distribution without glibc dependencies.

## Development

```bash
# Run tests
cargo test

# Build release
cargo build --release

# Build all platforms locally (requires Docker for Linux)
./scripts/build-all.sh
```

## License

MIT
