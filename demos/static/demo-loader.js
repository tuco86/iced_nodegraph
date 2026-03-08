/**
 * Generic WASM Demo Loader for iced_nodegraph
 *
 * This script loads and initializes WASM demos embedded in rustdoc.
 * It automatically detects the module name from its own URL path.
 *
 * Expected URL structure: .../<module_name>/pkg/demo-loader.js
 * The script extracts "<module_name>" and loads "./<module_name>.js"
 *
 * Embed mode: append ?embed=true to the page URL to hide rustdoc
 * chrome and fullscreen the demo container (used for iframe embeds).
 */

(async function () {
  // Embed mode: hide rustdoc chrome, fullscreen the demo container
  const params = new URLSearchParams(window.location.search);
  if (params.get("embed") === "true") {
    const style = document.createElement("style");
    style.textContent = `
      body > :not(#demo-container) { display: none !important; }
      #demo-container { position: fixed; inset: 0; height: 100vh !important; border-radius: 0 !important; }
    `;
    document.head.appendChild(style);
  }

  const loadingEl = document.getElementById("demo-loading");
  const errorEl = document.getElementById("demo-error");

  try {
    // Extract module name from this script's URL
    // Matches both "demo_*" and other names like "sdf_gallery"
    const scriptUrl = import.meta.url;
    const pathMatch = scriptUrl.match(/\/([^/]+)\/pkg\/demo-loader\.js/);

    if (!pathMatch) {
      throw new Error(`Cannot extract module name from URL: ${scriptUrl}`);
    }

    const moduleName = pathMatch[1];

    // Import the WASM module from same directory
    const demo = await import(`./${moduleName}.js`);

    // Initialize WASM binary
    await demo.default();

    // Hide loading indicator
    if (loadingEl) {
      loadingEl.style.display = "none";
    }

    // Start the demo
    demo.run_demo();

    // Set up canvas focus for keyboard input
    setTimeout(() => {
      const canvas = document.querySelector("#demo-canvas-container canvas");
      if (canvas) {
        canvas.setAttribute("tabindex", "0");
        canvas.focus();
      }
    }, 100);
  } catch (error) {
    console.error("Demo initialization error:", error);

    // Hide loading, show error
    if (loadingEl) {
      loadingEl.style.display = "none";
    }
    if (errorEl) {
      errorEl.style.display = "block";
    }
  }
})();
