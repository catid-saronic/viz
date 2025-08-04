#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;
use wasm_bindgen::JsCast;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test(async)]
async fn canvas_exists() {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let elem = document
        .get_element_by_id("c")
        .expect("canvas element not found");

    let rect = elem
        .dyn_ref::<web_sys::Element>()
        .unwrap()
        .get_bounding_client_rect();

    assert!(rect.width() > 0.0 && rect.height() > 0.0);
}

