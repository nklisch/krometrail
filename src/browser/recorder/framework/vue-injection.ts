import { buildInjectionPreamble } from "./injection-helpers.js";
import { getVuePatternCode } from "./patterns/vue-patterns.js";
import type { VueObserverConfig } from "./vue-observer.js";

/**
 * Generate the Vue 3 observer injection script.
 * Returns a self-contained IIFE that hooks into __VUE_DEVTOOLS_GLOBAL_HOOK__
 * to observe component lifecycle and report via __BL__.
 *
 * Uses only `var` declarations — no let/const — for maximum browser compatibility.
 * All state is closure-local. Only side effect is patching the global hook.
 */
export function buildVueInjectionScript(config: Required<VueObserverConfig>): string {
	const parts: string[] = [];

	parts.push("(function() {");
	parts.push("  'use strict';");
	parts.push("");

	// ===== SECTION 1: CONFIGURATION CONSTANTS =====
	parts.push("  // ===== CONFIGURATION =====");
	parts.push(`  var MAX_EVENTS_PER_SECOND = ${config.maxEventsPerSecond};`);
	parts.push(`  var MAX_DEPTH = ${config.maxSerializationDepth};`);
	parts.push(`  var MAX_COMPONENTS_PER_BATCH = ${config.maxComponentsPerBatch};`);
	parts.push(`  var MAX_QUEUE_SIZE = ${config.maxQueueSize};`);
	parts.push("  var INFINITE_LOOP_WINDOW_MS = 2000;");
	parts.push(`  var STORE_OBSERVATION = ${config.storeObservation};`);
	parts.push(`  var STORE_DISCOVERY_INTERVAL_MS = ${config.storeDiscoveryIntervalMs};`);
	parts.push("");

	// ===== SECTION 2: TRACKING STATE =====
	parts.push("  // ===== STATE =====");
	parts.push("  var componentTracking = new Map();");
	parts.push("  var eventQueue = [];");
	parts.push("  var lastFlushTime = 0;");
	parts.push("  var rafScheduled = false;");
	parts.push("  var apps = [];");
	parts.push("  var storeUnsubscribers = [];");
	parts.push("  var knownStoreIds = {};");
	parts.push("  var storeDiscoveryTimer = null;");
	parts.push("");

	// ===== SECTION 3: REPORTING HELPERS =====
	parts.push("  // ===== REPORTING =====");
	parts.push(...buildInjectionPreamble("vue", "update rate"));

	// ===== SECTION 5: VUE COMPONENT UTILITIES =====
	parts.push("  // ===== VUE COMPONENT UTILITIES =====");
	parts.push("  function getComponentName(instance) {");
	parts.push("    var type = instance.type;");
	parts.push("    if (!type) return 'Anonymous';");
	parts.push("    return type.__name || type.name || 'Anonymous';");
	parts.push("  }");
	parts.push("");

	parts.push("  function getComponentPath(instance) {");
	parts.push("    var parts = [];");
	parts.push("    var current = instance;");
	parts.push("    while (current) {");
	parts.push("      var name = getComponentName(current);");
	parts.push("      if (name !== 'Anonymous') parts.unshift(name);");
	parts.push("      current = current.parent;");
	parts.push("      if (parts.length > 10) break;");
	parts.push("    }");
	parts.push("    return parts.join(' > ');");
	parts.push("  }");
	parts.push("");

	// ===== SECTION 6: STATE EXTRACTION =====
	parts.push("  // ===== STATE EXTRACTION =====");
	parts.push("  function extractState(instance) {");
	parts.push("    var state = {};");
	parts.push("    // Composition API: setupState (auto-unwrapped refs via proxyRefs)");
	parts.push("    try {");
	parts.push("      var setup = instance.setupState;");
	parts.push("      if (setup) {");
	parts.push("        var skeys = Object.keys(setup);");
	parts.push("        for (var i = 0; i < skeys.length; i++) {");
	parts.push("          var k = skeys[i];");
	parts.push("          if (k[0] === '$' || k[0] === '_') continue;");
	parts.push("          if (typeof setup[k] === 'function') continue;");
	parts.push("          state['setup.' + k] = setup[k];");
	parts.push("        }");
	parts.push("      }");
	parts.push("    } catch(e) {}");
	parts.push("    // Options API: data()");
	parts.push("    try {");
	parts.push("      var data = instance.data;");
	parts.push("      if (data && Object.keys(data).length > 0) {");
	parts.push("        var dkeys = Object.keys(data);");
	parts.push("        for (var j = 0; j < dkeys.length; j++) {");
	parts.push("          state['data.' + dkeys[j]] = data[dkeys[j]];");
	parts.push("        }");
	parts.push("      }");
	parts.push("    } catch(e) {}");
	parts.push("    // Props");
	parts.push("    try {");
	parts.push("      var props = instance.props;");
	parts.push("      if (props) {");
	parts.push("        var pkeys = Object.keys(props);");
	parts.push("        for (var p = 0; p < pkeys.length; p++) {");
	parts.push("          state['props.' + pkeys[p]] = props[pkeys[p]];");
	parts.push("        }");
	parts.push("      }");
	parts.push("    } catch(e) {}");
	parts.push("    return state;");
	parts.push("  }");
	parts.push("");

	parts.push("  function diffState(prev, next) {");
	parts.push("    if (!prev) return null;");
	parts.push("    var changes = [];");
	parts.push("    var allKeys = {};");
	parts.push("    var k;");
	parts.push("    for (k in prev) allKeys[k] = true;");
	parts.push("    for (k in next) allKeys[k] = true;");
	parts.push("    for (k in allKeys) {");
	parts.push("      var p = prev[k], n = next[k];");
	parts.push("      if (p !== n) {");
	parts.push("        changes.push({ key: k, prev: serialize(p), next: serialize(n) });");
	parts.push("      }");
	parts.push("    }");
	parts.push("    return changes.length > 0 ? changes : null;");
	parts.push("  }");
	parts.push("");

	// ===== SECTION 7: TRIGGER SOURCE DETECTION =====
	parts.push("  // ===== TRIGGER SOURCE DETECTION =====");
	parts.push("  function detectTriggerSource(instance, prevState, nextState) {");
	parts.push("    var propsChanged = false;");
	parts.push("    var stateChanged = false;");
	parts.push("    for (var k in nextState) {");
	parts.push("      if (k.indexOf('props.') === 0) {");
	parts.push("        if (!prevState || prevState[k] !== nextState[k]) propsChanged = true;");
	parts.push("      } else {");
	parts.push("        if (!prevState || prevState[k] !== nextState[k]) stateChanged = true;");
	parts.push("      }");
	parts.push("    }");
	parts.push("    if (stateChanged && !propsChanged) return 'state';");
	parts.push("    if (propsChanged && !stateChanged) return 'props';");
	parts.push("    if (propsChanged && stateChanged) return 'state';");
	parts.push("    return 'parent';");
	parts.push("  }");
	parts.push("");

	// ===== SECTION 8: PATTERN DETECTION =====
	parts.push("  // ===== PATTERN DETECTION =====");
	const patternCode = getVuePatternCode(config);
	for (const line of patternCode.split("\n")) {
		parts.push(`  ${line}`);
	}
	parts.push("");

	// ===== SECTION 9: EVENT HANDLERS =====
	parts.push("  // ===== EVENT HANDLERS =====");
	parts.push("  function handleComponentAdded(instance, app) {");
	parts.push("    try {");
	parts.push("      var uid = instance.uid;");
	parts.push("      if (uid === undefined) return;");
	parts.push("      var state = extractState(instance);");
	parts.push("      var record = {");
	parts.push("        uid: uid,");
	parts.push("        updateCount: 0,");
	parts.push("        updateTimestamps: [],");
	parts.push("        lastState: state,");
	parts.push("        path: null,");
	parts.push("        dirty: false");
	parts.push("      };");
	parts.push("      componentTracking.set(uid, record);");
	parts.push("      record.updateCount++;");
	parts.push("      queueEvent('state', {");
	parts.push("        framework: 'vue',");
	parts.push("        componentName: getComponentName(instance),");
	parts.push("        componentPath: getComponentPath(instance),");
	parts.push("        changeType: 'mount',");
	parts.push("        renderCount: 1");
	parts.push("      });");
	parts.push("    } catch(e) {}");
	parts.push("  }");
	parts.push("");

	parts.push("  function handleComponentUpdated(instance, app) {");
	parts.push("    try {");
	parts.push("      var uid = instance.uid;");
	parts.push("      if (uid === undefined) return;");
	parts.push("      var record = componentTracking.get(uid);");
	parts.push("      if (!record) {");
	parts.push("        handleComponentAdded(instance, app);");
	parts.push("        return;");
	parts.push("      }");
	parts.push("      record.updateCount++;");
	parts.push("      var nowTs = Date.now();");
	parts.push("      record.updateTimestamps.push(nowTs);");
	parts.push("      var cutoff = nowTs - 2000;");
	parts.push("      var trimmed = [];");
	parts.push("      for (var ti = 0; ti < record.updateTimestamps.length; ti++) {");
	parts.push("        if (record.updateTimestamps[ti] > cutoff) trimmed.push(record.updateTimestamps[ti]);");
	parts.push("      }");
	parts.push("      record.updateTimestamps = trimmed;");
	parts.push("      var state = extractState(instance);");
	parts.push("      var changes = diffState(record.lastState, state);");
	parts.push("      var componentName = getComponentName(instance);");
	parts.push("      if (changes) {");
	parts.push("        var triggerSource = detectTriggerSource(instance, record.lastState, state);");
	parts.push("        queueEvent('state', {");
	parts.push("          framework: 'vue',");
	parts.push("          componentName: componentName,");
	parts.push("          componentPath: record.path || (record.path = getComponentPath(instance)),");
	parts.push("          changeType: 'update',");
	parts.push("          changes: changes,");
	parts.push("          renderCount: record.updateCount,");
	parts.push("          triggerSource: triggerSource");
	parts.push("        });");
	parts.push("      }");
	parts.push("      checkPatterns(instance, record, componentName);");
	parts.push("      record.lastState = state;");
	parts.push("    } catch(e) {}");
	parts.push("  }");
	parts.push("");

	parts.push("  function handleComponentRemoved(instance, app) {");
	parts.push("    try {");
	parts.push("      var uid = instance.uid;");
	parts.push("      if (uid === undefined) return;");
	parts.push("      var record = componentTracking.get(uid);");
	parts.push("      queueEvent('state', {");
	parts.push("        framework: 'vue',");
	parts.push("        componentName: getComponentName(instance),");
	parts.push("        componentPath: record ? record.path : getComponentPath(instance),");
	parts.push("        changeType: 'unmount',");
	parts.push("        renderCount: record ? record.updateCount : 0");
	parts.push("      });");
	parts.push("      componentTracking.delete(uid);");
	parts.push("    } catch(e) {}");
	parts.push("  }");
	parts.push("");

	parts.push("  function handleAppInit(app, version) {");
	parts.push("    apps.push(app);");
	parts.push("    if (STORE_OBSERVATION) {");
	parts.push("      try { detectAndObserveStores(app); } catch(e) {}");
	parts.push("      if (!storeDiscoveryTimer) {");
	parts.push("        storeDiscoveryTimer = setInterval(function() {");
	parts.push("          for (var ai = 0; ai < apps.length; ai++) {");
	parts.push("            try { detectAndObserveStores(apps[ai]); } catch(e) {}");
	parts.push("          }");
	parts.push("        }, STORE_DISCOVERY_INTERVAL_MS);");
	parts.push("      }");
	parts.push("    }");
	parts.push("  }");
	parts.push("");

	parts.push("  function handleAppUnmount(app) {");
	parts.push("    for (var i = apps.length - 1; i >= 0; i--) {");
	parts.push("      if (apps[i] === app) { apps.splice(i, 1); break; }");
	parts.push("    }");
	parts.push("    if (apps.length === 0 && storeDiscoveryTimer) {");
	parts.push("      clearInterval(storeDiscoveryTimer);");
	parts.push("      storeDiscoveryTimer = null;");
	parts.push("    }");
	parts.push("  }");
	parts.push("");

	// ===== SECTION 10: HOOK PATCHING & STORE OBSERVATION =====
	parts.push("  // ===== HOOK PATCHING =====");
	parts.push("  var hook = window.__VUE_DEVTOOLS_GLOBAL_HOOK__;");
	parts.push("  if (!hook) return;");
	parts.push("");
	parts.push("  hook.on('component:added', handleComponentAdded);");
	parts.push("  hook.on('component:updated', handleComponentUpdated);");
	parts.push("  hook.on('component:removed', handleComponentRemoved);");
	parts.push("  hook.on('app:init', handleAppInit);");
	parts.push("  hook.on('app:unmount', handleAppUnmount);");
	parts.push("");
	parts.push("  // Drain buffer — process events emitted before our listeners registered");
	parts.push("  var buffered = hook._buffer || [];");
	parts.push("  for (var bi = 0; bi < buffered.length; bi++) {");
	parts.push("    var entry = buffered[bi];");
	parts.push("    var event = entry[0];");
	parts.push("    if (event === 'app:init') {");
	parts.push("      try { handleAppInit(entry[1], entry[2]); } catch(e) {}");
	parts.push("    } else if (event === 'component:added') {");
	parts.push("      try { handleComponentAdded(entry[1], entry[2]); } catch(e) {}");
	parts.push("    }");
	parts.push("  }");
	parts.push("");

	// ===== STORE OBSERVATION =====
	parts.push("  // ===== STORE OBSERVATION =====");
	parts.push("  function detectAndObserveStores(app) {");
	parts.push("    if (!app || !app._context) return;");
	parts.push("    var provides = app._context.provides;");
	parts.push("    if (!provides) return;");
	parts.push("    var pinia = null;");
	parts.push("    var provideKeys = Object.keys(provides);");
	parts.push("    for (var pi = 0; pi < provideKeys.length; pi++) {");
	parts.push("      var val = provides[provideKeys[pi]];");
	parts.push("      if (val && typeof val === 'object' && val._s && val._s instanceof Map) {");
	parts.push("        pinia = val;");
	parts.push("        break;");
	parts.push("      }");
	parts.push("    }");
	parts.push("    if (!pinia) {");
	parts.push("      try {");
	parts.push("        var syms = Object.getOwnPropertySymbols(provides);");
	parts.push("        for (var si = 0; si < syms.length; si++) {");
	parts.push("          var sval = provides[syms[si]];");
	parts.push("          if (sval && typeof sval === 'object' && sval._s && sval._s instanceof Map) {");
	parts.push("            pinia = sval;");
	parts.push("            break;");
	parts.push("          }");
	parts.push("        }");
	parts.push("      } catch(e) {}");
	parts.push("    }");
	parts.push("    if (pinia) {");
	parts.push("      pinia._s.forEach(function(store, id) {");
	parts.push("        if (knownStoreIds[id]) return;");
	parts.push("        knownStoreIds[id] = true;");
	parts.push("        observePiniaStore(id, store);");
	parts.push("      });");
	parts.push("    }");
	parts.push("    try {");
	parts.push("      var vuexStore = app.config && app.config.globalProperties && app.config.globalProperties.$store;");
	parts.push("      if (vuexStore && !knownStoreIds['__vuex__']) {");
	parts.push("        knownStoreIds['__vuex__'] = true;");
	parts.push("        observeVuexStore(vuexStore);");
	parts.push("      }");
	parts.push("    } catch(e) {}");
	parts.push("  }");
	parts.push("");

	parts.push("  function observePiniaStore(id, store) {");
	parts.push("    try {");
	parts.push("      var actionInFlight = null;");
	parts.push("      var unsub1 = store.$subscribe(function(mutation, state) {");
	parts.push("        queueEvent('state', {");
	parts.push("          framework: 'vue',");
	parts.push("          componentName: '[Store] ' + id,");
	parts.push("          changeType: 'store_mutation',");
	parts.push("          storeId: id,");
	parts.push("          mutationType: mutation.type,");
	parts.push("          changes: serializeStoreState(id, state)");
	parts.push("        });");
	parts.push("        if (mutation.type === 'direct' && !actionInFlight) {");
	parts.push("          try { checkPiniaMutationOutsideAction(id); } catch(e) {}");
	parts.push("        }");
	parts.push("      }, { detached: true });");
	parts.push("      storeUnsubscribers.push(unsub1);");
	parts.push("      var unsub2 = store.$onAction(function(context) {");
	parts.push("        actionInFlight = context.name;");
	parts.push("        context.after(function() {");
	parts.push("          queueEvent('state', {");
	parts.push("            framework: 'vue',");
	parts.push("            componentName: '[Store] ' + context.store.$id,");
	parts.push("            changeType: 'store_mutation',");
	parts.push("            storeId: context.store.$id,");
	parts.push("            actionName: context.name");
	parts.push("          });");
	parts.push("          actionInFlight = null;");
	parts.push("        });");
	parts.push("        context.onError(function() {");
	parts.push("          actionInFlight = null;");
	parts.push("        });");
	parts.push("      }, true);");
	parts.push("      storeUnsubscribers.push(unsub2);");
	parts.push("    } catch(e) {}");
	parts.push("  }");
	parts.push("");

	parts.push("  function observeVuexStore(store) {");
	parts.push("    try {");
	parts.push("      var unsub1 = store.subscribe(function(mutation, state) {");
	parts.push("        var modulePath = mutation.type.split('/');");
	parts.push("        var moduleId = modulePath.length > 1 ? modulePath.slice(0, -1).join('/') : 'root';");
	parts.push("        queueEvent('state', {");
	parts.push("          framework: 'vue',");
	parts.push("          componentName: '[Store] vuex',");
	parts.push("          changeType: 'store_mutation',");
	parts.push("          storeId: moduleId,");
	parts.push("          mutationType: mutation.type");
	parts.push("        });");
	parts.push("      });");
	parts.push("      storeUnsubscribers.push(unsub1);");
	parts.push("      var unsub2 = store.subscribeAction({");
	parts.push("        after: function(action, state) {");
	parts.push("          var modulePath = action.type.split('/');");
	parts.push("          var moduleId = modulePath.length > 1 ? modulePath.slice(0, -1).join('/') : 'root';");
	parts.push("          queueEvent('state', {");
	parts.push("            framework: 'vue',");
	parts.push("            componentName: '[Store] vuex',");
	parts.push("            changeType: 'store_mutation',");
	parts.push("            storeId: moduleId,");
	parts.push("            actionName: action.type");
	parts.push("          });");
	parts.push("        }");
	parts.push("      });");
	parts.push("      storeUnsubscribers.push(unsub2);");
	parts.push("    } catch(e) {}");
	parts.push("  }");
	parts.push("");

	parts.push("  function serializeStoreState(storeId, state) {");
	parts.push("    var prevKey = '__store_' + storeId;");
	parts.push("    var prev = componentTracking.get(prevKey);");
	parts.push("    if (!prev) {");
	parts.push("      componentTracking.set(prevKey, { lastState: {} });");
	parts.push("      prev = componentTracking.get(prevKey);");
	parts.push("    }");
	parts.push("    var next = {};");
	parts.push("    try {");
	parts.push("      var keys = Object.keys(state);");
	parts.push("      for (var i = 0; i < keys.length; i++) {");
	parts.push("        next[keys[i]] = state[keys[i]];");
	parts.push("      }");
	parts.push("    } catch(e) {}");
	parts.push("    var changes = diffState(prev.lastState, next);");
	parts.push("    prev.lastState = next;");
	parts.push("    return changes;");
	parts.push("  }");
	parts.push("");

	parts.push("})();");

	return parts.join("\n");
}
