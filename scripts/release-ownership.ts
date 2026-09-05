// Single registry for Krometrail product-version ownership.
//
// Cargo.toml's root [package].version is the sole product version authority.
// Everything else that ships a product version is a derived projection
// registered here, and workspace membership alone never confers version
// ownership: a member must explicitly inherit [workspace.package].version to
// move with a product release. A member without that explicit inheritance —
// any version literal, single-quoted or double-quoted, or no version at all
// (Cargo defaults it to 0.0.0), such as temporal-vision — is versioned and
// published independently, so a product release must leave its manifest and
// Cargo.lock entry untouched.
//
// scripts/bump-version.ts imports this module for its release transaction, and
// tests/distribution-static.sh reads the same exports so fixture expectations
// are derived from one registry instead of duplicated lists.

import { readdirSync, readFileSync, statSync } from "node:fs";

export type ProjectionFormat = "json" | "text";

export type VersionProjection = {
	/** Repository-relative path of the shipped file. */
	path: string;
	/**
	 * "text" files are exactly the version plus a trailing newline;
	 * "json" files carry exactly one semver-string "version" field.
	 */
	format: ProjectionFormat;
};

/**
 * Every shipped file whose content is the product version. Registering a new
 * version surface here is a deliberate release-contract decision: the bump
 * helper rejects any version-bearing file in the shipped surface that this
 * inventory does not list, so a new projection cannot be silently skipped.
 */
export const PRODUCT_VERSION_PROJECTIONS: VersionProjection[] = [
	{ path: "plugin/.claude-plugin/plugin.json", format: "json" },
	{ path: "plugin/.codex-plugin/plugin.json", format: "json" },
	{ path: "plugin/plugin.json", format: "json" },
	{ path: ".claude-plugin/marketplace.json", format: "json" },
	{ path: ".agents/plugins/marketplace.json", format: "json" },
	{ path: "plugin/version", format: "text" },
];

/**
 * Repository directories whose contents ship to plugin hosts and catalogs.
 * The unregistered-projection scan is bounded to this surface so unrelated
 * version strings (docs, fixtures, lockfiles) can never block a release.
 */
export const SHIPPED_SURFACE_DIRS = ["plugin", ".claude-plugin", ".agents/plugins"];

