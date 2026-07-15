(() => {
  "use strict";

  // Canvas marker positions below are canvas-local drawing coordinates. The evaluator's
  // affected_region is separately defined in viewport pixels from the rendered 800x450 target;
  // these JavaScript values must never be interpreted as viewport coordinates.

  const CASE_IDS = new Set([
    "movement-reversal/basic",
    "flicker/visibility",
    "flicker/color",
    "flicker/text",
    "layout/width",
    "layout/content-shift",
    "layout/scroll-position",
    "dom-opaque/path-reversal",
    "dom-opaque/teleport",
    "dom-opaque/sprite",
    "stable/smooth-panel",
    "stable/loading-indicator",
    "stable/caret"
  ]);
  const DURATIONS_MS = new Set([16, 33, 50, 100, 200]);
  const params = new URLSearchParams(window.location.search);
  const selectedCase = params.get("case");
  const requestedDuration = Number(params.get("duration_ms"));
  const routeIsValid = CASE_IDS.has(selectedCase)
    && Number.isInteger(requestedDuration)
    && DURATIONS_MS.has(requestedDuration);

  const panel = document.getElementById("panel");
  const statusCard = document.getElementById("status-card");
  const statusText = document.getElementById("status-text");
  const spinner = document.getElementById("spinner");
  const contentBlock = document.getElementById("content-block");
  const notice = document.getElementById("notice");
  const scrollBox = document.getElementById("scroll-box");
  const canvas = document.getElementById("visual-surface");
  const context = canvas.getContext("2d");
  const caret = document.getElementById("caret");
  const runButton = document.getElementById("run");

  let running = false;
  let frameHandle = 0;

  function resetVisuals() {
    if (frameHandle !== 0) {
      cancelAnimationFrame(frameHandle);
      frameHandle = 0;
    }
    panel.style.transform = "translateX(0px)";
    statusCard.hidden = false;
    statusCard.classList.remove("warm");
    statusText.textContent = "Ready";
    spinner.hidden = true;
    spinner.style.transform = "rotate(0deg)";
    contentBlock.classList.remove("narrow");
    contentBlock.style.width = "640px";
    contentBlock.style.top = "216px";
    notice.hidden = true;
    scrollBox.scrollTop = 0;
    caret.classList.remove("off");
    drawSurface("baseline", 0);
    runButton.disabled = !routeIsValid;
    running = false;
  }

  function lerp(start, end, amount) {
    return start + (end - start) * Math.max(0, Math.min(1, amount));
  }

  function drawSurface(kind, elapsed) {
    context.clearRect(0, 0, canvas.width, canvas.height);
    context.fillStyle = "#f5f8fc";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.strokeStyle = "#d2dce7";
    context.lineWidth = 1;
    for (let x = 20; x < canvas.width; x += 40) {
      context.beginPath();
      context.moveTo(x, 0);
      context.lineTo(x, canvas.height);
      context.stroke();
    }
    for (let y = 20; y < canvas.height; y += 40) {
      context.beginPath();
      context.moveTo(0, y);
      context.lineTo(canvas.width, y);
      context.stroke();
    }

    let markerX = 80;
    if (kind === "path-reversal") {
      if (elapsed < 100) {
        markerX = 80;
      } else if (elapsed < 200) {
        markerX = lerp(80, 320, (elapsed - 100) / 100);
      } else if (elapsed < 200 + requestedDuration) {
        markerX = lerp(320, 240, (elapsed - 200) / requestedDuration);
      } else if (elapsed < 300 + requestedDuration) {
        markerX = lerp(240, 320, (elapsed - 200 - requestedDuration) / 100);
      } else {
        markerX = 320;
      }
    } else if (kind === "teleport") {
      markerX = elapsed >= 100 && elapsed < 100 + requestedDuration ? 520 : elapsed < 100 ? 80 : 320;
    } else if (kind === "sprite") {
      markerX = 160;
    }

    if (kind === "sprite" && elapsed >= 100 && elapsed < 100 + requestedDuration) {
      context.fillStyle = "#d46b42";
      context.fillRect(markerX - 16, 80 - 16, 32, 32);
    } else {
      context.fillStyle = "#4679b7";
      context.beginPath();
      context.arc(markerX, 80, 16, 0, Math.PI * 2);
      context.fill();
    }
  }

  function renderFrame(elapsed) {
    const activeStart = 100;
    const activeEnd = activeStart + requestedDuration;
    const movementEnd = 200 + requestedDuration;
    const correctionEnd = 300 + requestedDuration;

    if (selectedCase === "movement-reversal/basic") {
      let x = 48;
      if (elapsed >= 100 && elapsed < 200) {
        x = lerp(48, 160, (elapsed - 100) / 100);
      } else if (elapsed >= 200 && elapsed < movementEnd) {
        x = lerp(160, 120, (elapsed - 200) / requestedDuration);
      } else if (elapsed >= movementEnd && elapsed < correctionEnd) {
        x = lerp(120, 288, (elapsed - movementEnd) / 100);
      } else if (elapsed >= correctionEnd) {
        x = 288;
      }
      panel.style.transform = `translateX(${x - 48}px)`;
    } else if (selectedCase === "stable/smooth-panel") {
      const x = elapsed < requestedDuration
        ? lerp(48, 288, elapsed / requestedDuration)
        : 288;
      panel.style.transform = `translateX(${x - 48}px)`;
    }

    if (selectedCase === "flicker/visibility") {
      statusCard.hidden = elapsed >= activeStart && elapsed < activeEnd;
    } else if (selectedCase === "flicker/color") {
      statusCard.classList.toggle("warm", elapsed >= activeStart && elapsed < activeEnd);
    } else if (selectedCase === "flicker/text") {
      statusText.textContent = elapsed >= activeStart && elapsed < activeEnd ? "Stale data" : "Ready";
    } else if (selectedCase === "layout/width") {
      contentBlock.classList.toggle("narrow", elapsed >= activeStart && elapsed < activeEnd);
      contentBlock.style.width = elapsed >= activeStart && elapsed < activeEnd ? "480px" : "640px";
    } else if (selectedCase === "layout/content-shift") {
      const active = elapsed >= activeStart && elapsed < activeEnd;
      notice.hidden = !active;
      contentBlock.style.top = active ? "264px" : "216px";
    } else if (selectedCase === "layout/scroll-position") {
      scrollBox.scrollTop = elapsed >= activeStart && elapsed < activeEnd ? 160 : 0;
    } else if (selectedCase === "dom-opaque/path-reversal") {
      drawSurface("path-reversal", elapsed);
    } else if (selectedCase === "dom-opaque/teleport") {
      drawSurface("teleport", elapsed);
    } else if (selectedCase === "dom-opaque/sprite") {
      drawSurface("sprite", elapsed);
    } else if (selectedCase === "stable/loading-indicator") {
      const active = elapsed < requestedDuration;
      spinner.hidden = !active;
      spinner.style.transform = `rotate(${(elapsed / 16) * 12}deg)`;
    } else if (selectedCase === "stable/caret") {
      caret.classList.toggle("off", Math.floor(elapsed / 250) % 2 === 1);
    }
  }

  function runScenario() {
    if (!routeIsValid || running) {
      return;
    }
    resetVisuals();
    running = true;
    runButton.disabled = true;
    const startedAt = performance.now();
    const totalDuration = selectedCase === "stable/caret"
      ? Math.max(700, requestedDuration + 200)
      : 500 + requestedDuration;

    function frame(now) {
      const elapsed = now - startedAt;
      renderFrame(elapsed);
      if (elapsed < totalDuration) {
        frameHandle = requestAnimationFrame(frame);
      } else {
        renderFrame(totalDuration);
        frameHandle = 0;
        running = false;
        runButton.disabled = false;
      }
    }

    frameHandle = requestAnimationFrame(frame);
  }

  runButton.addEventListener("click", runScenario);
  resetVisuals();
})();
