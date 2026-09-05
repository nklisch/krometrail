#!/usr/bin/env bun
// Usage: bun scripts/bump-version.ts [patch|minor|major|x.y.z] [--prepare|--dry-run]

import {
	findTomlSection,
	inheritsWorkspaceVersion,
	findUnregisteredVersionProjections,
	prepareProjectionUpdates,
	type ProjectionUpdate,
} from "./release-ownership.ts";

const args = [...process.argv.slice(2)];
const prepare = args.includes("--prepare");
const dryRun = args.includes("--dry-run");
const mode = prepare ? "prepare" : dryRun ? "dry-run" : "release";
const bump = args.find((arg) => !arg.startsWith("--"));

if (!bump || (prepare && dryRun) || args.some((arg) => arg.startsWith("--") && arg !== "--prepare" && arg !== "--dry-run")) {
	console.error("Usage: bun scripts/bump-version.ts [patch|minor|major|x.y.z] [--prepare|--dry-run]");
	process.exit(1);
}

// Section discovery stays intentionally narrow; the reader lives in
// scripts/release-ownership.ts so every manifest consumer parses identically.
const semverRe = /^(\d+)\.(\d+)\.(\d+)$/;
const cargoPath = "Cargo.toml";
const cargoFile = Bun.file(cargoPath);
const originalCargo = await cargoFile.text();
const packageSectionBounds = findTomlSection(originalCargo, "[package]");
if (!packageSectionBounds) {
	throw new Error("Cargo.toml is missing the root [package] section");
}

const packageStart = packageSectionBounds.start;
const packageEnd = packageSectionBounds.end;
const packageSection = packageSectionBounds.content;
const nameAssignments = [...packageSection.matchAll(/^\s*name\s*=\s*"([^"]+)"\s*(?:#.*)?$/gm)];
if (nameAssignments.length !== 1) {
	throw new Error(`Expected exactly one root [package].name assignment, found ${nameAssignments.length}`);
}
const rootPackageName = nameAssignments[0][1];
const versionAssignments = [...packageSection.matchAll(/^\s*version\s*=\s*"([^"]+)"\s*(?:#.*)?$/gm)];
if (versionAssignments.length !== 1) {
	throw new Error(`Expected exactly one root [package].version assignment, found ${versionAssignments.length}`);
}

const current = versionAssignments[0][1];
const currentMatch = current.match(semverRe);
if (!currentMatch) {
	throw new Error(`Could not parse current version: ${current}`);
}

const [, majorText, minorText, patchText] = currentMatch;
const major = Number.parseInt(majorText, 10);
const minor = Number.parseInt(minorText, 10);
const patch = Number.parseInt(patchText, 10);
let nextVersion: string;

if (bump === "patch") {
	nextVersion = `${major}.${minor}.${patch + 1}`;
} else if (bump === "minor") {
	nextVersion = `${major}.${minor + 1}.0`;
} else if (bump === "major") {
	nextVersion = `${major + 1}.0.0`;
} else if (semverRe.test(bump)) {
	nextVersion = bump;
} else {
	throw new Error(`Invalid argument: ${bump}. Expected patch, minor, major, or x.y.z`);
}

