import { buildInjectionPreamble } from "./injection-helpers.js";
import { getReactPatternCode } from "./patterns/react-patterns.js";
import type { ReactObserverConfig } from "./react-observer.js";

/**
 * Generate the React observer injection script.
 * Returns a self-contained IIFE that patches __REACT_DEVTOOLS_GLOBAL_HOOK__
 * to observe fiber commits and report via __BL__.
 *
 * Uses only `var` declarations — no let/const — for maximum browser compatibility.
 * All state is closure-local. Only side effect is patching the global hook.
 */
export function buildReactInjectionScript(config: Required<ReactObserverConfig>): string {
	const parts: string[] = [];

	parts.push("(function() {");
	parts.push("  'use strict';");
	parts.push("");

	// ===== SECTION 1: CONFIGURATION CONSTANTS =====
	parts.push("  // ===== CONFIGURATION =====");
	parts.push(`  var MAX_EVENTS_PER_SECOND = ${config.maxEventsPerSecond};`);
	parts.push(`  var MAX_DEPTH = ${config.maxSerializationDepth};`);
	parts.push(`  var MAX_FIBERS_PER_COMMIT = ${config.maxFibersPerCommit};`);
	parts.push(`  var MAX_QUEUE_SIZE = ${config.maxQueueSize};`);
	parts.push(`  var INFINITE_RERENDER_WINDOW_MS = 1000;`);
	parts.push("");

	// ===== SECTION 2: TRACKING STATE =====
	parts.push("  // ===== STATE =====");
	parts.push("  var componentTracking = new WeakMap();");
	parts.push("  var eventQueue = [];");
	parts.push("  var lastFlushTime = 0;");
	parts.push("  var rafScheduled = false;");
	parts.push("");

	// ===== SECTION 3: REPORTING HELPERS =====
	parts.push("  // ===== REPORTING =====");
	parts.push(...buildInjectionPreamble("react", "commit rate"));

	// ===== SECTION 5: FIBER UTILITIES =====
	parts.push("  // ===== FIBER UTILITIES =====");
	parts.push("  function getComponentName(fiber) {");
	parts.push("    var type = fiber.type;");
	parts.push("    if (typeof type === 'string') return type;");
	parts.push("    if (typeof type === 'function') return type.displayName || type.name || 'Anonymous';");
	parts.push("    if (type && typeof type === 'object') {");
	parts.push("      if (type.displayName) return type.displayName;");
	parts.push("      if (type.render) return 'ForwardRef(' + (type.render.displayName || type.render.name || '') + ')';");
	parts.push("      if (type.type) return 'Memo(' + (type.type.displayName || type.type.name || '') + ')';");
	parts.push("    }");
	parts.push("    return 'Unknown';");
	parts.push("  }");
	parts.push("");

	parts.push("  function getComponentPath(fiber) {");
	parts.push("    var pathParts = [];");
	parts.push("    var current = fiber;");
	parts.push("    while (current) {");
	parts.push("      var tag = current.tag;");
	parts.push("      if (tag === 0 || tag === 1 || tag === 11 || tag === 14 || tag === 15) {");
	parts.push("        var name = getComponentName(current);");
	parts.push("        if (name !== 'Anonymous' && name !== 'Unknown') pathParts.unshift(name);");
	parts.push("      }");
	parts.push("      current = current.return;");
	parts.push("      if (pathParts.length > 10) break;");
	parts.push("    }");
	parts.push("    return pathParts.join(' > ');");
	parts.push("  }");
	parts.push("");

	parts.push("  function isUserComponent(fiber) {");
	parts.push("    var tag = fiber.tag;");
	parts.push("    return tag === 0 || tag === 1 || tag === 11 || tag === 14 || tag === 15;");
	parts.push("  }");
	parts.push("");

	parts.push("  function getFlags(fiber) {");
	parts.push("    return fiber.flags !== undefined ? fiber.flags : (fiber.effectTag || 0);");
	parts.push("  }");
	parts.push("");

	parts.push("  function shallowEqual(a, b) {");
	parts.push("    if (a === b) return true;");
	parts.push("    if (!Array.isArray(a) || !Array.isArray(b)) return false;");
	parts.push("    if (a.length !== b.length) return false;");
	parts.push("    for (var sei = 0; sei < a.length; sei++) {");
	parts.push("      if (a[sei] !== b[sei]) return false;");
	parts.push("    }");
	parts.push("    return true;");
	parts.push("  }");
	parts.push("");

	// ===== SECTION 6: HOOK INSPECTION =====
	parts.push("  // ===== HOOK INSPECTION =====");
	parts.push("  function getHooksState(fiber) {");
	parts.push("    var hooks = [];");
	parts.push("    var h = fiber.memoizedState;");
	parts.push("    var idx = 0;");
	parts.push("    while (h !== null && h !== undefined) {");
	parts.push("      hooks.push({ index: idx, hook: h });");
	parts.push("      h = h.next;");
	parts.push("      idx++;");
	parts.push("      if (idx > 500) break;");
	parts.push("    }");
	parts.push("    return hooks;");
	parts.push("  }");
	parts.push("");

	parts.push("  function classifyHook(hook, index) {");
	parts.push("    var ms = hook.memoizedState;");
	parts.push("    // useRef: { current: ... } with no queue and no create field");
	parts.push("    if (ms !== null && ms !== undefined && typeof ms === 'object' && 'current' in ms && !hook.queue && !('create' in ms)) {");
	parts.push("      return { index: index, type: 'ref', value: ms.current };");
	parts.push("    }");
	parts.push("    // useEffect / useLayoutEffect: { create, destroy, deps, tag }");
	parts.push("    if (ms !== null && ms !== undefined && typeof ms === 'object' && 'create' in ms && 'destroy' in ms && 'tag' in ms) {");
	parts.push("      var isLayout = (ms.tag & 4) !== 0;");
	parts.push("      return { index: index, type: isLayout ? 'layoutEffect' : 'effect', value: ms.destroy !== undefined ? '[has cleanup]' : '[no cleanup]', deps: ms.deps };");
	parts.push("    }");
	parts.push("    // useMemo / useCallback: [value, deps] tuple");
	parts.push("    if (Array.isArray(ms) && ms.length === 2 && (Array.isArray(ms[1]) || ms[1] === null)) {");
	parts.push("      return { index: index, type: 'memo', value: ms[0], deps: ms[1] };");
	parts.push("    }");
	parts.push("    // useId: string starting with ':' with no queue");
	parts.push("    if (typeof ms === 'string' && ms.length > 0 && ms[0] === ':' && !hook.queue) {");
	parts.push("      return { index: index, type: 'id', value: ms };");
	parts.push("    }");
	parts.push("    // useTransition: boolean with queue");
	parts.push("    if (typeof ms === 'boolean' && hook.queue) {");
	parts.push("      return { index: index, type: 'transition', value: ms };");
	parts.push("    }");
	parts.push("    // useState / useReducer: queue with dispatch");
	parts.push("    if (hook.queue && typeof hook.queue.dispatch === 'function') {");
	parts.push("      return { index: index, type: 'state', value: ms };");
	parts.push("    }");
	parts.push("    return { index: index, type: 'unknown', value: ms };");
	parts.push("  }");
	parts.push("");

	// ===== SECTION 7: STATE CHANGE DIFFING =====
	parts.push("  // ===== STATE CHANGE DIFFING =====");
	parts.push("  function computeChanges(fiber, tracking) {");
	parts.push("    var changes = [];");
	parts.push("    // Props diff");
	parts.push("    var prevProps = fiber.alternate ? fiber.alternate.memoizedProps : tracking.prevProps;");
	parts.push("    var nextProps = fiber.memoizedProps;");
	parts.push("    if (prevProps !== nextProps && prevProps && nextProps) {");
	parts.push("      var pkeys = Object.keys(nextProps);");
	parts.push("      for (var pi = 0; pi < pkeys.length; pi++) {");
	parts.push("        var pkey = pkeys[pi];");
	parts.push("        if (pkey === 'children') continue;");
	parts.push("        try {");
	parts.push("          if (prevProps[pkey] !== nextProps[pkey]) {");
	parts.push("            changes.push({ key: 'props.' + pkey, prev: serialize(prevProps[pkey]), next: serialize(nextProps[pkey]) });");
	parts.push("          }");
	parts.push("        } catch(e) {}");
	parts.push("      }");
	parts.push("    }");
	parts.push("    // State diff");
	parts.push("    if (fiber.tag === 0 || fiber.tag === 11 || fiber.tag === 14 || fiber.tag === 15) {");
	parts.push("      // Function component: walk hooks");
	parts.push("      var currHooks = getHooksState(fiber);");
	parts.push("      var prevHooks = fiber.alternate ? getHooksState(fiber.alternate) : [];");
	parts.push("      for (var hi = 0; hi < currHooks.length; hi++) {");
	parts.push("        var currHook = currHooks[hi].hook;");
	parts.push("        var prevHook = prevHooks[hi] ? prevHooks[hi].hook : null;");
	parts.push("        if (currHook.queue && currHook.queue.dispatch) {");
	parts.push("          if (!prevHook || currHook.memoizedState !== prevHook.memoizedState) {");
	parts.push("            changes.push({");
	parts.push("              key: 'state[' + hi + ']',");
	parts.push("              prev: prevHook ? serialize(prevHook.memoizedState) : undefined,");
	parts.push("              next: serialize(currHook.memoizedState)");
	parts.push("            });");
	parts.push("          }");
	parts.push("        }");
	parts.push("      }");
	parts.push("    } else if (fiber.tag === 1) {");
	parts.push("      // Class component: diff memoizedState object");
	parts.push("      var ps = fiber.alternate ? fiber.alternate.memoizedState : tracking.prevState;");
	parts.push("      var ns = fiber.memoizedState;");
	parts.push("      if (ps && ns && typeof ps === 'object' && typeof ns === 'object') {");
	parts.push("        var stateKeys = Object.keys(ns);");
	parts.push("        for (var si = 0; si < stateKeys.length; si++) {");
	parts.push("          try {");
	parts.push("            if (ps[stateKeys[si]] !== ns[stateKeys[si]]) {");
	parts.push("              changes.push({ key: 'state.' + stateKeys[si], prev: serialize(ps[stateKeys[si]]), next: serialize(ns[stateKeys[si]]) });");
	parts.push("            }");
	parts.push("          } catch(e) {}");
	parts.push("        }");
	parts.push("      }");
	parts.push("    }");
	parts.push("    return changes.length > 0 ? changes : undefined;");
	parts.push("  }");
	parts.push("");

	parts.push("  function detectTriggerSource(fiber) {");
	parts.push("    if (!fiber.alternate) return 'mount';");
	parts.push("    var propsChanged = fiber.memoizedProps !== fiber.alternate.memoizedProps;");
	parts.push("    var stateChanged = fiber.memoizedState !== fiber.alternate.memoizedState;");
	parts.push("    // Check context dependencies");
	parts.push("    var contextChanged = false;");
	parts.push("    try {");
	parts.push("      var deps = fiber.dependencies || fiber.contextDependencies;");
	parts.push("      if (deps && deps.firstContext) {");
	parts.push("        var ctx = deps.firstContext;");
	parts.push("        while (ctx) {");
	parts.push("          if (ctx.context && ctx.context._currentValue !== undefined) {");
	parts.push("            contextChanged = true;");
	parts.push("            break;");
	parts.push("          }");
	parts.push("          ctx = ctx.next;");
	parts.push("        }");
	parts.push("      }");
	parts.push("    } catch(e) {}");
	parts.push("    if (contextChanged) return 'context';");
	parts.push("    if (stateChanged && !propsChanged) return 'state';");
	parts.push("    if (propsChanged && !stateChanged) return 'props';");
	parts.push("    if (propsChanged && stateChanged) return 'state';");
	parts.push("    return 'parent';");
	parts.push("  }");
	parts.push("");

	// ===== SECTION 8: PATTERN DETECTION =====
	parts.push("  // ===== PATTERN DETECTION =====");
	const patternCode = getReactPatternCode(config);
	// Indent pattern code by 2 spaces to match IIFE body
	for (const line of patternCode.split("\n")) {
		parts.push(`  ${line}`);
	}
	parts.push("");

	// ===== SECTION 9: COMMIT PROCESSING =====
	parts.push("  // ===== COMMIT PROCESSING =====");
	parts.push("  function processCommit(rendererId, fiberRoot) {");
	parts.push("    try {");
	parts.push("      var rootFiber = fiberRoot.current;");
	parts.push("      if (!rootFiber) return;");
	parts.push("      var stack = [rootFiber];");
	parts.push("      var visited = 0;");
	parts.push("      while (stack.length > 0 && visited < MAX_FIBERS_PER_COMMIT) {");
	parts.push("        var fiber = stack.pop();");
	parts.push("        visited++;");
	parts.push("        if (!isUserComponent(fiber)) {");
	parts.push("          if (fiber.sibling) stack.push(fiber.sibling);");
	parts.push("          if (fiber.child) stack.push(fiber.child);");
	parts.push("          continue;");
	parts.push("        }");
	parts.push("        var tracking = componentTracking.get(fiber);");
	parts.push("        if (!tracking && fiber.alternate) tracking = componentTracking.get(fiber.alternate);");
	parts.push("        if (!tracking) {");
	parts.push("          tracking = { renderCount: 0, renderTimestamps: [], prevState: null, prevProps: null, prevDeps: {}, _staleCount: {} };");
	parts.push("        }");
	parts.push("        componentTracking.set(fiber, tracking);");
	parts.push("        var isMount = !fiber.alternate;");
	parts.push("        var isUpdate = !isMount && (");
	parts.push("          fiber.memoizedProps !== fiber.alternate.memoizedProps ||");
	parts.push("          fiber.memoizedState !== fiber.alternate.memoizedState");
	parts.push("        );");
	parts.push("        if (isMount || isUpdate) {");
	parts.push("          tracking.renderCount++;");
	parts.push("          var nowTs = Date.now();");
	parts.push("          tracking.renderTimestamps.push(nowTs);");
	parts.push("          // Trim timestamps older than 2s");
	parts.push("          var cutoff = nowTs - 2000;");
	parts.push("          var trimmed = [];");
	parts.push("          for (var ti = 0; ti < tracking.renderTimestamps.length; ti++) {");
	parts.push("            if (tracking.renderTimestamps[ti] > cutoff) trimmed.push(tracking.renderTimestamps[ti]);");
	parts.push("          }");
	parts.push("          tracking.renderTimestamps = trimmed;");
	parts.push("          var changeType = isMount ? 'mount' : 'update';");
	parts.push("          var componentName = getComponentName(fiber);");
	parts.push("          var eventData = {");
	parts.push("            framework: 'react',");
	parts.push("            componentName: componentName,");
	parts.push("            componentPath: getComponentPath(fiber),");
	parts.push("            changeType: changeType,");
	parts.push("            renderCount: tracking.renderCount");
	parts.push("          };");
	parts.push("          if (isUpdate) {");
	parts.push("            try { eventData.changes = computeChanges(fiber, tracking); } catch(e) {}");
	parts.push("            try { eventData.triggerSource = detectTriggerSource(fiber); } catch(e) {}");
	parts.push("          }");
	parts.push("          queueEvent('state', eventData);");
	parts.push("          try { checkPatterns(fiber, tracking, componentName); } catch(e) {}");
	parts.push("          tracking.prevProps = fiber.memoizedProps;");
	parts.push("          tracking.prevState = fiber.memoizedState;");
	parts.push("          try { updateDepsTracking(fiber, tracking); } catch(e) {}");
	parts.push("          // Push children and siblings for changed fiber");
	parts.push("          if (fiber.sibling) stack.push(fiber.sibling);");
	parts.push("          if (fiber.child) stack.push(fiber.child);");
	parts.push("        } else {");
	parts.push("          // Parent unchanged — still visit children (they may have independent updates)");
	parts.push("          if (fiber.sibling) stack.push(fiber.sibling);");
	parts.push("          if (fiber.child) stack.push(fiber.child);");
	parts.push("        }");
	parts.push("      }");
	parts.push("    } catch(e) {}");
	parts.push("  }");
	parts.push("");

	parts.push("  function processUnmount(rendererId, fiber) {");
	parts.push("    try {");
	parts.push("      if (!isUserComponent(fiber)) return;");
	parts.push("      var tracking = componentTracking.get(fiber);");
	parts.push("      queueEvent('state', {");
	parts.push("        framework: 'react',");
	parts.push("        componentName: getComponentName(fiber),");
	parts.push("        componentPath: getComponentPath(fiber),");
	parts.push("        changeType: 'unmount',");
	parts.push("        renderCount: tracking ? tracking.renderCount : 0");
	parts.push("      });");
	parts.push("    } catch(e) {}");
	parts.push("  }");
	parts.push("");

	// ===== SECTION 10: HOOK PATCHING =====
	parts.push("  // ===== HOOK PATCHING =====");
	parts.push("  var hook = window.__REACT_DEVTOOLS_GLOBAL_HOOK__;");
	parts.push("  if (!hook) return;");
	parts.push("");
	parts.push("  var origOnCommit = hook.onCommitFiberRoot;");
	parts.push("  var origOnUnmount = hook.onCommitFiberUnmount;");
	parts.push("");
	parts.push("  hook.onCommitFiberRoot = function(id, root, priority) {");
	parts.push("    if (origOnCommit) {");
	parts.push("      try { origOnCommit.call(hook, id, root, priority); } catch(e) {}");
	parts.push("    }");
	parts.push("    try { processCommit(id, root); } catch(e) {}");
	parts.push("  };");
	parts.push("");
	parts.push("  hook.onCommitFiberUnmount = function(id, fiber) {");
	parts.push("    if (origOnUnmount) {");
	parts.push("      try { origOnUnmount.call(hook, id, fiber); } catch(e) {}");
	parts.push("    }");
	parts.push("    try { processUnmount(id, fiber); } catch(e) {}");
	parts.push("  };");
	parts.push("");
	parts.push("})();");

	return parts.join("\n");
}
