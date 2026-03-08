/**
 * SDF Shape Doc Loader
 *
 * Embeds SDF gallery WASM instances into iced_sdf::Sdf struct documentation.
 * Lazy load-in-view: visible slots get their own isolated WASM instance,
 * slots leaving the viewport get torn down and their target div recreated.
 *
 * Each import uses a unique query param so the browser creates a fresh
 * JS module scope (and thus a fresh WASM instance) per slot.
 */

(function () {
  const slots = document.querySelectorAll(".sdf-shape-slot");
  if (slots.length === 0) return;

  const scriptUrl = import.meta.url;
  const baseUrl = scriptUrl.substring(0, scriptUrl.lastIndexOf("/"));

  // Track active instances: slot -> { shape, targetId }
  const active = new Map();
  // Track slots currently loading to avoid double-init
  const loading = new Set();

  async function activate(slot) {
    if (active.has(slot) || loading.has(slot)) return;

    const shape = slot.dataset.shape;
    const targetId = `sdf-target-${shape}`;

    // Ensure target div exists
    if (!document.getElementById(targetId)) {
      const div = document.createElement("div");
      div.id = targetId;
      div.className = "sdf-target";
      slot.appendChild(div);
    }

    loading.add(slot);

    try {
      // Unique URL = fresh JS module = fresh WASM instance
      const uid = `${shape}_${Date.now()}`;
      const demo = await import(`${baseUrl}/sdf_gallery.js?s=${uid}`);
      await demo.default();

      // Slot may have left viewport during async load
      if (!loading.has(slot)) return;

      demo.run_demo_in(targetId, shape);
      active.set(slot, { shape, targetId });
    } catch (error) {
      console.error(`SDF load error (${shape}):`, error);
    } finally {
      loading.delete(slot);
    }
  }

  function deactivate(slot) {
    // Cancel pending load
    loading.delete(slot);

    const info = active.get(slot);
    if (!info) return;

    // Remove canvas and all iced-generated content
    const target = document.getElementById(info.targetId);
    if (target) {
      target.remove();
    }

    active.delete(slot);
  }

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          activate(entry.target);
        } else {
          deactivate(entry.target);
        }
      }
    },
    { rootMargin: "200px" }
  );

  slots.forEach((slot) => observer.observe(slot));
})();
