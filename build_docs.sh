#!/usr/bin/env bash
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
#   - a WGPU-capable adapter for the screenshots (CI: mesa-vulkan-drivers)
#
# Usage:
#   ./build_docs.sh
#
# Output locations:
#   - target/doc/index.html (landing page)
#   - target/doc/gallery/ (screenshots and the WASM module in pkg/)

set -e

# getrandom 0.3 (nanoid -> rand 0.9) has no implicit browser backend:
# wasm32-unknown-unknown needs this cfg plus the `wasm_js` crate feature
# (see demos/hello_world/Cargo.toml). Scoped to the wasm target so the
# native `cargo doc` build below is unaffected.
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS='--cfg getrandom_backend="wasm_js"'

# Color codes
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
RED='\033[0;31m'
GRAY='\033[0;90m'
NC='\033[0m' # No Color

# Step 1: Build rustdoc documentation
echo -e "${CYAN}Building workspace documentation...${NC}"
echo ""

if cargo doc --workspace --no-deps; then
    echo -e "${GREEN}Documentation built successfully${NC}"
    echo ""
else
    echo -e "${RED}Failed to build documentation${NC}"
    exit 1
fi

# Step 2: Render the still image every embed shows until it is scrolled into
# view. It is the only thing visitors without WebGPU ever see, so a failure
# here is fatal.
echo -e "${CYAN}Rendering demo screenshots...${NC}"
echo ""

if cargo run -p demo_gallery --bin gallery_screenshots -- target/doc/gallery; then
    echo -e "${GREEN}Screenshots rendered${NC}"
    echo ""
else
    echo -e "${RED}Failed to render screenshots${NC}"
    exit 1
fi

# Step 3: Build the gallery WASM module
echo -e "${CYAN}Building the gallery WASM module...${NC}"
echo ""

build_cmd="wasm-pack build demos/gallery --release --target web --out-dir ../../target/doc/gallery/pkg --out-name demo_gallery"
echo -e "${GRAY}  $build_cmd${NC}"

if $build_cmd; then
    echo -e "${GREEN}Gallery built${NC}"
    echo ""
else
    echo -e "${RED}Failed to build the gallery${NC}"
    exit 1
fi

# Step 4: Site assets
echo -e "${YELLOW}Copying site assets...${NC}"
cp site/demo.css site/demo-loader.js target/doc/gallery/pkg/
cp site/index.html site/site.css target/doc/

echo -e "${GREEN}Build complete!${NC}"
echo ""
echo "Landing page: target/doc/index.html"
echo "Gallery:      target/doc/gallery/"
