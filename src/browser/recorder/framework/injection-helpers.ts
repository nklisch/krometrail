/**
 * Build the shared injection script preamble: blReport, queueEvent, flushEvents, serialize.
 * Called by both React and Vue injection builders.
 */
export function buildInjectionPreamble(framework: string, rateDescription = "update rate"): string[] {
	const parts: string[] = [];

	// ===== blReport =====
	parts.push("  function blReport(type, data) {");
	parts.push("    try {");
	parts.push("      console.debug('__BL__', JSON.stringify({ type: 'framework_' + type, ts: Date.now(), data: data }));");
	parts.push("    } catch(e) {}");
	parts.push("  }");
	parts.push("");

	// ===== queueEvent =====
	parts.push("  function queueEvent(type, data) {");
	parts.push("    // Coalesce updates to the same component");
	parts.push("    if (type === 'state' && data.changeType === 'update') {");
	parts.push("      var scanLimit = Math.min(eventQueue.length, 30);");
	parts.push("      for (var qi = eventQueue.length - 1; qi >= eventQueue.length - scanLimit; qi--) {");
	parts.push("        var existing = eventQueue[qi];");
	parts.push("        if (existing && existing.type === 'state'");
	parts.push("            && existing.data.changeType === 'update'");
	parts.push("            && existing.data.componentName === data.componentName) {");
	parts.push("          existing.data.changes = data.changes;");
	parts.push("          existing.data.renderCount = data.renderCount;");
	parts.push("          existing.data.triggerSource = data.triggerSource;");
	parts.push("          if (!rafScheduled) {");
	parts.push("            rafScheduled = true;");
	parts.push("            requestAnimationFrame(flushEvents);");
	parts.push("          }");
	parts.push("          return;");
	parts.push("        }");
	parts.push("      }");
	parts.push("    }");
	parts.push("    eventQueue.push({ type: type, data: data });");
	parts.push("    // Overflow protection");
	parts.push("    if (eventQueue.length > MAX_QUEUE_SIZE) {");
	parts.push("      var dropped = eventQueue.length - Math.floor(MAX_QUEUE_SIZE / 2);");
	parts.push("      eventQueue = eventQueue.slice(-Math.floor(MAX_QUEUE_SIZE / 2));");
	parts.push("      blReport('error', {");
	parts.push(`        framework: '${framework}',`);
	parts.push("        pattern: 'observer_overflow',");
	parts.push("        componentName: '[Observer]',");
	parts.push("        severity: 'low',");
	parts.push(`        detail: 'Dropped ' + dropped + ' framework events due to high ${rateDescription}.',`);
	parts.push("        evidence: { dropped: dropped }");
	parts.push("      });");
	parts.push("    }");
	parts.push("    if (!rafScheduled) {");
	parts.push("      rafScheduled = true;");
	parts.push("      requestAnimationFrame(flushEvents);");
	parts.push("    }");
	parts.push("  }");
	parts.push("");

	// ===== flushEvents =====
	parts.push("  function flushEvents() {");
	parts.push("    rafScheduled = false;");
	parts.push("    var now = Date.now();");
	parts.push("    var elapsed = now - lastFlushTime;");
	parts.push("    var budget = Math.floor(MAX_EVENTS_PER_SECOND * (elapsed / 1000));");
	parts.push("    if (budget < 1) budget = 1;");
	parts.push("    var toSend = eventQueue.splice(0, budget);");
	parts.push("    for (var fi = 0; fi < toSend.length; fi++) {");
	parts.push("      blReport(toSend[fi].type, toSend[fi].data);");
	parts.push("    }");
	parts.push("    lastFlushTime = now;");
	parts.push("    if (eventQueue.length > 0) {");
	parts.push("      rafScheduled = true;");
	parts.push("      requestAnimationFrame(flushEvents);");
	parts.push("    }");
	parts.push("  }");
	parts.push("");

	// ===== serialize =====
	parts.push("  function serialize(value, depth) {");
	parts.push("    if (depth === undefined) depth = 0;");
	parts.push("    if (depth >= MAX_DEPTH) {");
	parts.push("      if (Array.isArray(value)) return '[Array(' + value.length + ')]';");
	parts.push("      if (value && typeof value === 'object') return '[Object]';");
	parts.push("      return value;");
	parts.push("    }");
	parts.push("    if (value === null || value === undefined) return value;");
	parts.push("    if (typeof value === 'function') return '[Function: ' + (value.name || 'anonymous') + ']';");
	parts.push("    if (typeof value === 'symbol') return value.toString();");
	parts.push("    if (typeof value !== 'object') {");
	parts.push("      if (typeof value === 'string' && value.length > 200) return value.slice(0, 200) + '...';");
	parts.push("      return value;");
	parts.push("    }");
	parts.push("    if (Array.isArray(value)) {");
	parts.push("      var sarr = [];");
	parts.push("      for (var si = 0; si < Math.min(value.length, 10); si++) {");
	parts.push("        sarr.push(serialize(value[si], depth + 1));");
	parts.push("      }");
	parts.push("      if (value.length > 10) sarr.push('...(' + (value.length - 10) + ' more)');");
	parts.push("      return sarr;");
	parts.push("    }");
	parts.push("    var sobj = {};");
	parts.push("    var skeys = Object.keys(value);");
	parts.push("    for (var sk = 0; sk < Math.min(skeys.length, 20); sk++) {");
	parts.push("      try { sobj[skeys[sk]] = serialize(value[skeys[sk]], depth + 1); } catch(e) { sobj[skeys[sk]] = '[Error]'; }");
	parts.push("    }");
	parts.push("    if (skeys.length > 20) sobj['...'] = '(' + (skeys.length - 20) + ' more keys)';");
	parts.push("    return sobj;");
	parts.push("  }");
	parts.push("");

	return parts;
}
