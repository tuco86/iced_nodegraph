# GitHub Pages Demo

This directory contains the GitHub Pages website for the iced_nodegraph project.

## Features

- 🎨 Modern, responsive design with Catppuccin theme
- 📱 Mobile-friendly layout
- 🚀 Automatic deployment via GitHub Actions
- 📖 Comprehensive documentation and examples
- 🔗 Direct links to source code and examples

## Local Development

To run the demo locally:

```bash
# Serve the static files (Python 3)
cd docs
python -m http.server 8000

# Or with Node.js
npx serve .

# Then open http://localhost:8000
```

## WASM Demo (Future)

The infrastructure is ready for a WebAssembly demo. To add WASM support:

1. Install wasm-pack: `curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh`
2. Configure Iced for WASM target (canvas backend)
3. Build with: `wasm-pack build --target web`
4. Update GitHub Actions to build WASM automatically

## Structure

```
docs/
├── index.html          # Main landing page
├── style.css          # Optional: separate CSS (currently inline)
└── pkg/               # Future: WASM output directory
    ├── *.wasm         # WebAssembly binary
    ├── *.js           # JavaScript bindings
    └── package.json   # NPM package info
```

## Deployment

The site is automatically deployed to GitHub Pages when pushing to the main branch via `.github/workflows/deploy.yml`.

Access the live demo at: `https://tuco86.github.io/iced_nodegraph/`