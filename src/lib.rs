#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

// Only compile wasm-specific code when targeting wasm32.

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_test::wasm_bindgen_test_configure;

    wasm_bindgen_test_configure!(run_in_browser);

    mod render;

    #[wasm_bindgen(start)]
    pub fn main() -> Result<(), JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let canvas = document
            .get_element_by_id("c")
            .ok_or("canvas not found")?
            .dyn_into::<web_sys::HtmlCanvasElement>()?;

        render::start(canvas)?;
        Ok(())
    }
}

// When compiling for non-wasm targets (e.g., `cargo test` on host),
// provide an empty stub so the crate still builds.
#[cfg(not(target_arch = "wasm32"))]
pub fn main() {}
