#!/usr/bin/env bash
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
#   ./build_docs.sh
#
# Output locations:
#   - target/doc/ (rustdoc documentation)
#   - target/doc/demo_*/pkg/ (WASM binaries + static assets)

set -e

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

# Step 2: Build WASM demos
echo -e "${CYAN}Building WASM demos...${NC}"
echo ""

# Define all demos to build (name:path:out_name:features)
declare -a DEMOS=(
    "demo_hello_world:demos/hello_world:demo_hello_world:wasm"
    "demo_interaction:demos/interaction:demo_interaction:wasm"
    "demo_styling:demos/styling:demo_styling:wasm"
    "demo_500_nodes:demos/500_nodes:demo_500_nodes:wasm"
    "demo_shader_editor:demos/shader_editor:demo_shader_editor:wasm"
)

for demo_config in "${DEMOS[@]}"; do
    IFS=':' read -r name path out_name features <<< "$demo_config"

    echo -e "${YELLOW}Building $name...${NC}"

    # Create output directory in doc structure
    out_dir="target/doc/$out_name/pkg"
    mkdir -p "$out_dir"

    # Build command with optional wasm feature
    if [ -n "$features" ]; then
        features_flag="--features $features"
    else
        features_flag=""
    fi

    build_cmd="wasm-pack build $path --release --target web --out-dir ../../$out_dir --out-name $out_name $features_flag"

    echo -e "${GRAY}  $build_cmd${NC}"

    if eval $build_cmd; then
        # Copy static assets into pkg folder alongside WASM files
        cp demos/static/demo.css "$out_dir/"
        cp demos/static/demo-loader.js "$out_dir/"
        echo -e "${GREEN}  Built $name${NC}"
    else
        echo -e "${RED}  Failed to build $name${NC}"
        exit 1
    fi

    echo ""
done

echo -e "${GREEN}Build complete!${NC}"
echo ""
echo "Documentation: target/doc/index.html"
echo "WASM demos:    target/doc/demo_*/pkg/"