const versionLine = /^(\s*version\s*=\s*")[^"]+("\s*(?:#.*)?$)/m;
const updatedPackageSection = packageSection.replace(versionLine, `$1${nextVersion}$2`);
if (updatedPackageSection === packageSection) {
	throw new Error("Failed to update the root [package].version assignment");
}
let updatedCargo = originalCargo.slice(0, packageStart) + updatedPackageSection + originalCargo.slice(packageEnd);

// Workspace members inherit their crate version from [workspace.package]. Keep
// that Cargo-owned metadata in sync without treating it as another product
// version source. Krometrail plugin versions are derived separately below.
const workspacePackage = findTomlSection(updatedCargo, "[workspace.package]");
if (workspacePackage) {
	const workspaceVersions = [...workspacePackage.content.matchAll(/^\s*version\s*=\s*"([^"]+)"\s*(?:#.*)?$/gm)];
	if (workspaceVersions.length !== 1 || workspaceVersions[0][1] !== current) {
		throw new Error("[workspace.package].version must contain exactly the current root package version");
	}
	const updatedWorkspaceSection = workspacePackage.content.replace(versionLine, `$1${nextVersion}$2`);
	updatedCargo = updatedCargo.slice(0, workspacePackage.start) + updatedWorkspaceSection + updatedCargo.slice(workspacePackage.end);
}

// Workspace membership does not confer product-version ownership. The root
// package is the explicit product version authority, and a member joins it
// exactly when it explicitly inherits [workspace.package].version (dotted or
// inline-table form). A member without that explicit inheritance is
// independently versioned — literal or single-quoted versions, or no version
// at all (Cargo defaults to 0.0.0) — so its manifest and Cargo.lock entry must
// move only through its own release flow.
const productOwnedLockNames = [rootPackageName];
const independentMemberNames: string[] = [];

// temporal-vision is versioned and published independently of the workspace
// (see docs/RELEASING.md). Guard the decoupling with the same classifier the
// ownership split uses, so recoupling in any TOML shape — dotted
// `version.workspace = true` or the inline table — is refused by name. This
// script must never be the thing that moves its version.
{
	// The crate may be absent when this script runs inside the bare fixture
	// repos used by tests/distribution-static.sh; there is nothing to guard
	// there, so only enforce the decoupling when the manifest exists.
	const tvManifest = Bun.file("crates/temporal-vision/Cargo.toml");
	const tvCargo = (await tvManifest.exists()) ? await tvManifest.text() : "";
	if (tvCargo !== "" && inheritsWorkspaceVersion(tvCargo)) {
		throw new Error(
			"crates/temporal-vision is versioned independently — do not recouple it to the workspace version; bump it manually per docs/RELEASING.md",
		);
	}
}

const workspaceMembers = findTomlSection(originalCargo, "[workspace]");
if (workspaceMembers) {
	const membersMatch = workspaceMembers.content.match(/members\s*=\s*\[([\s\S]*?)\]/m);
	if (membersMatch) {
		for (const memberPath of membersMatch[1].matchAll(/"([^"]+)"/g)) {
			const memberCargo = await Bun.file(`${memberPath[1]}/Cargo.toml`).text();
			const memberPackage = findTomlSection(memberCargo, "[package]");
			if (!memberPackage) {
				throw new Error(`Workspace member ${memberPath[1]} is missing a root package name`);
			}
			const memberName = memberPackage.content.match(/^\s*name\s*=\s*"([^"]+)"\s*(?:#.*)?$/m)?.[1];
			if (!memberName) {
				throw new Error(`Workspace member ${memberPath[1]} is missing a root package name`);
			}
			if (inheritsWorkspaceVersion(memberCargo)) {
				productOwnedLockNames.push(memberName);
			} else {
				independentMemberNames.push(memberName);
			}
		}
	}
}

console.log(`Bumping ${current} → ${nextVersion}`);

// Cargo.toml remains the sole version authority. Registered version
// projections are distribution surfaces that must move atomically with a
// Krometrail release so the plugin launcher installs the exact binary its
// package declares. Validate them — and reject any unregistered shipped
// version surface — before anything mutates, including under --dry-run.
let derivedVersionUpdates: ProjectionUpdate[] = [];
if (rootPackageName === "krometrail") {
	const unregistered = findUnregisteredVersionProjections(".");
	if (unregistered.length > 0) {
		throw new Error(
			`Unregistered shipped version projection(s): ${unregistered.join(", ")} — register them in scripts/release-ownership.ts so every release moves them`,
		);
	}
	derivedVersionUpdates = await prepareProjectionUpdates(".", current, nextVersion);
}

if (mode === "dry-run") {
	console.log(`Dry run validated ${derivedVersionUpdates.length} registered version projection(s); no files, commits, tags, or pushes changed.`);
	process.exit(0);
}

function run(command: string[], capture = false): string {
	const result = Bun.spawnSync(command, {
		stdout: capture ? "pipe" : "inherit",
		stderr: "inherit",
	});
	if (!result.success) {
		throw new Error(`Command failed (${result.exitCode}): ${command.join(" ")}`);
	}
	return capture ? new TextDecoder().decode(result.stdout) : "";
}

if (mode === "release" && run(["git", "status", "--porcelain"], true).trim() !== "") {
	throw new Error("Working tree must be clean before creating a release");
}

const lockPath = "Cargo.lock";
const originalLock = (await Bun.file(lockPath).exists()) ? await Bun.file(lockPath).text() : undefined;

type LockPackage = {
	name: string;
	version: string;
	source: string;
	checksum: string;
	section: string;
};

function lockPackages(lock: string): LockPackage[] {
	const normalized = lock.replaceAll("\r\n", "\n");
	return normalized.split(/^\[\[package\]\]\n/m).slice(1).map((body) => {
		const section = `[[package]]\n${body}`;
		const readField = (field: string): string | undefined => section.match(new RegExp(`^${field} = "([^"]*)"$`, "m"))?.[1];
		const name = readField("name");
		const version = readField("version");
		if (!name || !version) throw new Error("Cargo.lock contains a package without a name or version");
		return {
			name,
			version,
			source: readField("source") ?? "",
			checksum: readField("checksum") ?? "",
			section: section.trimEnd(),
		};
	});
}

function packageMultiset(packages: LockPackage[]): Map<string, number> {
	const multiset = new Map<string, number>();
	for (const pkg of packages) {
		// Include the complete record as well as identity fields. Cargo.lock can
		// contain duplicate names from different sources or versions.
		const identity = [pkg.name, pkg.version, pkg.source, pkg.checksum].join("\u0000");
		const key = `${identity}\u0000${pkg.section}`;
		multiset.set(key, (multiset.get(key) ?? 0) + 1);
	}
	return multiset;
}

