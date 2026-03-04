#!/usr/bin/env bun
import { defineCommand, runMain } from "citty";
import { launchCommand } from "./commands/index.js";

const main = defineCommand({
	meta: {
		name: "agent-lens",
		version: "0.1.0",
		description: "Runtime debugging viewport for AI coding agents",
	},
	subCommands: {
		launch: launchCommand,
		// TODO: stop, status, continue, step, run-to,
		// break, breakpoints, eval, vars, stack, source,
		// watch, log, output, doctor
	},
});

runMain(main);
