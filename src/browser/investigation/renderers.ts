import { estimateTokens, fitToBudget, type RenderSection, truncateToTokens } from "../../core/token-budget.js";
import type { EventRow } from "../storage/database.js";
import type { DiffResult } from "./diff.js";
import { formatTime } from "./format-helpers.js";
import type { InspectResult, SessionOverview, SessionSummary } from "./query-engine.js";

/**
 * Render a session list for the agent.
 */
export function renderSessionList(sessions: SessionSummary[]): string {
	if (sessions.length === 0) return "No recorded sessions found.";

	const lines: string[] = [`Sessions (${sessions.length}):\n`];
	for (const s of sessions) {
		const duration = formatDuration(s.duration);
		const markers = s.markerCount > 0 ? `, ${s.markerCount} markers` : "";
		const errors = s.errorCount > 0 ? `, ${s.errorCount} errors` : "";
		lines.push(`  ${s.id}  ${formatTime(s.startedAt)}  ${duration}  ${s.url}  (${s.eventCount} events${markers}${errors})`);
	}
	return lines.join("\n");
}

/**
 * Render a session overview with token budgeting.
 * Sections are prioritized: markers > errors > timeline > network summary.
 */
export function renderSessionOverview(overview: SessionOverview, tokenBudget = 3000): string {
	const sections: RenderSection[] = [];

	// Header (always included, highest priority)
	const header = [`Session: ${overview.session.id}`, `URL: ${overview.session.url}`, `Started: ${formatTime(overview.session.startedAt)}`, ""].join("\n");
	sections.push({ key: "header", content: header, priority: 100 });

	// Markers (high priority — this is what the agent is usually looking for)
	if (overview.markers.length > 0) {
		const markerLines = ["Markers:"];
		for (const m of overview.markers) {
			const prefix = m.auto_detected ? "[auto]" : "[user]";
			const sev = m.severity ? ` (${m.severity})` : "";
			markerLines.push(`  ${prefix} ${formatTime(m.timestamp)} — ${m.label ?? "unmarked"}${sev}  (id: ${m.id})`);
		}
		markerLines.push("");
		sections.push({ key: "markers", content: markerLines.join("\n"), priority: 90 });
	}

	// Errors (high priority)
	if (overview.errorSummary && overview.errorSummary.length > 0) {
		const errorLines = ["Errors:"];
		for (const e of overview.errorSummary.slice(0, 10)) {
			errorLines.push(`  ${formatTime(e.timestamp)}  [${e.type}] ${e.summary}`);
		}
		if (overview.errorSummary.length > 10) {
			errorLines.push(`  (${overview.errorSummary.length - 10} more...)`);
		}
		errorLines.push("");
		sections.push({ key: "errors", content: errorLines.join("\n"), priority: 80 });
	}

	// Framework summary (priority 75 — between errors and timeline)
	if (overview.frameworkSummary) {
		const fs = overview.frameworkSummary;
		const fwLines: string[] = ["Framework:"];
		for (const fw of fs.frameworks) {
			const store = fw.storeDetected ? ` + ${fw.storeDetected}` : "";
			fwLines.push(`  ${fw.name} ${fw.version} (${fw.componentCount} components${store})`);
		}
		fwLines.push(`  ${fs.stateEventCount} state events`);
		if (fs.errors.high > 0 || fs.errors.medium > 0 || fs.errors.low > 0) {
			const parts: string[] = [];
			if (fs.errors.high > 0) parts.push(`${fs.errors.high} high`);
			if (fs.errors.medium > 0) parts.push(`${fs.errors.medium} medium`);
			if (fs.errors.low > 0) parts.push(`${fs.errors.low} low`);
			fwLines.push(`  Bug patterns: ${parts.join(", ")}`);
		}
		if (fs.topComponents.length > 0) {
			fwLines.push("  Most active components:");
			for (const c of fs.topComponents.slice(0, 5)) {
				fwLines.push(`    ${c.name} (${c.updateCount} updates)`);
			}
		}
		fwLines.push("");
		sections.push({ key: "framework", content: fwLines.join("\n"), priority: 75 });
	}

	// Navigation timeline (medium priority)
	if (overview.timeline.length > 0) {
		const timelineLines = ["Timeline:"];
		for (const e of overview.timeline) {
			const marker = e.type === "marker" ? " ← MARKER" : "";
			timelineLines.push(`  ${formatTime(e.timestamp)}  ${e.summary}${marker}`);
		}
		timelineLines.push("");
		sections.push({ key: "timeline", content: timelineLines.join("\n"), priority: 60 });
	}

	// Network summary (lower priority)
	if (overview.networkSummary) {
		const ns = overview.networkSummary;
		const netLines = [`Network: ${ns.total} requests | ${ns.succeeded} succeeded | ${ns.failed} failed`];
		if (ns.notable.length > 0) {
			netLines.push("Notable:");
			for (const n of ns.notable.slice(0, 5)) {
				netLines.push(`  ${n}`);
			}
		}
		netLines.push("");
		sections.push({ key: "network", content: netLines.join("\n"), priority: 40 });
	}

	const included = fitToBudget(sections, tokenBudget);
	return included.map((s) => s.content).join("\n");
}

