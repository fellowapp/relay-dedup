#!/usr/bin/env node

const https = require("https");
const fs = require("fs");
const path = require("path");
const { execSync } = require("child_process");
const zlib = require("zlib");

// GitHub repo info
const REPO = "fellowapp/relay-dedup";
const BINARY_NAME = "relay-dedup";

// Map Node.js platform/arch to Rust target
const PLATFORM_MAP = {
	"darwin-arm64": "aarch64-apple-darwin",
	"darwin-x64": "x86_64-apple-darwin",
	"linux-arm64": "aarch64-unknown-linux-musl",
	"linux-x64": "x86_64-unknown-linux-musl",
};

function getPlatformKey() {
	const platform = process.platform;
	const arch = process.arch;
	return `${platform}-${arch}`;
}

function getTarget() {
	const key = getPlatformKey();
	const target = PLATFORM_MAP[key];
	if (!target) {
		throw new Error(
			`Unsupported platform: ${key}. Supported: ${Object.keys(PLATFORM_MAP).join(", ")}`
		);
	}
	return target;
}

function getVersion() {
	const pkg = require("../package.json");
	return pkg.version;
}

function httpsGet(url) {
	return new Promise((resolve, reject) => {
		https
			.get(url, { headers: { "User-Agent": "relay-dedup-installer" } }, (res) => {
				// Follow redirects
				if (res.statusCode === 301 || res.statusCode === 302) {
					return httpsGet(res.headers.location).then(resolve).catch(reject);
				}
				if (res.statusCode !== 200) {
					reject(new Error(`HTTP ${res.statusCode}: ${url}`));
					return;
				}
				const chunks = [];
				res.on("data", (chunk) => chunks.push(chunk));
				res.on("end", () => resolve(Buffer.concat(chunks)));
				res.on("error", reject);
			})
			.on("error", reject);
	});
}

async function downloadBinary() {
	const target = getTarget();
	const version = getVersion();
	const assetName = `${BINARY_NAME}-${target}.tar.gz`;
	const url = `https://github.com/${REPO}/releases/download/v${version}/${assetName}`;

	console.log(`Downloading ${BINARY_NAME} v${version} for ${getPlatformKey()}...`);
	console.log(`  ${url}`);

	try {
		const tarGz = await httpsGet(url);

		// Extract tar.gz
		const tar = zlib.gunzipSync(tarGz);

		// Simple tar extraction (binary is first file, starts at byte 512)
		// TAR header is 512 bytes, then file content
		const binaryContent = extractTarFile(tar, BINARY_NAME);

		// Write binary
		const binDir = path.join(__dirname, "..", "bin");
		const binaryPath = path.join(binDir, "relay-dedup-binary");

		fs.mkdirSync(binDir, { recursive: true });
		fs.writeFileSync(binaryPath, binaryContent);
		fs.chmodSync(binaryPath, 0o755);

		console.log(`✓ Installed ${BINARY_NAME} to ${binaryPath}`);
	} catch (error) {
		if (error.message.includes("404")) {
			console.error(`\nError: No prebuilt binary found for ${getPlatformKey()}`);
			console.error(`Release v${version} may not exist or may not have binaries yet.`);
			console.error(`\nYou can build from source with: cargo build --release`);
		} else {
			console.error(`\nError downloading binary: ${error.message}`);
		}
		process.exit(1);
	}
}

function extractTarFile(tarBuffer, filename) {
	// TAR format: 512-byte headers followed by file content (padded to 512)
	let offset = 0;
	while (offset < tarBuffer.length) {
		// Read filename from header (first 100 bytes, null-terminated)
		const header = tarBuffer.slice(offset, offset + 512);
		const name = header.slice(0, 100).toString("utf8").replace(/\0.*$/, "");

		if (!name) break; // Empty header = end of archive

		// Read file size from header (bytes 124-136, octal)
		const sizeStr = header.slice(124, 136).toString("utf8").replace(/\0.*$/, "").trim();
		const size = parseInt(sizeStr, 8);

		offset += 512; // Move past header

		if (name === filename || name === `./${filename}`) {
			return tarBuffer.slice(offset, offset + size);
		}

		// Skip to next header (content is padded to 512-byte boundary)
		offset += Math.ceil(size / 512) * 512;
	}

	throw new Error(`File ${filename} not found in tar archive`);
}

downloadBinary();