function sameMultiset(left: Map<string, number>, right: Map<string, number>): boolean {
	if (left.size !== right.size) return false;
	for (const [key, count] of left) if (right.get(key) !== count) return false;
	return true;
}

function isProductOwnedPackage(pkg: LockPackage): boolean {
	return productOwnedLockNames.includes(pkg.name) && pkg.source === "";
}

function verifyLockRefresh(original: string | undefined, updated: string, expectedVersion: string): void {
	const originalPackages = original === undefined ? [] : lockPackages(original);
	const updatedPackages = lockPackages(updated);
	if (original !== undefined) {
		const originalHeader = original.replaceAll("\r\n", "\n").split("[[package]]", 1)[0];
		const updatedHeader = updated.replaceAll("\r\n", "\n").split("[[package]]", 1)[0];
		if (originalHeader !== updatedHeader) throw new Error("Cargo.lock header changed during the narrow version refresh");
	}
	for (const packageName of productOwnedLockNames) {
		const before = originalPackages.filter((pkg) => pkg.name === packageName && isProductOwnedPackage(pkg));
		const after = updatedPackages.filter((pkg) => pkg.name === packageName && isProductOwnedPackage(pkg));
		if (after.length !== 1) throw new Error(`Cargo.lock must contain exactly one product-owned package ${packageName}`);
		if (before.length === 1 && before[0].version !== current) {
			throw new Error(`Cargo.lock product-owned package ${packageName} did not start at ${current}`);
		}
		if (before.length > 1) throw new Error(`Cargo.lock contains duplicate product-owned package ${packageName}`);
		if (after[0].version !== expectedVersion) {
			throw new Error(`Cargo.lock product-owned package ${packageName} was not refreshed to ${expectedVersion}`);
		}
	}
	// Independently versioned members keep their lock entries byte-identical
	// through a product refresh: workspace membership is not version ownership.
	for (const packageName of independentMemberNames) {
		const before = originalPackages.filter((pkg) => pkg.name === packageName && pkg.source === "");
		const after = updatedPackages.filter((pkg) => pkg.name === packageName && pkg.source === "");
		if (before.length !== 1 || after.length !== 1 || before[0].section !== after[0].section) {
			throw new Error(
				`Cargo.lock entry for independently versioned package ${packageName} changed during the product refresh; bump it through its own release flow`,
			);
		}
	}
	if (original !== undefined) {
		const normalizedMultiset = (packages: LockPackage[]): Map<string, number> => packageMultiset(packages.map((pkg) => {
			if (!isProductOwnedPackage(pkg)) return pkg;
			return { ...pkg, version: "<product-version>", section: pkg.section.replace(/^version = "[^"]+"$/m, 'version = "<product-version>"') };
		}));
		if (!sameMultiset(normalizedMultiset(originalPackages), normalizedMultiset(updatedPackages))) {
			throw new Error("Cargo.lock package multiset changed outside expected product version updates");
		}
	}
}

await Bun.write(cargoPath, updatedCargo);

try {
	for (const update of derivedVersionUpdates) {
		await Bun.write(update.path, update.updated);
	}
	console.log("Refreshing only product-owned package versions in Cargo.lock...");
	run(["cargo", "update", "-p", rootPackageName, "--precise", nextVersion]);
	const refreshedLock = await Bun.file(lockPath).text();
	verifyLockRefresh(originalLock, refreshedLock, nextVersion);

	console.log("Running locked Rust release checks...");
	run(["cargo", "fmt", "--all", "--check"]);
	run(["cargo", "check", "--workspace", "--all-targets", "--locked"]);
	run(["cargo", "test", "--workspace", "--all-targets", "--locked"]);
	run(["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"]);
} catch (error) {
	await Bun.write(cargoPath, originalCargo);
	if (originalLock === undefined) {
		if (await Bun.file(lockPath).exists()) await Bun.write(lockPath, "");
	} else {
		await Bun.write(lockPath, originalLock);
	}
	for (const update of derivedVersionUpdates) {
		await Bun.write(update.path, update.original);
	}
	throw error;
}

if (mode === "prepare") {
	console.log(`Prepared v${nextVersion}; no commit, tag, or push was performed.`);
	process.exit(0);
}

run(["git", "add", cargoPath, lockPath, ...derivedVersionUpdates.map((update) => update.path)]);
run(["git", "commit", "-m", `Release v${nextVersion}`]);
run(["git", "tag", `v${nextVersion}`]);
run(["git", "push"]);
run(["git", "push", "origin", `v${nextVersion}`]);
console.log(`Pushed v${nextVersion}; GitHub Actions must finish publishing before the release can be installed.`);
