import { z } from "zod";

// --- Run Mode ---

/**
 * How bugscope is exposed to the agent under test.
 * - "mcp"      — bugscope MCP server configured; agent uses debug_* MCP tools
 * - "cli"      — no MCP server, but bugscope CLI + skill file available; agent uses bash
 * - "baseline" — no bugscope at all; agent relies on code reading and test output
 */
export type RunMode = "mcp" | "cli" | "baseline";

// --- Scenario Config (scenario.json) ---

export const ScenarioConfigSchema = z.object({
	scenario: z.object({
		name: z.string(),
		language: z.string(),
		description: z.string(),
		timeout_seconds: z.number(),
	}),
	setup: z
		.object({
			commands: z.array(z.string()).default([]),
		})
		.default({ commands: [] }),
	visible_test: z.object({
		command: z.string(),
	}),
	validation: z.object({
		command: z.string(),
	}),
});

export type ScenarioConfig = z.infer<typeof ScenarioConfigSchema>;

// --- Parsed Scenario (runtime type with resolved paths) ---

export interface Scenario {
	/** Scenario name from config */
	name: string;
	/** Human description */
	description: string;
	/** Language ("python", "node", "typescript", "go", "rust", "cpp", "java") */
	language: string;
	/** Timeout in seconds for the agent run */
	timeoutSeconds: number;
	/** Setup commands to run before the agent starts */
	setupCommands: string[];
	/** Command to run to check the visible test (pre/post agent) */
	visibleTestCommand: string;
	/** Command to run the hidden oracle test */
	validationCommand: string;
	/** Absolute path to the scenario directory */
	scenarioDir: string;
	/** Absolute path to src/ files to copy into workspace */
	srcDir: string;
	/** Absolute path to hidden/ files to copy in after agent runs */
	hiddenDir: string;
	/** Absolute path to prompt.md */
	promptPath: string;
}

// --- Workspace (temp directory prepared for one run) ---

export interface Workspace {
	/** Absolute path to temp workspace directory */
	workDir: string;
	/** Absolute path to the generated MCP config JSON */
	mcpConfigPath: string;
}

// --- Agent Run Result ---

export interface AgentRunOptions {
	workDir: string;
	mcpConfigPath: string;
	prompt: string;
	timeoutMs: number;
	env?: Record<string, string>;
	/** Run mode — controls whether MCP debugging tools are passed to the agent */
	mode: RunMode;
	/** Resume an existing session by ID instead of starting a new conversation */
	resumeSessionId?: string;
}

export interface AgentRunResult {
	exitCode: number | null;
	stdout: string;
	stderr: string;
	timedOut: boolean;
	durationMs: number;
	/** Human-readable session log lines (optional, driver-provided) */
	sessionLog?: string[];
	/** Structured tool call/result timeline (optional, driver-provided) */
	toolTimeline?: ToolEvent[];
	/** Session ID for resuming this conversation (driver-provided if available) */
	sessionId?: string;
}

/** A single tool call + result pair in the agent's investigation. */
export interface ToolEvent {
	/** Tool name */
	tool: string;
	/** Input passed to the tool */
	input: unknown;
	/** Tool result content (text output the agent saw) */
	output: string | null;
	/** Tool use ID for correlating call/result */
	toolUseId: string;
}

// --- Metrics extracted from agent output ---

export interface TokenUsage {
	/** Direct (non-cached) input tokens */
	input: number;
	/** Tokens read from cache */
	cacheRead: number;
	/** Tokens written to cache */
	cacheWrite: number;
	/** Output tokens */
	output: number;
	/** Total input tokens (input + cacheRead + cacheWrite) */
	total: number;
}

export interface AgentMetrics {
	/** Number of agent turns / steps */
	numTurns: number | null;
	/** Token usage breakdown */
	tokens: TokenUsage | null;
	/** Model used */
	model: string | null;
	/** Agent binary version */
	agentVersion: string | null;
	/** Tool call counts per tool name */
	toolCalls: Record<string, number>;
}

// --- Validation Result ---

export interface ValidationResult {
	passed: boolean;
	stdout: string;
	stderr: string;
}

// --- Full Run Result ---

export interface RunResult {
	/** Scenario name */
	scenario: string;
	/** Run mode — "tools" had bugscope MCP available, "baseline" did not */
	mode: RunMode;
	/** Scenario metadata for self-contained reports */
	scenarioMeta: {
		description: string;
		language: string;
	};
	/** Agent name */
	agent: string;
	/** ISO timestamp */
	timestamp: string;
	/** Whether the hidden oracle test passed */
	passed: boolean;
	/** Agent run duration in ms */
	durationMs: number;
	/** Whether the agent was killed by timeout */
	timedOut: boolean;
	/** Agent process exit code */
	agentExitCode: number | null;
	/** Extracted metrics */
	metrics: AgentMetrics;
	/** Bugscope version used */
	bugscopeVersion: string;
	/** Visible test pass/fail before agent ran */
	visibleTestBefore: boolean;
	/** Visible test pass/fail after agent ran */
	visibleTestAfter: boolean;
	/** Hidden oracle test result */
	validation: ValidationResult;
	/** Files the agent modified */
	filesChanged: string[];
	/** Git diff of agent's changes */
	diff: string;
	/** Human-readable session log */
	sessionLog: string[];
	/** Structured tool call timeline with inputs and outputs */
	toolTimeline: ToolEvent[];
	/** Agent's final summary (from result event, if available) */
	resultSummary: string | null;
	/** Number of retry attempts after the initial run (0 if passed first time or no retries) */
	retries: number;
}

// --- Agent Driver Interface ---

export interface AgentDriver {
	/** Human-readable name, e.g. "claude-code" */
	name: string;
	/** Check if the agent binary is available on PATH */
	available(): Promise<boolean>;
	/** Get the agent binary version string */
	version(): Promise<string>;
	/** Run the agent with the given options */
	run(options: AgentRunOptions): Promise<AgentRunResult>;
	/** Extract metrics from raw agent output */
	parseMetrics(result: AgentRunResult): AgentMetrics;
}
