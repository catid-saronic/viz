
use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{window, HtmlCanvasElement, WebGl2RenderingContext as GL};

/// Start render loop – placeholder draws clear color changing.
pub fn start(canvas: HtmlCanvasElement) -> Result<(), JsValue> {
    let gl: GL = canvas
        .get_context("webgl2")?
        .ok_or("WebGL2 not supported")?
        .dyn_into()?;

    // Resize canvas to fit window
    let resize_closure = {
        let canvas = canvas.clone();
        Closure::wrap(Box::new(move || {
            let w = window().unwrap().inner_width().unwrap().as_f64().unwrap();
            let h = window().unwrap().inner_height().unwrap().as_f64().unwrap();
            canvas.set_width(w as u32);
            canvas.set_height(h as u32);
        }) as Box<dyn FnMut()>)
    };
    window()
        .unwrap()
        .add_event_listener_with_callback("resize", resize_closure.as_ref().unchecked_ref())?;
    resize_closure.forget();

    // Animation loop
    // `f` holds the animation-frame closure so that we can keep calling
    // `request_animation_frame` recursively. Storing it inside an `Option`
    // allows us to create the `Closure` first and then obtain a reference to
    // it from within itself.
    let f: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let g = f.clone();
    let mut t: f32 = 0.0;
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        t += 0.01;
        let r = (t.sin() * 0.5 + 0.5) as f32;
        gl.clear_color(r, 0.0, 0.3, 1.0);
        gl.clear(GL::COLOR_BUFFER_BIT);

        // schedule next
        window()
            .unwrap()
            .request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }) as Box<dyn FnMut()>));

    window()
        .unwrap()
        .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())?;

    Ok(())
}
#![cfg(target_arch = "wasm32")]
