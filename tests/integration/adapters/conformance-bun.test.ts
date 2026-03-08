// Bun 1.3.x CDP inspector does not fire Debugger.paused events — neither debugger;
// statements, Debugger.pause(), setPauseOnDebuggerStatements, nor setBreakpointByUrl
// cause the VM to actually pause. This is a known Bun limitation; skip conformance
// until Bun implements CDP pause support.
// TODO: re-enable when Bun's CDP supports Debugger.paused
import { describe } from "vitest";

describe.skip("Bun adapter conformance (Bun CDP does not support Debugger.paused)", () => {});
