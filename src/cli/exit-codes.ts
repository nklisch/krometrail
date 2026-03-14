import { AdapterNotFoundError, BrowserRecorderStateError, DAPTimeoutError, KrometrailError, SessionLimitError, SessionNotFoundError, SessionStateError, TabNotFoundError } from "../core/errors.js";
import { RPC_SESSION_NOT_FOUND, RPC_SESSION_STATE_ERROR } from "../daemon/protocol.js";

/** Process exited normally. */
export const EXIT_SUCCESS = 0;

/** Generic runtime error (catch-all). */
export const EXIT_ERROR = 1;

/** Invalid usage (bad args, missing required flags, etc.). */
export const EXIT_USAGE = 2;

/** Requested resource not found (session, adapter, tab). */
export const EXIT_NOT_FOUND = 3;

/** Operation timed out (DAP, CDP). */
export const EXIT_TIMEOUT = 4;

/** Resource is in wrong state for the requested operation. */
export const EXIT_STATE = 5;

/**
 * Classify an error into the appropriate exit code.
 *
 * Uses instanceof checks on the KrometrailError hierarchy first.
 * Falls back to inspecting numeric RPC error codes for errors that
 * arrive from the daemon via JSON-RPC (where the daemon client wraps
 * them as generic KrometrailError with string numeric codes).
 */
export function exitCodeFromError(err: unknown): number {
	// instanceof hierarchy — most specific first
	if (err instanceof SessionNotFoundError || err instanceof AdapterNotFoundError || err instanceof TabNotFoundError) {
		return EXIT_NOT_FOUND;
	}
	if (err instanceof DAPTimeoutError) {
		return EXIT_TIMEOUT;
	}
	if (err instanceof SessionStateError || err instanceof SessionLimitError || err instanceof BrowserRecorderStateError) {
		return EXIT_STATE;
	}

	// Generic KrometrailError — may be a daemon RPC error wrapped with a string numeric code
	if (err instanceof KrometrailError) {
		const numericCode = Number(err.code);
		if (!Number.isNaN(numericCode)) {
			if (numericCode === RPC_SESSION_NOT_FOUND) return EXIT_NOT_FOUND;
			if (numericCode === RPC_SESSION_STATE_ERROR) return EXIT_STATE;
		}
		return EXIT_ERROR;
	}

	// Plain Error or unknown — check for RPC error shape: { code: number, message: string }
	if (typeof err === "object" && err !== null && "code" in err && "message" in err) {
		const code = (err as { code: unknown }).code;
		if (typeof code === "number") {
			if (code === RPC_SESSION_NOT_FOUND) return EXIT_NOT_FOUND;
			if (code === RPC_SESSION_STATE_ERROR) return EXIT_STATE;
		}
	}

	return EXIT_ERROR;
}
