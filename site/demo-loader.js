/**
 * Drives the embedded demos of the iced_nodegraph documentation.
 *
 * Every `.demo-embed[data-scene]` figure on the page shows a still image until
 * it scrolls into view; then the gallery wasm module - downloaded on the first
 * activation, never before - opens one window for it inside the figure. All
 * embeds share that one module, one wgpu device and one render loop.
 */

import init, { run_gallery, open_scene, close_scene } from "./demo_gallery.js";

(async function () {
  const embeds = [...document.querySelectorAll(".demo-embed[data-scene]")];

  if (embeds.length === 0) {
    return;
  }

  for (const [i, fig] of embeds.entries()) {
    const scene = fig.dataset.scene;
    const img = fig.querySelector("img");

    // Point the fallback at this build's screenshots; the markup carries the
    // published URLs so it also renders on GitHub and docs.rs.
    if (img) {
      img.src = new URL(`../${scene}.png`, import.meta.url).href;
    }

    fig.dataset.mount = `demo-mount-${scene}-${i}`;
  }

  const caption = (text) => {
    for (const fig of embeds) {
      const figcaption = fig.querySelector("figcaption");

      if (figcaption) {
        figcaption.textContent = text;
      }
    }
  };

  if (!navigator.gpu) {
    caption("WebGPU is not available in this browser; showing a still image.");
    return;
  }

  let ready = null;
  const ensureStarted = () => (ready ??= init().then(() => run_gallery()));

  const focusCanvas = (event) => {
    const canvas = event.currentTarget.querySelector("canvas");

    if (canvas) {
      canvas.tabIndex = 0;
      canvas.focus();
    }
  };

  async function activate(fig) {
    if (fig.dataset.live) {
      return;
    }

    fig.dataset.live = "1";

    try {
      await ensureStarted();
    } catch (error) {
      console.error("Failed to start the demo runtime:", error);
      io.disconnect();
      caption("Failed to start the demo runtime; showing a still image.");
      return;
    }

    if (!fig.dataset.live) {
      return;
    }

    const mount = document.createElement("div");
    mount.id = fig.dataset.mount;
    mount.className = "demo-mount";
    fig.querySelector(".demo-frame").appendChild(mount);

    open_scene(mount.id, fig.dataset.scene);
    fig.addEventListener("click", focusCanvas);
  }

  function deactivate(fig) {
    if (!fig.dataset.live) {
      return;
    }

    delete fig.dataset.live;
    close_scene(fig.dataset.mount);
    // winit leaves the canvas behind, and the mount div survives when the open
    // never completed.
    fig.querySelector(".demo-frame canvas")?.remove();
    fig.querySelector(`#${fig.dataset.mount}`)?.remove();
    fig.removeEventListener("click", focusCanvas);
  }

  const io = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        (entry.isIntersecting ? activate : deactivate)(entry.target);
      }
    },
    { threshold: 0.25 },
  );

  embeds.forEach((fig) => io.observe(fig));
})();
