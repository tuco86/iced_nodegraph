#!/usr/bin/env pwsh
# Build documentation and WASM demos
#
# This script:
# 1. Generates rustdoc documentation for the workspace
# 2. Copies shared static assets (CSS/JS) to each demo's doc folder
# 3. Compiles each demo in demos/ to WebAssembly
# 4. Places WASM output in target/doc/<demo_name>/pkg/ for embedding
#
# Requirements:
#   - wasm-pack (install: cargo install wasm-pack)
#   - wasm32-unknown-unknown target (install: rustup target add wasm32-unknown-unknown)
#
# Usage:
#   .\build_docs.ps1
#
# Output locations:
#   - target/doc/ (rustdoc documentation)
#   - target/doc/demo_*/pkg/ (WASM binaries + static assets)

$ErrorActionPreference = "Stop"

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

# Step 2: Build WASM demos
Write-Host "Building WASM demos..." -ForegroundColor Cyan
Write-Host ""

$demos = @(
    @{
        Name = "demo_hello_world"
        Path = "demos/hello_world"
        OutName = "demo_hello_world"
        HasWasmFeature = $true
    },
    @{
        Name = "demo_interaction"
        Path = "demos/interaction"
        OutName = "demo_interaction"
        HasWasmFeature = $true
    },
    @{
        Name = "demo_styling"
        Path = "demos/styling"
        OutName = "demo_styling"
        HasWasmFeature = $true
    },
    @{
        Name = "demo_500_nodes"
        Path = "demos/500_nodes"
        OutName = "demo_500_nodes"
        HasWasmFeature = $true
    },
    @{
        Name = "demo_shader_editor"
        Path = "demos/shader_editor"
        OutName = "demo_shader_editor"
        HasWasmFeature = $true
    }
)

foreach ($demo in $demos) {
    Write-Host "Building $($demo.Name)..." -ForegroundColor Yellow

    # Create output directory in doc structure
    $outDir = "target/doc/$($demo.OutName)/pkg"
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null

    # Build command with optional wasm feature
    $features = if ($demo.HasWasmFeature) { "--features wasm" } else { "" }
    $buildCmd = "wasm-pack build $($demo.Path) --release --target web --out-dir ../../$outDir --out-name $($demo.OutName) $features"

    Write-Host "  $buildCmd" -ForegroundColor Gray

    try {
        Invoke-Expression $buildCmd

        if ($LASTEXITCODE -ne 0) {
            throw "Build failed with exit code $LASTEXITCODE"
        }

        # Copy static assets into pkg folder alongside WASM files
        Copy-Item "demos/static/demo.css" -Destination $outDir
        Copy-Item "demos/static/demo-loader.js" -Destination $outDir

        Write-Host "  Built $($demo.Name)" -ForegroundColor Green
    } catch {
        Write-Host "  Failed to build $($demo.Name): $_" -ForegroundColor Red
        exit 1
    }

    Write-Host ""
}

Write-Host "Build complete!" -ForegroundColor Green
Write-Host ""
Write-Host "Documentation: target/doc/index.html"
Write-Host "WASM demos:    target/doc/demo_*/pkg/"
