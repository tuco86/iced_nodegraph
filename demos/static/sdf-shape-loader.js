/**
 * SDF Shape Doc Loader
 *
 * Embeds the SDF gallery WASM app into iced_sdf::Sdf struct documentation.
 * One app instance is loaded and moved between shape slots as the user scrolls.
 *
 * Each Sdf method has a <div class="sdf-shape-slot" data-shape="<slug>">
 * placeholder. This script:
 *   1. Loads the gallery WASM once
 *   2. Captures the canvas iced/winit creates (it lands in <body>)
 *   3. Reparents it into the active slot
 *   4. IntersectionObserver moves it between slots on scroll
 *   5. Sets window.__sdf_shape so the gallery app switches shape
 */

(async function () {
  const slots = document.querySelectorAll(".sdf-shape-slot");
  if (slots.length === 0) return;

  const container = document.getElementById("sdf-demo-container");
  if (!container) return;

  // Track active slot
  let currentSlot = null;
  window.__sdf_shape = slots[0].dataset.shape;

  function moveToSlot(slot) {
    if (currentSlot === slot) return;
    currentSlot = slot;
    slot.appendChild(container);
    container.style.display = "block";
    window.__sdf_shape = slot.dataset.shape;
    window.dispatchEvent(new Event("resize"));
  }

  moveToSlot(slots[0]);

  try {
    const scriptUrl = import.meta.url;
    const baseUrl = scriptUrl.substring(0, scriptUrl.lastIndexOf("/"));
    const demo = await import(`${baseUrl}/sdf_gallery.js`);
    await demo.default();

    const loadingEl = document.getElementById("sdf-demo-loading");
    if (loadingEl) loadingEl.style.display = "none";

    demo.run_demo();

    // iced/winit creates the canvas in <body>. We need to capture it
    // and move it into our container. Poll until it appears.
    function captureCanvas() {
      // winit creates a canvas directly in body
      const canvas = document.querySelector("body > canvas");
      if (canvas) {
        // Move canvas into our container
        const canvasContainer = document.getElementById("demo-canvas-container");
        if (canvasContainer) {
          canvasContainer.appendChild(canvas);
        } else {
          container.appendChild(canvas);
        }
        canvas.style.width = "100%";
        canvas.style.height = "100%";
        canvas.setAttribute("tabindex", "0");
        window.dispatchEvent(new Event("resize"));
        return;
      }
      // Not there yet, retry
      requestAnimationFrame(captureCanvas);
    }
    requestAnimationFrame(captureCanvas);
  } catch (error) {
    console.error("SDF gallery load error:", error);
    const errorEl = document.getElementById("sdf-demo-error");
    if (errorEl) errorEl.style.display = "block";
    const loadingEl = document.getElementById("sdf-demo-loading");
    if (loadingEl) loadingEl.style.display = "none";
    return;
  }

  // IntersectionObserver: move canvas to the most visible slot
  const visibilityMap = new Map();

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        visibilityMap.set(entry.target, entry.intersectionRatio);
      }

      let bestSlot = null;
      let bestRatio = 0;
      for (const [slot, ratio] of visibilityMap) {
        if (ratio > bestRatio) {
          bestRatio = ratio;
          bestSlot = slot;
        }
      }

      if (bestSlot && bestRatio > 0.2) {
        moveToSlot(bestSlot);
      }
    },
    { threshold: [0, 0.2, 0.5, 0.8, 1.0] }
  );

  slots.forEach((slot) => observer.observe(slot));
})();
