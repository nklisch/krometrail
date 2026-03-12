import type { VueObserverConfig } from "../vue-observer.js";

/** Threshold constants — exported for unit testing. */
export const VUE_PATTERN_DEFAULTS = {
	infiniteLoopThreshold: 30,
	infiniteLoopWindowMs: 2000,
} as const;

/**
 * Returns the JavaScript code string for all Vue pattern detection functions.
 * Injected into the observer IIFE. All functions use `var` only.
 *
 * Generated functions:
 * - checkPatterns(instance, record, componentName)
 * - checkInfiniteLoop(instance, record, componentName)
 * - checkLostReactivity(instance, record, componentName)
 * - checkPiniaMutationOutsideAction(storeId)
 */
export function getVuePatternCode(config: Required<VueObserverConfig>): string {
	const parts: string[] = [];

	// checkInfiniteLoop — detects >threshold updates in 2s window
	parts.push("function checkInfiniteLoop(instance, record, componentName) {");
	parts.push("  var _now = Date.now();");
	parts.push("  var _windowMs = 2000;");
	parts.push(`  var _threshold = ${config.infiniteLoopThreshold};`);
	parts.push("  var _recent = [];");
	parts.push("  for (var _ri = 0; _ri < record.updateTimestamps.length; _ri++) {");
	parts.push("    if (_now - record.updateTimestamps[_ri] < _windowMs) _recent.push(record.updateTimestamps[_ri]);");
	parts.push("  }");
	parts.push("  if (_recent.length > _threshold) {");
	parts.push("    queueEvent('error', {");
	parts.push("      framework: 'vue',");
	parts.push("      pattern: 'watcher_infinite_loop',");
	parts.push("      componentName: componentName,");
	parts.push("      severity: 'high',");
	parts.push("      detail: componentName + ' updated ' + _recent.length + ' times in ' + _windowMs + 'ms. Likely a watcher mutating its own dependency.',");
	parts.push("      evidence: {");
	parts.push("        updateCount: _recent.length,");
	parts.push("        windowMs: _windowMs,");
	parts.push("        lastKeys: record.lastState ? Object.keys(record.lastState) : []");
	parts.push("      }");
	parts.push("    });");
	parts.push("  }");
	parts.push("}");
	parts.push("");

	// checkLostReactivity — detects non-reactive plain objects in setupState
	parts.push("function checkLostReactivity(instance, record, componentName) {");
	parts.push("  try {");
	parts.push("    var setup = instance.setupState;");
	parts.push("    if (!setup) return;");
	parts.push("    var keys = Object.keys(setup);");
	parts.push("    for (var _lri = 0; _lri < keys.length; _lri++) {");
	parts.push("      var _lrk = keys[_lri];");
	parts.push("      if (_lrk[0] === '$' || _lrk[0] === '_') continue;");
	parts.push("      var val = setup[_lrk];");
	parts.push("      if (val === null || val === undefined) continue;");
	parts.push("      if (typeof val !== 'object') continue;");
	parts.push("      if (typeof val === 'function') continue;");
	parts.push("      if (Array.isArray(val)) continue;");
	parts.push("      var isReactive = val.__v_isReactive === true;");
	parts.push("      var isRef = val.__v_isRef === true;");
	parts.push("      var isReadonly = val.__v_isReadonly === true;");
	parts.push("      var isShallow = val.__v_isShallow === true;");
	parts.push("      if (!isReactive && !isRef && !isReadonly && !isShallow) {");
	parts.push("        queueEvent('error', {");
	parts.push("          framework: 'vue',");
	parts.push("          pattern: 'lost_reactivity',");
	parts.push("          componentName: componentName,");
	parts.push("          severity: 'medium',");
	parts.push("          detail: '\"' + _lrk + '\" in ' + componentName + ' setupState is a plain object (not reactive). This often happens when destructuring a reactive() object or unwrapping a ref without .value.',");
	parts.push("          evidence: {");
	parts.push("            key: _lrk,");
	parts.push("            actualType: typeof val,");
	parts.push("            hasProxy: false");
	parts.push("          }");
	parts.push("        });");
	parts.push("      }");
	parts.push("    }");
	parts.push("  } catch(e) {}");
	parts.push("}");
	parts.push("");

	// checkPiniaMutationOutsideAction — called inline from observePiniaStore
	parts.push("function checkPiniaMutationOutsideAction(storeId) {");
	parts.push("  queueEvent('error', {");
	parts.push("    framework: 'vue',");
	parts.push("    pattern: 'pinia_mutation_outside_action',");
	parts.push("    componentName: '[Store] ' + storeId,");
	parts.push("    severity: 'low',");
	parts.push("    detail: 'Pinia store \"' + storeId + '\" state was directly mutated outside of an action. This bypasses devtools tracking and time-travel debugging.',");
	parts.push("    evidence: {");
	parts.push("      storeId: storeId,");
	parts.push("      mutationType: 'direct'");
	parts.push("    }");
	parts.push("  });");
	parts.push("}");
	parts.push("");

	// checkPatterns — dispatcher
	parts.push("function checkPatterns(instance, record, componentName) {");
	parts.push("  try { checkInfiniteLoop(instance, record, componentName); } catch(e) {}");
	parts.push("  try { checkLostReactivity(instance, record, componentName); } catch(e) {}");
	parts.push("}");

	return parts.join("\n");
}
