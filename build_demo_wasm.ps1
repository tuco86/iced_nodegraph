#!/usr/bin/env pwsh
# Build all demos for WASM and embed them in their respective documentation
#
# This script compiles each demo in demos/ to WebAssembly and places the output
# in target/doc/<demo_name>/pkg/ for embedding in rustdoc documentation.
#
# Requirements:
#   - wasm-pack (install: cargo install wasm-pack)
#   - wasm32-unknown-unknown target (install: rustup target add wasm32-unknown-unknown)
#
# Usage:
#   .\build_demo_wasm.ps1
#
# Output locations:
#   - target/doc/demo_hello_world/pkg/
#   - target/doc/demo_interaction/pkg/
#   - target/doc/demo_styling/pkg/

$ErrorActionPreference = "Stop"

# Define all demos to build
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
    }
)

Write-Host "Building WASM demos and embedding in documentation..." -ForegroundColor Cyan
Write-Host ""

foreach ($demo in $demos) {
    Write-Host "Building $($demo.Name) demo..." -ForegroundColor Yellow
    
    # Create output directory in doc structure
    $outDir = "target/doc/$($demo.OutName)/pkg"
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
    
    # Build command with optional wasm feature
    $features = if ($demo.HasWasmFeature) { "--features wasm" } else { "" }
    $buildCmd = "wasm-pack build $($demo.Path) --release --target web --out-dir ../../$outDir --out-name $($demo.OutName) $features"
    
    Write-Host "  Running: $buildCmd" -ForegroundColor Gray
    
    try {
        Invoke-Expression $buildCmd
        
        if ($LASTEXITCODE -ne 0) {
            throw "Build failed with exit code $LASTEXITCODE"
        }
        
        Write-Host "  ✓ Successfully built $($demo.Name)" -ForegroundColor Green
        Write-Host "  Output: $outDir" -ForegroundColor Gray
    } catch {
        Write-Host "  ✗ Failed to build $($demo.Name): $_" -ForegroundColor Red
        exit 1
    }
    
    Write-Host ""
}

Write-Host "All WASM demos built successfully!" -ForegroundColor Green
Write-Host "Demos are embedded in: target/doc/demo_{hello_world,interaction,styling}/pkg/" -ForegroundColor Green
