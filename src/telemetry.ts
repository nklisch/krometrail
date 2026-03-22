/**
 * Minimal fire-and-forget usage ping.
 *
 * Collects no user data. Honors standard opt-out signals:
 *   DO_NOT_TRACK=1, KROMETRAIL_NO_TELEMETRY=1, TELEMETRY_DISABLED=1, CI=true
 *
 * Uses the GA4 Measurement Protocol so events show up in the same GA property
 * as the docs site. Requires KROMETRAIL_GA_SECRET to be set (bake in at build
 * time or set in the environment). If the secret is absent, the ping is skipped.
 */

const MEASUREMENT_ID = "G-8VK84SJ371";
const OPT_OUT_VARS = ["DO_NOT_TRACK", "KROMETRAIL_NO_TELEMETRY", "TELEMETRY_DISABLED"];

function isOptedOut(): boolean {
	for (const key of OPT_OUT_VARS) {
		const envValue = process.env[key];
		if (envValue && envValue !== "0" && envValue !== "false") return true;
	}
	// Skip in CI by default — installs there are not real users
	if (process.env.CI) return true;
	return false;
}

/** Send a single anonymous run-ping. Best-effort — never throws. */
export async function sendPing(event: "run" | "mcp_start"): Promise<void> {
	const secret = process.env.KROMETRAIL_GA_SECRET;
	if (!secret || isOptedOut()) return;

	// Random client_id per invocation — nothing is stored or persisted
	const clientId = Math.random().toString(36).slice(2) + Math.random().toString(36).slice(2);

	const body = {
		client_id: clientId,
		non_personalized_ads: true,
		events: [
			{
				name: event,
				params: {
					version: process.env.npm_package_version ?? "unknown",
					platform: process.platform,
					engagement_time_msec: 1,
				},
			},
		],
	};

	const url = `https://www.google-analytics.com/mp/collect?measurement_id=${MEASUREMENT_ID}&api_secret=${secret}`;

	try {
		await fetch(url, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify(body),
			signal: AbortSignal.timeout(3000),
		});
	} catch {
		// Ignore all errors — telemetry must never affect the tool
	}
}
