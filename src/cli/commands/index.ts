import { defineCommand } from "citty";

export const launchCommand = defineCommand({
	meta: {
		name: "launch",
		description: "Launch a debug session",
	},
	args: {
		command: {
			type: "positional",
			description: "Command to debug, e.g. 'python app.py'",
			required: true,
		},
		break: {
			type: "string",
			description: "Set a breakpoint, e.g. 'order.py:147' or 'order.py:147 when discount < 0'",
			alias: "b",
		},
		language: {
			type: "string",
			description: "Override language detection",
		},
		"stop-on-entry": {
			type: "boolean",
			description: "Pause on first executable line",
			default: false,
		},
		json: {
			type: "boolean",
			description: "Output as JSON instead of viewport text",
			default: false,
		},
		quiet: {
			type: "boolean",
			description: "Viewport only, no banners or hints",
			default: false,
		},
	},
	run({ args }) {
		// TODO: connect to daemon, create session, print viewport
		console.log("launch:", args);
	},
});
