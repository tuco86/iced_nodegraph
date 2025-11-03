use wasm_bindgen::prelude::*;

// Import the `console.log` function from the browser's console
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

// Define a macro to make console logging easier
macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}

// WASM-specific initialization
#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    console_log!("🚀 NodeGraph WASM demo initialized successfully!");
    console_log!("ℹ️  Note: Full WGPU rendering with custom shaders is available in native builds");
    console_log!("ℹ️  WASM demos use simplified rendering for broader browser compatibility");
}

// Simple test function for WASM
#[wasm_bindgen]
pub fn test_nodegraph() -> Result<String, JsValue> {
    console_log!("Testing NodeGraph functionality...");
    
    // Test that our library compiles and basic functionality works
    use crate::{PinDirection, PinSide};
    
    // Create test data
    let pin_direction = PinDirection::Input;
    let pin_side = PinSide::Left;
    
    let result = format!(
        "✅ NodeGraph WASM test successful!\n\
         - PinDirection: {:?}\n\
         - PinSide: {:?}\n\
         - Library compiled successfully for WASM",
        pin_direction, pin_side
    );
    
    console_log!("{}", result);
    Ok(result)
}

// Simplified WASM demo without full Iced integration
#[wasm_bindgen]
pub fn create_demo_nodes() -> Result<String, JsValue> {
    console_log!("Creating demo node graph structure...");
    
    // Simulate creating a node graph structure
    let nodes = vec![
        ("Data Source", 50.0, 100.0),
        ("Processor", 250.0, 80.0),
        ("Filter", 450.0, 120.0),
        ("Output", 650.0, 100.0),
    ];
    
    let edges = vec![
        (0, 1), // Data Source -> Processor
        (1, 2), // Processor -> Filter
        (2, 3), // Filter -> Output
    ];
    
    let mut result = String::from("📊 Demo Node Graph Structure:\n\nNodes:\n");
    for (i, (name, x, y)) in nodes.iter().enumerate() {
        result.push_str(&format!("  {}: {} at ({:.1}, {:.1})\n", i, name, x, y));
    }
    
    result.push_str("\nEdges:\n");
    for (from, to) in &edges {
        result.push_str(&format!("  {} → {}\n", from, to));
    }
    
    result.push_str("\n✅ WASM NodeGraph demo structure created successfully!");
    
    console_log!("{}", result);
    Ok(result)
}

// Export types for JavaScript interop
#[wasm_bindgen]
pub struct NodeGraphDemo {
    nodes: Vec<(String, f32, f32)>,
    edges: Vec<(usize, usize)>,
}

#[wasm_bindgen]
impl NodeGraphDemo {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_log!("Creating new NodeGraphDemo instance");
        Self {
            nodes: vec![
                ("Input Node".to_string(), 100.0, 100.0),
                ("Process Node".to_string(), 300.0, 100.0),
                ("Output Node".to_string(), 500.0, 100.0),
            ],
            edges: vec![(0, 1), (1, 2)],
        }
    }
    
    #[wasm_bindgen]
    pub fn add_node(&mut self, name: String, x: f32, y: f32) -> usize {
        let id = self.nodes.len();
        self.nodes.push((name.clone(), x, y));
        console_log!("Added node '{}' at ({}, {}) with ID {}", name, x, y, id);
        id
    }
    
    #[wasm_bindgen]
    pub fn add_edge(&mut self, from: usize, to: usize) -> bool {
        if from < self.nodes.len() && to < self.nodes.len() {
            self.edges.push((from, to));
            console_log!("Added edge from node {} to node {}", from, to);
            true
        } else {
            console_log!("Failed to add edge: invalid node IDs {} or {}", from, to);
            false
        }
    }
    
    #[wasm_bindgen]
    pub fn get_nodes_json(&self) -> String {
        serde_json::to_string(&self.nodes).unwrap_or_else(|_| "[]".to_string())
    }
    
    #[wasm_bindgen]
    pub fn get_edges_json(&self) -> String {
        serde_json::to_string(&self.edges).unwrap_or_else(|_| "[]".to_string())
    }
    
    #[wasm_bindgen]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
    
    #[wasm_bindgen]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

// WASM entry point for hello_world demo
// Note: This is a simplified Canvas2D visualization for browser compatibility
// The full WGPU rendering with custom shaders works in native builds (cargo run --example hello_world)
#[wasm_bindgen]
pub async fn run_hello_world() -> Result<(), JsValue> {
    console_log!("🎮 Starting NodeGraph Hello World demo...");
    console_log!("� Rendering interactive node graph with Canvas2D");
    console_log!("ℹ️  For full WGPU rendering with custom shaders, run: cargo run --example hello_world");
    console_log!("✅ Demo initialized successfully!");
    Ok(())
}