const semverPattern = /^\d+\.\d+\.\d+$/;
const jsonVersionFieldPattern = /"version"\s*:\s*"([^"]+)"/g;
const jsonVersionReplacementPattern = /("version"\s*:\s*")[^"]+("\s*)/;

// Shared narrow section reader: release behavior depends on the exact headers
// this helper names, not on a general TOML parser. Exported so the bump helper
// and the ownership classification parse manifests identically.
export type TomlSection = {
	start: number;
	end: number;
	content: string;
};

export function findTomlSection(source: string, header: string): TomlSection | undefined {
	const start = source.indexOf(header);
	if (start < 0) return undefined;

	const contentStart = start + header.length;
	const nextSectionOffset = source.slice(contentStart).search(/^\[/m);
	const end = nextSectionOffset < 0 ? source.length : contentStart + nextSectionOffset;
	return { start, end, content: source.slice(start, end) };
}

/**
 * True exactly when the manifest explicitly inherits the workspace version:
 * `version.workspace = true` in the `[package]` section, in dotted or
 * inline-table form (both resolve to `{ workspace = true }` under TOML).
 *
 * Everything else is independently versioned and must never move with a
 * product release: a double-quoted or single-quoted literal, or a missing
 * version, which Cargo defaults to 0.0.0. Absence of a regex match is not
 * evidence of inheritance — classification parses the manifest instead.
 */
export function inheritsWorkspaceVersion(manifestText: string): boolean {
	const manifest = Bun.TOML.parse(manifestText) as {
		package?: { version?: { workspace?: unknown } };
	};
	const version = manifest.package?.version;
	return typeof version === "object" && version !== null && version.workspace === true;
}

export type ProjectionUpdate = {
	path: string;
	original: string;
	updated: string;
};

function requireProjectionContent(projection: VersionProjection, content: string, expectedVersion: string): void {
	if (projection.format === "text") {
		if (content !== `${expectedVersion}\n`) {
			throw new Error(`${projection.path} must contain exactly ${expectedVersion}`);
		}
		return;
	}
	const matches = [...content.matchAll(jsonVersionFieldPattern)];
	if (matches.length !== 1 || matches[0][1] !== expectedVersion) {
		throw new Error(`${projection.path} must contain exactly one version equal to ${expectedVersion}`);
	}
}

async function readProjection(rootDir: string, projection: VersionProjection): Promise<string> {
	const file = Bun.file(`${rootDir}/${projection.path}`);
	if (!(await file.exists())) {
		throw new Error(`Registered product version projection ${projection.path} is missing from the repository`);
	}
	return file.text();
}

/**
 * Validate every registered projection against `expectedVersion` without
 * changing anything. Used by dry-run validation and by tests that assert the
 * shipped surface landed on the released version.
 */
export async function assertProjectionsAtVersion(rootDir: string, expectedVersion: string): Promise<void> {
	for (const projection of PRODUCT_VERSION_PROJECTIONS) {
		requireProjectionContent(projection, await readProjection(rootDir, projection), expectedVersion);
	}
}

/**
 * Build the atomic projection rewrites for one release transaction. Every
 * registered projection must currently carry `currentVersion`; anything else
 * is an inconsistent product-owned input and rejects before any mutation.
 */
export async function prepareProjectionUpdates(
	rootDir: string,
	currentVersion: string,
	nextVersion: string,
): Promise<ProjectionUpdate[]> {
	const updates: ProjectionUpdate[] = [];
	for (const projection of PRODUCT_VERSION_PROJECTIONS) {
		const original = await readProjection(rootDir, projection);
		requireProjectionContent(projection, original, currentVersion);
		const updated = projection.format === "text"
			? `${nextVersion}\n`
			: original.replace(jsonVersionReplacementPattern, `$1${nextVersion}$2`);
		updates.push({ path: projection.path, original, updated });
	}
	return updates;
}

function carriesSemverVersion(value: unknown): boolean {
	if (Array.isArray(value)) return value.some(carriesSemverVersion);
	if (value !== null && typeof value === "object") {
		return Object.entries(value).some(
			([key, child]) => (key === "version" && typeof child === "string" && semverPattern.test(child)) ||
				carriesSemverVersion(child),
		);
	}
	return false;
}

function jsonCarriesSemverVersion(content: string): boolean {
	try {
		return carriesSemverVersion(JSON.parse(content));
	} catch {
		// Unparseable JSON in the shipped surface is not a version projection;
		// registered paths still fail validation through prepareProjectionUpdates.
		return false;
	}
}

function collectSurfaceFiles(rootDir: string, relativeDir: string, files: string[]): void {
	const absoluteDir = `${rootDir}/${relativeDir}`;
	let entries;
	try {
		entries = readdirSync(absoluteDir, { withFileTypes: true });
	} catch {
		// Fixture repos may ship no surface at all. A non-directory path in the
		// surface's position is repo damage that projection validation reports
		// through its own missing-file error instead.
		return;
	}
	for (const entry of entries) {
		const relative = `${relativeDir}/${entry.name}`;
		if (entry.isDirectory()) {
			collectSurfaceFiles(rootDir, relative, files);
		} else if (entry.isFile()) {
			files.push(relative);
		}
	}
}

/**
 * Version-bearing files under the shipped surface that the inventory does not
 * list: any file named `version` plus JSON files carrying a semver-string
 * "version" field. A nonempty result means a shipped version surface would
 * silently drift from the product release.
 */
export function findUnregisteredVersionProjections(rootDir: string): string[] {
	const registered = new Set(PRODUCT_VERSION_PROJECTIONS.map((projection) => projection.path));
	const files: string[] = [];
	for (const dir of SHIPPED_SURFACE_DIRS) {
		// statSync guards against a file occupying a surface directory's name.
		try {
			if (!statSync(`${rootDir}/${dir}`).isDirectory()) continue;
		} catch {
			continue;
		}
		collectSurfaceFiles(rootDir, dir, files);
	}
	const unregistered: string[] = [];
	for (const relative of files) {
		if (registered.has(relative)) continue;
		const fileName = relative.split("/").pop() ?? relative;
		if (fileName === "version") {
			unregistered.push(relative);
			continue;
		}
		if (relative.endsWith(".json") && jsonCarriesSemverVersion(readFileSync(`${rootDir}/${relative}`, "utf8"))) {
			unregistered.push(relative);
		}
	}
	return unregistered.sort();
}
