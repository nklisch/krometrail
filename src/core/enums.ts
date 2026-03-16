import { z } from "zod";

// --- Debug Enums ---

export const STEP_DIRECTIONS = ["over", "into", "out"] as const;
export const StepDirectionSchema = z.enum(STEP_DIRECTIONS);
export type StepDirection = z.infer<typeof StepDirectionSchema>;

export const VARIABLE_SCOPES = ["local", "global", "closure", "all"] as const;
export const VariableScopeSchema = z.enum(VARIABLE_SCOPES);
export type VariableScope = z.infer<typeof VariableScopeSchema>;

export const OUTPUT_STREAMS = ["stdout", "stderr", "both"] as const;
export const OutputStreamSchema = z.enum(OUTPUT_STREAMS);
export type OutputStream = z.infer<typeof OutputStreamSchema>;

export const SESSION_LOG_FORMATS = ["summary", "detailed"] as const;
export const SessionLogFormatSchema = z.enum(SESSION_LOG_FORMATS);
export type SessionLogFormat = z.infer<typeof SessionLogFormatSchema>;

export const SESSION_STATUSES = ["running", "stopped", "terminated", "error"] as const;
export const SessionStatusSchema = z.enum(SESSION_STATUSES);
export type SessionStatus = z.infer<typeof SessionStatusSchema>;

export const SESSION_STATES = ["launching", ...SESSION_STATUSES] as const;
export const SessionStateSchema = z.enum(SESSION_STATES);
export type SessionState = z.infer<typeof SessionStateSchema>;

export const STOP_REASONS = ["breakpoint", "step", "exception", "pause", "entry"] as const;
export const StopReasonSchema = z.enum(STOP_REASONS);
export type StopReason = z.infer<typeof StopReasonSchema>;

// --- Browser Enums ---

export const FRAMEWORKS = ["react", "vue", "solid", "svelte"] as const;
export const FrameworkSchema = z.enum(FRAMEWORKS);
export type Framework = z.infer<typeof FrameworkSchema>;

export const SEVERITIES = ["low", "medium", "high"] as const;
export const SeveritySchema = z.enum(SEVERITIES);
export type Severity = z.infer<typeof SeveritySchema>;

export const EVENT_TYPES = [
	"navigation",
	"network_request",
	"network_response",
	"console",
	"page_error",
	"user_input",
	"dom_mutation",
	"form_state",
	"screenshot",
	"performance",
	"websocket",
	"storage_change",
	"marker",
	"framework_detect",
	"framework_state",
	"framework_error",
	"annotation",
] as const;
export const EventTypeSchema = z.enum(EVENT_TYPES);
export type EventType = z.infer<typeof EventTypeSchema>;

/** Subset of EventType used in search filters (excludes dom_mutation, form_state, storage_change). */
export const SEARCHABLE_EVENT_TYPES = [
	"navigation",
	"network_request",
	"network_response",
	"console",
	"page_error",
	"user_input",
	"websocket",
	"performance",
	"marker",
	"framework_detect",
	"framework_state",
	"framework_error",
	"annotation",
] as const;
export const SearchableEventTypeSchema = z.enum(SEARCHABLE_EVENT_TYPES);

export const FRAMEWORK_CHANGE_TYPES = ["mount", "update", "unmount", "store_mutation"] as const;
export const FrameworkChangeTypeSchema = z.enum(FRAMEWORK_CHANGE_TYPES);
export type FrameworkChangeType = z.infer<typeof FrameworkChangeTypeSchema>;

export const ACTION_OBSERVATION_KINDS = ["unexpected_value", "variable_changed", "new_frame", "exception", "bp_hit", "terminated"] as const;
export const ActionObservationKindSchema = z.enum(ACTION_OBSERVATION_KINDS);
export type ActionObservationKind = z.infer<typeof ActionObservationKindSchema>;

// --- Browser Investigation Enums ---

export const OVERVIEW_INCLUDES = ["timeline", "markers", "errors", "network_summary", "framework"] as const;
export const OverviewIncludeSchema = z.enum(OVERVIEW_INCLUDES);
export type OverviewInclude = z.infer<typeof OverviewIncludeSchema>;

export const INSPECT_INCLUDES = ["surrounding_events", "network_body", "screenshot", "form_state", "console_context"] as const;
export const InspectIncludeSchema = z.enum(INSPECT_INCLUDES);
export type InspectInclude = z.infer<typeof InspectIncludeSchema>;

export const DIFF_INCLUDES = ["form_state", "storage", "url", "console_new", "network_new", "framework_state"] as const;
export const DiffIncludeSchema = z.enum(DIFF_INCLUDES);
export type DiffInclude = z.infer<typeof DiffIncludeSchema>;

export const REPLAY_FORMATS = ["summary", "reproduction_steps", "test_scaffold"] as const;
export const ReplayFormatSchema = z.enum(REPLAY_FORMATS);
export type ReplayFormat = z.infer<typeof ReplayFormatSchema>;

export const TEST_FRAMEWORKS = ["playwright", "cypress"] as const;
export const TestFrameworkSchema = z.enum(TEST_FRAMEWORKS);
export type TestFramework = z.infer<typeof TestFrameworkSchema>;

export const EXPORT_FORMATS = ["har"] as const;
export const ExportFormatSchema = z.enum(EXPORT_FORMATS);
export type ExportFormat = z.infer<typeof ExportFormatSchema>;

// --- ViewportConfig shared field shape ---
// Used by daemon protocol (camelCase) and core types.
// MCP tools derive a snake_case version from these keys.

export const VIEWPORT_CONFIG_FIELDS = ["sourceContextLines", "stackDepth", "localsMaxDepth", "localsMaxItems", "stringTruncateLength", "collectionPreviewItems"] as const;

// Re-exported from types.ts — derived from ViewportConfigSchema.partial() (single source of truth).
export { ViewportConfigPartialSchema } from "./types.js";
