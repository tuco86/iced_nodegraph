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

// Re-export the main library
pub use iced_nodegraph::*;

// WASM-specific initialization
#[wasm_bindgen(start)]
pub fn main() {
    // Set up panic hook for better error messages in browser console
    console_error_panic_hook::set_once();
    console_log!("WASM module initialized");
}

// WASM entry point for the demo application
#[wasm_bindgen]
pub async fn run_demo() -> Result<(), JsValue> {
    use iced::{Settings, Size};
    
    console_log!("Starting NodeGraph WASM demo...");
    
    // Import the hello_world application
    mod hello_world {
        include!("../examples/hello_world.rs");
    }
    
    let settings = Settings {
        window: iced::window::Settings {
            size: Size::new(1200.0, 800.0),
            ..Default::default()
        },
        ..Default::default()
    };
    
    hello_world::Application::run(settings)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}