/**
 * Generic WASM Demo Loader for iced_nodegraph
 *
 * This script loads and initializes WASM demos embedded in rustdoc.
 * It automatically detects the module name from its own URL path.
 *
 * Expected URL structure: .../demo_<name>/pkg/demo-loader.js
 * The script extracts "demo_<name>" and loads "./demo_<name>.js"
 */

(async function () {
  const loadingEl = document.getElementById("demo-loading");
  const errorEl = document.getElementById("demo-error");

  try {
    // Extract module name from this script's URL
    // URL pattern: .../target/doc/demo_hello_world/pkg/demo-loader.js
    const scriptUrl = import.meta.url;
    const pathMatch = scriptUrl.match(/\/(demo_[^/]+)\/pkg\/demo-loader\.js/);

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
