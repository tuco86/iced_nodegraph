#!/usr/bin/env bash
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
#   ./build_demo_wasm.sh
#
# Output locations:
#   - target/doc/demo_hello_world/pkg/
#   - target/doc/demo_interaction/pkg/
#   - target/doc/demo_styling/pkg/

set -e

# Color codes
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
RED='\033[0;31m'
GRAY='\033[0;90m'
NC='\033[0m' # No Color

# Define all demos to build
declare -a DEMOS=(
    "demo_hello_world:demos/hello_world:demo_hello_world:wasm"
    "demo_interaction:demos/interaction:demo_interaction:wasm"
    "demo_styling:demos/styling:demo_styling:wasm"
)

echo -e "${CYAN}Building WASM demos and embedding in documentation...${NC}"
echo ""

for demo_config in "${DEMOS[@]}"; do
    IFS=':' read -r name path out_name features <<< "$demo_config"
    
    echo -e "${YELLOW}Building $name demo...${NC}"
    
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
    
    echo -e "${GRAY}  Running: $build_cmd${NC}"
    
    if eval $build_cmd; then
        echo -e "${GREEN}  ✓ Successfully built $name${NC}"
        echo -e "${GRAY}  Output: $out_dir${NC}"
    else
        echo -e "${RED}  ✗ Failed to build $name${NC}"
        exit 1
    fi
    
    echo ""
done

echo -e "${GREEN}All WASM demos built successfully!${NC}"
echo -e "${GREEN}Demos are embedded in: target/doc/demo_{hello_world,interaction,styling}/pkg/${NC}"
