#!/usr/bin/env pwsh
# Build hello_world demo for WASM and copy to docs

Write-Host "Building hello_world demo for WASM..." -ForegroundColor Cyan

# Build the WASM package
Push-Location demos/hello_world
wasm-pack build --target web --out-dir pkg --release -- --features wasm

if ($LASTEXITCODE -ne 0) {
    Write-Host "WASM build failed!" -ForegroundColor Red
    Pop-Location
    exit 1
}

Pop-Location

# Create demo directory in target/doc
$demoDir = "target/doc/demo"
New-Item -ItemType Directory -Force -Path $demoDir | Out-Null

# Copy WASM files to doc directory
Write-Host "Copying WASM files to doc directory..." -ForegroundColor Cyan
Copy-Item -Path "demos/hello_world/pkg/*" -Destination $demoDir -Recurse -Force

Write-Host "WASM demo build complete!" -ForegroundColor Green
Write-Host "Files copied to: $demoDir" -ForegroundColor Green
