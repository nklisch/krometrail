import { homedir } from "node:os";
import { join } from "node:path";

/** Base directory for all Krometrail data and caches. */
export function getKrometrailDir(): string {
	return join(homedir(), ".krometrail");
}

/** Subdirectory under the Krometrail base directory. */
export function getKrometrailSubdir(...segments: string[]): string {
	return join(getKrometrailDir(), ...segments);
}
