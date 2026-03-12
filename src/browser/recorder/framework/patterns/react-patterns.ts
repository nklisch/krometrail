import type { ReactObserverConfig } from "../react-observer.js";

/** Threshold constants — exported for unit testing. */
export const REACT_PATTERN_DEFAULTS = {
	infiniteRerenderThreshold: 15,
	infiniteRerenderWindowMs: 1000,
	staleClosureThreshold: 5,
	contextRerenderThreshold: 20,
} as const;

/**
 * Returns the JavaScript code string for all React pattern detection functions.
 * Injected into the observer IIFE. All functions use `var` only.
 *
 * Generated functions:
 * - checkPatterns(fiber, tracking, componentName)
 * - checkInfiniteRerender(fiber, tracking, componentName)
 * - checkStaleClosures(fiber, tracking, componentName)
 * - checkMissingCleanup(fiber, tracking, componentName)
 * - checkExcessiveContextRerender(fiber, tracking, componentName)
 * - updateDepsTracking(fiber, tracking)
 */
export function getReactPatternCode(config: Required<ReactObserverConfig>): string {
	const parts: string[] = [];

	// updateDepsTracking — snapshots hook deps for stale closure detection
	parts.push("function updateDepsTracking(fiber, tracking) {");
	parts.push("  if (fiber.tag !== 0 && fiber.tag !== 11 && fiber.tag !== 14 && fiber.tag !== 15) return;");
	parts.push("  var hookList = getHooksState(fiber);");
	parts.push("  for (var _di = 0; _di < hookList.length; _di++) {");
	parts.push("    var _ms = hookList[_di].hook.memoizedState;");
	parts.push("    if (_ms && typeof _ms === 'object' && 'deps' in _ms) {");
	parts.push("      tracking.prevDeps[_di] = _ms.deps;");
	parts.push("    } else if (Array.isArray(_ms) && _ms.length === 2 && (Array.isArray(_ms[1]) || _ms[1] === null)) {");
	parts.push("      tracking.prevDeps[_di] = _ms[1];");
	parts.push("    }");
	parts.push("  }");
	parts.push("}");
	parts.push("");

	// checkInfiniteRerender — detects >threshold renders in 1s window
	parts.push("function checkInfiniteRerender(fiber, tracking, componentName) {");
	parts.push("  var _now = Date.now();");
	parts.push(`  var _windowMs = 1000;`);
	parts.push(`  var _threshold = ${config.infiniteRerenderThreshold};`);
	parts.push("  var _recent = [];");
	parts.push("  for (var _ri = 0; _ri < tracking.renderTimestamps.length; _ri++) {");
	parts.push("    if (_now - tracking.renderTimestamps[_ri] < _windowMs) _recent.push(tracking.renderTimestamps[_ri]);");
	parts.push("  }");
	parts.push("  if (_recent.length > _threshold) {");
	parts.push("    queueEvent('error', {");
	parts.push("      framework: 'react',");
	parts.push("      pattern: 'infinite_rerender',");
	parts.push("      componentName: componentName,");
	parts.push("      severity: 'high',");
	parts.push("      detail: componentName + ' rendered ' + _recent.length + ' times in ' + _windowMs + 'ms. Likely setState in useEffect without proper deps.',");
	parts.push("      evidence: {");
	parts.push("        rendersInWindow: _recent.length,");
	parts.push("        windowMs: _windowMs,");
	parts.push("        lastState: serialize(fiber.memoizedState)");
	parts.push("      }");
	parts.push("    });");
	parts.push("  }");
	parts.push("}");
	parts.push("");

	// checkStaleClosures — detects hooks with unchanged deps while state changes
	parts.push("function checkStaleClosures(fiber, tracking, componentName) {");
	parts.push("  if (fiber.tag !== 0 && fiber.tag !== 11 && fiber.tag !== 14 && fiber.tag !== 15) return;");
	parts.push("  if (!fiber.alternate) return;");
	parts.push(`  var _staleThreshold = ${config.staleClosureThreshold};`);
	parts.push("  var _scHooks = getHooksState(fiber);");
	parts.push("  for (var _sci = 0; _sci < _scHooks.length; _sci++) {");
	parts.push("    var _scMs = _scHooks[_sci].hook.memoizedState;");
	parts.push("    // Check effect hooks with deps ({ create, destroy, deps, tag })");
	parts.push("    if (_scMs && typeof _scMs === 'object' && 'deps' in _scMs && _scMs.deps !== null) {");
	parts.push("      var _scPrevDeps = tracking.prevDeps[_sci];");
	parts.push("      if (_scPrevDeps !== undefined && shallowEqual(_scMs.deps, _scPrevDeps)) {");
	parts.push("        if (!tracking._staleCount) tracking._staleCount = {};");
	parts.push("        tracking._staleCount[_sci] = (tracking._staleCount[_sci] || 0) + 1;");
	parts.push("        if (tracking._staleCount[_sci] >= _staleThreshold) {");
	parts.push("          var _stateChanged = fiber.memoizedState !== fiber.alternate.memoizedState;");
	parts.push("          if (_stateChanged) {");
	parts.push("            queueEvent('error', {");
	parts.push("              framework: 'react',");
	parts.push("              pattern: 'stale_closure',");
	parts.push("              componentName: componentName,");
	parts.push("              severity: 'medium',");
	parts.push(
		"              detail: 'Hook at index ' + _sci + ' in ' + componentName + ' has unchanged deps for ' + tracking._staleCount[_sci] + ' renders while state changed. Possible stale closure.',",
	);
	parts.push("              evidence: {");
	parts.push("                hookIndex: _sci,");
	parts.push("                unchangedDeps: serialize(_scMs.deps),");
	parts.push("                rendersSinceLastDepsChange: tracking._staleCount[_sci],");
	parts.push("                renderCount: tracking.renderCount");
	parts.push("              }");
	parts.push("            });");
	parts.push("            tracking._staleCount[_sci] = 0;");
	parts.push("          }");
	parts.push("        }");
	parts.push("      } else {");
	parts.push("        if (tracking._staleCount) tracking._staleCount[_sci] = 0;");
	parts.push("      }");
	parts.push("    }");
	parts.push("    // Check [value, deps] tuples (useMemo/useCallback)");
	parts.push("    if (Array.isArray(_scMs) && _scMs.length === 2 && (Array.isArray(_scMs[1]) || _scMs[1] === null) && _scMs[1] !== null) {");
	parts.push("      var _memoPrevDeps = tracking.prevDeps[_sci];");
	parts.push("      if (_memoPrevDeps !== undefined && shallowEqual(_scMs[1], _memoPrevDeps)) {");
	parts.push("        if (!tracking._staleCount) tracking._staleCount = {};");
	parts.push("        tracking._staleCount[_sci] = (tracking._staleCount[_sci] || 0) + 1;");
	parts.push("        if (tracking._staleCount[_sci] >= _staleThreshold) {");
	parts.push("          var _stateChanged2 = fiber.alternate && fiber.memoizedState !== fiber.alternate.memoizedState;");
	parts.push("          if (_stateChanged2) {");
	parts.push("            queueEvent('error', {");
	parts.push("              framework: 'react',");
	parts.push("              pattern: 'stale_closure',");
	parts.push("              componentName: componentName,");
	parts.push("              severity: 'medium',");
	parts.push(
		"              detail: 'Memo/callback hook at index ' + _sci + ' in ' + componentName + ' has unchanged deps for ' + tracking._staleCount[_sci] + ' renders while state changed. Possible stale closure.',",
	);
	parts.push("              evidence: {");
	parts.push("                hookIndex: _sci,");
	parts.push("                unchangedDeps: serialize(_scMs[1]),");
	parts.push("                rendersSinceLastDepsChange: tracking._staleCount[_sci],");
	parts.push("                renderCount: tracking.renderCount");
	parts.push("              }");
	parts.push("            });");
	parts.push("            tracking._staleCount[_sci] = 0;");
	parts.push("          }");
	parts.push("        }");
	parts.push("      } else {");
	parts.push("        if (tracking._staleCount) tracking._staleCount[_sci] = 0;");
	parts.push("      }");
	parts.push("    }");
	parts.push("  }");
	parts.push("}");
	parts.push("");

	// checkMissingCleanup — detects passive effects with no destroy function
	parts.push("function checkMissingCleanup(fiber, tracking, componentName) {");
	parts.push("  if (fiber.tag !== 0 && fiber.tag !== 11) return;");
	parts.push("  var _mcHooks = getHooksState(fiber);");
	parts.push("  for (var _mci = 0; _mci < _mcHooks.length; _mci++) {");
	parts.push("    var _mcMs = _mcHooks[_mci].hook.memoizedState;");
	parts.push("    if (_mcMs && typeof _mcMs === 'object' && 'create' in _mcMs && 'tag' in _mcMs) {");
	parts.push("      var _isPassive = (_mcMs.tag & 8) !== 0;");
	parts.push("      var _hasEffect = (_mcMs.tag & 1) !== 0;");
	parts.push("      if (_isPassive && _mcMs.destroy === undefined && tracking.renderCount > 1 && _hasEffect) {");
	parts.push("        queueEvent('error', {");
	parts.push("          framework: 'react',");
	parts.push("          pattern: 'missing_cleanup',");
	parts.push("          componentName: componentName,");
	parts.push("          severity: 'low',");
	parts.push(
		"          detail: 'useEffect at index ' + _mci + ' in ' + componentName + ' has no cleanup function but re-runs on re-render. If it sets up subscriptions or timers, this may cause leaks.',",
	);
	parts.push("          evidence: {");
	parts.push("            hookIndex: _mci,");
	parts.push("            effectTag: _mcMs.tag,");
	parts.push("            hasDestroyFn: false,");
	parts.push("            renderCount: tracking.renderCount");
	parts.push("          }");
	parts.push("        });");
	parts.push("      }");
	parts.push("    }");
	parts.push("  }");
	parts.push("}");
	parts.push("");

	// checkExcessiveContextRerender — detects ContextProvider (tag 10) with too many consumers
	parts.push("function checkExcessiveContextRerender(fiber, tracking, componentName) {");
	parts.push("  if (fiber.tag !== 10) return;");
	parts.push("  if (!fiber.alternate) return;");
	parts.push("  if (!fiber.memoizedProps || fiber.memoizedProps.value === fiber.alternate.memoizedProps.value) return;");
	parts.push(`  var _ctxThreshold = ${config.contextRerenderThreshold};`);
	parts.push("  var _consumerCount = 0;");
	parts.push("  var _consumerNames = [];");
	parts.push("  var _ctxStack = [];");
	parts.push("  if (fiber.child) _ctxStack.push(fiber.child);");
	parts.push("  while (_ctxStack.length > 0 && _consumerCount <= _ctxThreshold + 5) {");
	parts.push("    var _ctxFiber = _ctxStack.pop();");
	parts.push("    if (!_ctxFiber) continue;");
	parts.push("    var _ctxDeps = _ctxFiber.dependencies || _ctxFiber.contextDependencies;");
	parts.push("    if (_ctxDeps && _ctxDeps.firstContext) {");
	parts.push("      var _ctxNode = _ctxDeps.firstContext;");
	parts.push("      while (_ctxNode) {");
	parts.push("        try {");
	parts.push("          if (_ctxNode.context && fiber.type && _ctxNode.context === fiber.type._context) {");
	parts.push("            _consumerCount++;");
	parts.push("            if (_consumerNames.length < 10) _consumerNames.push(getComponentName(_ctxFiber));");
	parts.push("            break;");
	parts.push("          }");
	parts.push("        } catch(e) {}");
	parts.push("        _ctxNode = _ctxNode.next;");
	parts.push("      }");
	parts.push("    }");
	parts.push("    if (_ctxFiber.sibling) _ctxStack.push(_ctxFiber.sibling);");
	parts.push("    if (_ctxFiber.child) _ctxStack.push(_ctxFiber.child);");
	parts.push("  }");
	parts.push("  if (_consumerCount > _ctxThreshold) {");
	parts.push("    queueEvent('error', {");
	parts.push("      framework: 'react',");
	parts.push("      pattern: 'excessive_context_rerender',");
	parts.push("      componentName: componentName,");
	parts.push("      severity: 'medium',");
	parts.push("      detail: 'Context provider ' + componentName + ' value changed, causing ' + _consumerCount + '+ consumers to re-render. Consider memoizing the value or splitting the context.',");
	parts.push("      evidence: {");
	parts.push("        contextDisplayName: componentName,");
	parts.push("        affectedConsumerCount: _consumerCount,");
	parts.push("        consumerNames: _consumerNames");
	parts.push("      }");
	parts.push("    });");
	parts.push("  }");
	parts.push("}");
	parts.push("");

	// checkPatterns — dispatcher
	parts.push("function checkPatterns(fiber, tracking, componentName) {");
	parts.push("  try { checkInfiniteRerender(fiber, tracking, componentName); } catch(e) {}");
	parts.push("  try { checkStaleClosures(fiber, tracking, componentName); } catch(e) {}");
	parts.push("  try { checkMissingCleanup(fiber, tracking, componentName); } catch(e) {}");
	parts.push("  try { checkExcessiveContextRerender(fiber, tracking, componentName); } catch(e) {}");
	parts.push("}");

	return parts.join("\n");
}
