#!/usr/bin/env bun
import { defineCommand, runMain } from "citty";
import { browserCommand } from "./commands/browser.js";
import {
	attachCommand,
	breakCommand,
	breakpointsCommand,
	continueCommand,
	doctorCommand,
	evalCommand,
	launchCommand,
	logCommand,
	outputCommand,
	runToCommand,
	skillCommand,
	sourceCommand,
	stackCommand,
	statusCommand,
	stepCommand,
	stopCommand,
	threadsCommand,
	unwatchCommand,
	varsCommand,
	watchCommand,
} from "./commands/index.js";

const main = defineCommand({
	meta: {
		name: "agent-lens",
		version: "0.1.0",
		description: "Runtime debugging viewport for AI coding agents",
	},
	subCommands: {
		launch: launchCommand,
		attach: attachCommand,
		stop: stopCommand,
		status: statusCommand,
		continue: continueCommand,
		step: stepCommand,
		"run-to": runToCommand,
		break: breakCommand,
		breakpoints: breakpointsCommand,
		eval: evalCommand,
		vars: varsCommand,
		stack: stackCommand,
		source: sourceCommand,
		watch: watchCommand,
		unwatch: unwatchCommand,
		log: logCommand,
		output: outputCommand,
		threads: threadsCommand,
		doctor: doctorCommand,
		skill: skillCommand,
		browser: browserCommand,
		// Hidden: internal daemon entry point
		_daemon: () =>
			defineCommand({
				meta: { hidden: true },
				async run() {
					await import("../daemon/entry.js");
				},
			}),
	},
});

runMain(main);
