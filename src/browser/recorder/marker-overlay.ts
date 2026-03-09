import type { CDPClient } from "./cdp-client.js";

const MARK_BINDING = "agentLensMark";
const SCREENSHOT_BINDING = "agentLensScreenshot";

/**
 * Page script injected via CDP into every recorded tab.
 * Renders a floating control panel (bottom-right) with:
 *   - ◎ Mark button  — calls agentLensMark CDP binding
 *   - 📷 Snap button — calls agentLensScreenshot CDP binding
 *   - Auto-capture interval display
 * Keyboard shortcuts: Ctrl+Shift+M (mark), Ctrl+Shift+S (snap).
 */
function getControlPanelScript(intervalMs: number): string {
	const intervalLabel = intervalMs > 0 ? `${intervalMs / 1000}s` : "off";
	return `(function() {
  if (window.__agentLensPanel) return;
  window.__agentLensPanel = true;

  // --- Panel ---
  var panel = document.createElement('div');
  panel.id = '__agent_lens_panel';
  var ps = panel.style;
  ps.position = 'fixed';
  ps.bottom = '16px';
  ps.right = '16px';
  ps.zIndex = '2147483647';
  ps.background = '#0f172a';
  ps.border = '1px solid #1e3a5f';
  ps.borderRadius = '10px';
  ps.padding = '8px 10px 7px';
  ps.fontFamily = 'ui-monospace, SFMono-Regular, monospace';
  ps.fontSize = '11px';
  ps.color = '#94a3b8';
  ps.boxShadow = '0 4px 20px rgba(0,0,0,0.55)';
  ps.userSelect = 'none';
  ps.minWidth = '190px';
  ps.lineHeight = '1';

  // --- Header ---
  var header = document.createElement('div');
  header.style.cssText = 'display:flex;align-items:center;gap:6px;margin-bottom:8px;padding-bottom:7px;border-bottom:1px solid #1e293b;';
  var dot = document.createElement('span');
  dot.style.cssText = 'width:7px;height:7px;border-radius:50%;background:#22c55e;display:inline-block;box-shadow:0 0 5px #22c55e88;flex-shrink:0;';
  var title = document.createElement('span');
  title.style.cssText = 'color:#e2e8f0;font-weight:700;font-size:11px;letter-spacing:0.06em;text-transform:uppercase;';
  title.textContent = 'agent-lens';
  header.appendChild(dot);
  header.appendChild(title);
  panel.appendChild(header);

  // --- Button helpers ---
  function makeBtn(icon, label, title, accentColor) {
    var btn = document.createElement('button');
    btn.title = title;
    btn.style.cssText = [
      'display:inline-flex;align-items:center;gap:4px;',
      'background:#1e293b;color:#cbd5e1;',
      'border:1px solid #334155;border-radius:6px;',
      'padding:5px 9px;font-size:11px;',
      'font-family:ui-monospace,SFMono-Regular,monospace;',
      'cursor:pointer;transition:background 0.12s,border-color 0.12s,color 0.12s;',
      'outline:none;',
    ].join('');
    btn.innerHTML = '<span>' + icon + '</span><span class="lbl">' + label + '</span>';
    btn.onmouseenter = function() { btn.style.background = '#263548'; btn.style.borderColor = accentColor; btn.style.color = '#f1f5f9'; };
    btn.onmouseleave = function() { btn.style.background = '#1e293b'; btn.style.borderColor = '#334155'; btn.style.color = '#cbd5e1'; };
    return btn;
  }

  function flash(btn, label, bg, border) {
    var lbl = btn.querySelector('.lbl');
    var origLabel = lbl.textContent;
    var origBg = btn.style.background;
    var origBorder = btn.style.borderColor;
    lbl.textContent = label;
    btn.style.background = bg;
    btn.style.borderColor = border;
    btn.style.color = '#f1f5f9';
    setTimeout(function() {
      lbl.textContent = origLabel;
      btn.style.background = origBg;
      btn.style.borderColor = origBorder;
      btn.style.color = '#cbd5e1';
    }, 1000);
  }

  // --- Button row ---
  var row = document.createElement('div');
  row.style.cssText = 'display:flex;gap:6px;';

  // Mark button
  var markBtn = makeBtn('\\u25CE', 'Mark', 'Place marker (Ctrl+Shift+M)', '#22c55e');
  function triggerMark() {
    window.agentLensMark('user');
    flash(markBtn, 'Marked!', '#14532d', '#22c55e');
  }
  markBtn.onclick = triggerMark;
  row.appendChild(markBtn);

  // Snap button
  var snapBtn = makeBtn('\\uD83D\\uDCF7', 'Snap', 'Capture screenshot (Ctrl+Shift+S)', '#3b82f6');
  function triggerSnap() {
    window.agentLensScreenshot('manual');
    flash(snapBtn, 'Saved!', '#1e3a5f', '#3b82f6');
  }
  snapBtn.onclick = triggerSnap;
  row.appendChild(snapBtn);

  panel.appendChild(row);

  // --- Footer: interval indicator ---
  var footer = document.createElement('div');
  footer.style.cssText = 'margin-top:7px;padding-top:6px;border-top:1px solid #1e293b;color:#475569;font-size:10px;display:flex;align-items:center;gap:5px;';
  footer.innerHTML = '<span style="opacity:0.7">\\u23F1</span><span>auto: <span style="color:#64748b">${intervalLabel}</span></span>';
  panel.appendChild(footer);

  // --- Keyboard shortcuts ---
  document.addEventListener('keydown', function(e) {
    if (e.ctrlKey && e.shiftKey && (e.key === 'M' || e.key === 'm')) { e.preventDefault(); triggerMark(); }
    if (e.ctrlKey && e.shiftKey && (e.key === 'S' || e.key === 's')) { e.preventDefault(); triggerSnap(); }
  });

  function mount() {
    if (document.body && !document.getElementById('__agent_lens_panel')) {
      document.body.appendChild(panel);
    }
  }
  if (document.body) { mount(); }
  else { document.addEventListener('DOMContentLoaded', mount); }
})();`;
}

/**
 * Registers agentLensMark + agentLensScreenshot CDP bindings, injects the control panel.
 * Returns a cleanup function.
 */
export async function setupControlPanel(
	cdpClient: CDPClient,
	sessionId: string,
	placeMarker: (label?: string) => Promise<unknown>,
	takeScreenshot: (() => Promise<void>) | null,
	intervalMs: number,
): Promise<() => void> {
	const script = getControlPanelScript(intervalMs);

	await cdpClient.sendToTarget(sessionId, "Runtime.addBinding", { name: MARK_BINDING }).catch(() => {});
	await cdpClient.sendToTarget(sessionId, "Runtime.addBinding", { name: SCREENSHOT_BINDING }).catch(() => {});
	await cdpClient.sendToTarget(sessionId, "Page.addScriptToEvaluateOnNewDocument", { source: script }).catch(() => {});
	await cdpClient.sendToTarget(sessionId, "Runtime.evaluate", { expression: script }).catch(() => {});

	function onEvent(eventSessionId: string, method: string, params: Record<string, unknown>) {
		if (eventSessionId !== sessionId || method !== "Runtime.bindingCalled") return;
		if (params.name === MARK_BINDING) {
			const label = typeof params.payload === "string" && params.payload ? params.payload : undefined;
			placeMarker(label).catch(() => {});
		} else if (params.name === SCREENSHOT_BINDING && takeScreenshot) {
			takeScreenshot().catch(() => {});
		}
	}

	cdpClient.on("event", onEvent);
	return () => cdpClient.off("event", onEvent);
}
