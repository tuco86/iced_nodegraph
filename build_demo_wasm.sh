#!/usr/bin/env bash
# Build hello_world demo for WASM and copy to docs

set -e

echo "Building hello_world demo for WASM..."

# Build the WASM package
cd demos/hello_world
wasm-pack build --target web --out-dir pkg --release -- --features wasm
cd ../..

# Create demo directory in target/doc
mkdir -p target/doc/demo

# Copy WASM files to doc directory
echo "Copying WASM files to doc directory..."
cp -r demos/hello_world/pkg/* target/doc/demo/

echo "WASM demo build complete!"
echo "Files copied to: target/doc/demo"
