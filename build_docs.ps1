#!/usr/bin/env pwsh
# Build the documentation site
#
# This script:
# 1. Generates rustdoc documentation for the workspace
# 2. Renders one PNG per demo scene (the fallback every embed shows first)
# 3. Compiles the gallery, the single WASM module all embeds share
# 4. Copies the landing page and the embed assets into target/doc
#
# Requirements:
#   - wasm-pack (install: cargo install wasm-pack)
#   - wasm32-unknown-unknown target (install: rustup target add wasm32-unknown-unknown)
#   - a WGPU-capable adapter for the screenshots
#
# Usage:
#   .\build_docs.ps1
#
# Output locations:
#   - target/doc/index.html (landing page)
#   - target/doc/gallery/ (screenshots and the WASM module in pkg/)

$ErrorActionPreference = "Stop"

# getrandom 0.3 (nanoid -> rand 0.9) has no implicit browser backend:
# wasm32-unknown-unknown needs this cfg plus the `wasm_js` crate feature
# (see demos/hello_world/Cargo.toml). Scoped to the wasm target so the
# native `cargo doc` build below is unaffected.
$env:CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS = '--cfg getrandom_backend="wasm_js"'

# Step 1: Build rustdoc documentation
Write-Host "Building workspace documentation..." -ForegroundColor Cyan
Write-Host ""

try {
    cargo doc --workspace --no-deps

    if ($LASTEXITCODE -ne 0) {
        throw "Documentation build failed with exit code $LASTEXITCODE"
    }

    Write-Host "Documentation built successfully" -ForegroundColor Green
    Write-Host ""
} catch {
    Write-Host "Failed to build documentation: $_" -ForegroundColor Red
    exit 1
}

# Step 2: Render the still image every embed shows until it is scrolled into
# view. It is the only thing visitors without WebGPU ever see, so a failure
# here is fatal.
Write-Host "Rendering demo screenshots..." -ForegroundColor Cyan
Write-Host ""

try {
    cargo run -p demo_gallery --bin gallery_screenshots -- target/doc/gallery

    if ($LASTEXITCODE -ne 0) {
        throw "Screenshot rendering failed with exit code $LASTEXITCODE"
    }

    Write-Host "Screenshots rendered" -ForegroundColor Green
    Write-Host ""
} catch {
    Write-Host "Failed to render screenshots: $_" -ForegroundColor Red
    exit 1
}

# Step 3: Build the gallery WASM module
Write-Host "Building the gallery WASM module..." -ForegroundColor Cyan
Write-Host ""

$buildCmd = "wasm-pack build demos/gallery --release --target web --out-dir ../../target/doc/gallery/pkg --out-name demo_gallery"
Write-Host "  $buildCmd" -ForegroundColor DarkGray

try {
    wasm-pack build demos/gallery --release --target web --out-dir ../../target/doc/gallery/pkg --out-name demo_gallery

    if ($LASTEXITCODE -ne 0) {
        throw "Gallery build failed with exit code $LASTEXITCODE"
    }

    Write-Host "Gallery built" -ForegroundColor Green
    Write-Host ""
} catch {
    Write-Host "Failed to build the gallery: $_" -ForegroundColor Red
    exit 1
}

# Step 4: Site assets
Write-Host "Copying site assets..." -ForegroundColor Yellow
Copy-Item site/demo.css, site/demo-loader.js -Destination target/doc/gallery/pkg/
Copy-Item site/index.html, site/site.css -Destination target/doc/

Write-Host "Build complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Landing page: target/doc/index.html"
Write-Host "Gallery:      target/doc/gallery/"