/**
 * Render search results with token budgeting.
 */
export function renderSearchResults(results: EventRow[], tokenBudget = 2000): string {
	if (results.length === 0) return "No matching events found.";

	const lines: string[] = [`Found ${results.length} events:\n`];
	let tokens = estimateTokens(lines[0]);

	for (const r of results) {
		const line = formatSearchResultLine(r);
		const lineTokens = estimateTokens(line);
		if (tokens + lineTokens > tokenBudget) {
			lines.push(`  ... (${results.length - lines.length + 1} more results)`);
			break;
		}
		lines.push(line);
		tokens += lineTokens;
	}

	return lines.join("\n");
}

function formatSearchResultLine(r: EventRow): string {
	const time = formatTime(r.timestamp);
	// Framework events already have [framework] prefix in summary — skip redundant [type]
	if (r.type.startsWith("framework_")) {
		return `  ${time}  ${r.summary}  (id: ${r.event_id})`;
	}
	return `  ${time}  [${r.type}] ${r.summary}  (id: ${r.event_id})`;
}

/**
 * Render a detailed event inspection with token budgeting.
 */
export function renderInspectResult(result: InspectResult, tokenBudget = 3000): string {
	const sections: RenderSection[] = [];

	// Event detail (highest priority)
	const event = result.event;
	const eventLines = [`Event: ${event.summary}`, `Type: ${event.type}`, `Time: ${formatTime(event.timestamp)}`, `ID: ${event.id}`];

	// Add type-specific detail
	if (event.type === "network_request" || event.type === "network_response") {
		const d = event.data as Record<string, unknown>;
		if (d.method) eventLines.push(`Method: ${d.method}`);
		if (d.url) eventLines.push(`URL: ${d.url}`);
		if (d.status) eventLines.push(`Status: ${d.status}`);
		if (d.durationMs) eventLines.push(`Duration: ${d.durationMs}ms`);
	}
	if (event.type === "console" || event.type === "page_error") {
		const d = event.data as Record<string, unknown>;
		if (d.stackTrace) eventLines.push(`Stack: ${d.stackTrace}`);
	}
	if (event.type === "framework_detect") {
		const d = event.data as Record<string, unknown>;
		eventLines.push(`Framework: ${d.framework} ${d.version}`);
		if (d.rootCount != null) eventLines.push(`Roots: ${d.rootCount}`);
		if (d.componentCount != null) eventLines.push(`Components: ${d.componentCount}`);
		if (d.storeDetected) eventLines.push(`Store: ${d.storeDetected}`);
		if (d.bundleType != null) eventLines.push(`Build: ${d.bundleType === 1 ? "development" : "production"}`);
	}
	if (event.type === "framework_state") {
		const d = event.data as Record<string, unknown>;
		if (d.componentPath) eventLines.push(`Path: ${d.componentPath}`);
		eventLines.push(`Change: ${d.changeType}`);
		if (d.triggerSource) eventLines.push(`Trigger: ${d.triggerSource}`);
		if (d.renderCount != null) eventLines.push(`Render #${d.renderCount}`);
		if (d.storeId) eventLines.push(`Store: ${d.storeId}`);
		if (d.actionName) eventLines.push(`Action: ${d.actionName}`);
		if (d.mutationType) eventLines.push(`Mutation type: ${d.mutationType}`);
		if (Array.isArray(d.changes) && d.changes.length > 0) {
			eventLines.push("Changes:");
			for (const change of d.changes as Array<{ key: string; prev: unknown; next: unknown }>) {
				eventLines.push(`  ${change.key}: ${formatValue(change.prev)} → ${formatValue(change.next)}`);
			}
		}
	}
	if (event.type === "framework_error") {
		const d = event.data as Record<string, unknown>;
		eventLines.push(`Pattern: ${d.pattern}`);
		eventLines.push(`Severity: ${d.severity}`);
		if (d.detail) eventLines.push(`Detail: ${d.detail}`);
		if (d.evidence && typeof d.evidence === "object") {
			eventLines.push("Evidence:");
			for (const [k, v] of Object.entries(d.evidence as Record<string, unknown>)) {
				eventLines.push(`  ${k}: ${formatValue(v)}`);
			}
		}
	}
	eventLines.push("");
	sections.push({ key: "event", content: eventLines.join("\n"), priority: 100 });

	// Network body (high priority — often the key evidence)
	if (result.networkBody) {
		const bodyLines: string[] = [];
		if (result.networkBody.request) {
			bodyLines.push("Request Body:");
			bodyLines.push(truncateToTokens(result.networkBody.request, 500));
			bodyLines.push("");
		}
		if (result.networkBody.response) {
			bodyLines.push(`Response Body (${result.networkBody.contentType ?? "unknown"}, ${result.networkBody.size ?? 0} bytes):`);
			bodyLines.push(truncateToTokens(result.networkBody.response, 800));
			bodyLines.push("");
		}
		if (bodyLines.length > 0) {
			sections.push({ key: "body", content: bodyLines.join("\n"), priority: 90 });
		}
	}

	// Surrounding events (medium priority — context)
	if (result.surroundingEvents.length > 0) {
		const ctxLines = [`Context (${result.surroundingEvents.length} events ±5s):`];
		for (const e of result.surroundingEvents) {
			const isCurrent = e.event_id === result.event.id;
			const prefix = isCurrent ? "→" : " ";
			ctxLines.push(`  ${prefix} ${formatTime(e.timestamp)}  [${e.type}] ${e.summary}`);
		}
		ctxLines.push("");
		sections.push({ key: "context", content: ctxLines.join("\n"), priority: 60 });
	}

	// Screenshot reference (low priority)
	if (result.screenshot) {
		sections.push({
			key: "screenshot",
			content: `Screenshot: ${result.screenshot}\n`,
			priority: 20,
		});
	}

	const included = fitToBudget(sections, tokenBudget);
	return included.map((s) => s.content).join("\n");
}

