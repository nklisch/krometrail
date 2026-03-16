import { basename } from "node:path";
import type { RunStepsResult, StepResult } from "./types.js";

export function renderStepResults(result: RunStepsResult): string {
	const { totalSteps, completedSteps, results, sessionId, totalDurationMs } = result;
	const totalSecs = (totalDurationMs / 1000).toFixed(1);
	const hasError = results.some((r) => r.status === "error");

	const lines: string[] = [];

	if (hasError) {
		const errorStep = results.find((r) => r.status === "error");
		lines.push(`Step Results (${completedSteps}/${totalSteps} completed, ${totalSecs}s total — STOPPED on step ${errorStep?.index ?? "?"}):`);
	} else {
		lines.push(`Step Results (${completedSteps}/${totalSteps} completed, ${totalSecs}s total):`);
	}

	lines.push("");

	for (const step of results) {
		lines.push(formatStepLine(step));
		if (step.status === "error" && step.error) {
			lines.push(`   Error: ${step.error}`);
		}
	}

	lines.push("");
	if (sessionId) {
		lines.push(`Session: ${sessionId} (use session_overview to investigate)`);
	}

	return lines.join("\n");
}

function formatStepLine(step: StepResult): string {
	const statusIcon = step.status === "ok" ? "✓" : "✗";
	const durationStr = `${step.durationMs}ms`.padStart(7);
	const indexStr = String(step.index).padStart(2);
	const labelStr = step.label.padEnd(30);

	let line = ` ${indexStr}. ${labelStr} ${statusIcon} ${durationStr}`;

	if (step.returnValue !== undefined) {
		line += `  → ${JSON.stringify(step.returnValue)}`;
	}

	if (step.screenshotPath) {
		line += `  📸 ${basename(step.screenshotPath)}`;
	}

	return line;
}