/**
 * Render a session diff result with token budgeting.
 */
export function renderDiff(diff: DiffResult, tokenBudget = 2000): string {
	const sections: RenderSection[] = [];

	// Header
	sections.push({
		key: "header",
		content: `Diff: ${formatTime(diff.beforeTime)} → ${formatTime(diff.afterTime)} (${formatDuration(diff.durationMs)})\n`,
		priority: 100,
	});

	// URL change
	if (diff.urlChange) {
		sections.push({
			key: "url",
			content: `URL: ${diff.urlChange.before} → ${diff.urlChange.after}\n`,
			priority: 90,
		});
	}

	// Form changes
	if (diff.formChanges && diff.formChanges.length > 0) {
		const lines = ["Form State Changes:"];
		for (const f of diff.formChanges) {
			lines.push(`  ${f.selector}  "${f.before}" → "${f.after}"`);
		}
		lines.push("");
		sections.push({ key: "form", content: lines.join("\n"), priority: 85 });
	}

	// Storage changes
	if (diff.storageChanges && diff.storageChanges.length > 0) {
		const lines = ["Storage Changes:"];
		for (const s of diff.storageChanges) {
			if (s.type === "added") lines.push(`  + ${s.key} = ${s.after}`);
			else if (s.type === "removed") lines.push(`  - ${s.key} (was: ${s.before})`);
			else lines.push(`  ~ ${s.key}: "${s.before}" → "${s.after}"`);
		}
		lines.push("");
		sections.push({ key: "storage", content: lines.join("\n"), priority: 70 });
	}

	// Console messages
	if (diff.newConsoleMessages && diff.newConsoleMessages.length > 0) {
		const lines = ["New Console Messages:"];
		for (const m of diff.newConsoleMessages) {
			lines.push(`  ${formatTime(m.timestamp)}  ${m.summary}`);
		}
		lines.push("");
		sections.push({ key: "console", content: lines.join("\n"), priority: 60 });
	}

	// Network activity
	if (diff.newNetworkRequests && diff.newNetworkRequests.length > 0) {
		const lines = [`Network Activity (${diff.newNetworkRequests.length} requests):`];
		for (const n of diff.newNetworkRequests) {
			lines.push(`  ${formatTime(n.timestamp)}  ${n.summary}`);
		}
		lines.push("");
		sections.push({ key: "network", content: lines.join("\n"), priority: 50 });
	}

	// Framework errors (high priority — bugs between the two moments)
	if (diff.frameworkErrors && diff.frameworkErrors.length > 0) {
		const lines = ["Framework Bug Patterns:"];
		for (const e of diff.frameworkErrors) {
			lines.push(`  [${e.severity}] ${e.pattern} in ${e.componentName}`);
			if (e.detail) lines.push(`    ${e.detail.slice(0, 120)}`);
		}
		lines.push("");
		sections.push({ key: "framework_errors", content: lines.join("\n"), priority: 65 });
	}

	// Framework component changes
	if (diff.frameworkChanges && diff.frameworkChanges.length > 0) {
		const lines = ["Component Changes:"];
		const mounted = diff.frameworkChanges.filter((c) => c.changeType === "mounted");
		const unmounted = diff.frameworkChanges.filter((c) => c.changeType === "unmounted");
		const updated = diff.frameworkChanges.filter((c) => c.changeType === "updated");

		if (mounted.length > 0) {
			lines.push(`  Mounted (${mounted.length}): ${mounted.map((c) => c.componentName).join(", ")}`);
		}
		if (unmounted.length > 0) {
			lines.push(`  Unmounted (${unmounted.length}): ${unmounted.map((c) => c.componentName).join(", ")}`);
		}
		for (const c of updated) {
			if (c.changes && c.changes.length > 0) {
				const changeSummary = c.changes
					.slice(0, 3)
					.map((ch) => `${ch.key}: ${formatValue(ch.prev)} → ${formatValue(ch.next)}`)
					.join(", ");
				const more = c.changes.length > 3 ? ` +${c.changes.length - 3} more` : "";
				lines.push(`  ~ ${c.componentName}: ${changeSummary}${more}`);
			} else {
				lines.push(`  ~ ${c.componentName}: updated`);
			}
		}
		lines.push("");
		sections.push({ key: "framework_components", content: lines.join("\n"), priority: 55 });
	}

	// Store mutations
	if (diff.storeMutations && diff.storeMutations.length > 0) {
		const lines = [`Store Mutations (${diff.storeMutations.length}):`];
		for (const m of diff.storeMutations.slice(0, 10)) {
			const action = m.actionName ? ` (action: ${m.actionName})` : "";
			lines.push(`  ${formatTime(m.timestamp)}  ${m.storeId}: ${m.mutationType}${action}`);
		}
		if (diff.storeMutations.length > 10) {
			lines.push(`  ... (${diff.storeMutations.length - 10} more)`);
		}
		lines.push("");
		sections.push({ key: "store_mutations", content: lines.join("\n"), priority: 45 });
	}

	const included = fitToBudget(sections, tokenBudget);
	return included.map((s) => s.content).join("\n");
}

// --- Helpers ---

function formatDuration(ms: number): string {
	const seconds = Math.floor(ms / 1000);
	if (seconds < 60) return `${seconds}s`;
	const minutes = Math.floor(seconds / 60);
	const remainingSeconds = seconds % 60;
	return `${minutes}m ${remainingSeconds}s`;
}

function formatValue(v: unknown): string {
	if (v === null || v === undefined) return String(v);
	if (typeof v === "string") return v.length > 80 ? `"${v.slice(0, 80)}..."` : `"${v}"`;
	if (typeof v === "object") {
		try {
			const s = JSON.stringify(v);
			return s.length > 100 ? `${s.slice(0, 100)}...` : s;
		} catch {
			return "[Object]";
		}
	}
	return String(v);
}
