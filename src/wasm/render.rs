
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{window, HtmlCanvasElement, WebGl2RenderingContext as GL};

/// Start render loop – placeholder draws clear color changing.
pub fn start(canvas: HtmlCanvasElement) -> Result<(), JsValue> {
    use std::cell::RefCell;
    use std::rc::Rc;

    let gl: GL = canvas
        .get_context("webgl2")?
        .ok_or("WebGL2 not supported")?
        .dyn_into()?;

    // Helper to match the canvas size & WebGL viewport to the current window size.
    // Doing this via a small closure keeps the logic in one place so we can invoke
    // it both on start-up and for every resize event.
    let adjust_size = |canvas: &HtmlCanvasElement, gl: &GL| {
        let w = window()
            .unwrap()
            .inner_width()
            .unwrap()
            .as_f64()
            .unwrap();
        let h = window()
            .unwrap()
            .inner_height()
            .unwrap()
            .as_f64()
            .unwrap();
        // Only update if a change is actually required to avoid needless work.
        if canvas.width() != w as u32 || canvas.height() != h as u32 {
            canvas.set_width(w as u32);
            canvas.set_height(h as u32);
            gl.viewport(0, 0, w as i32, h as i32);
        }
    };

    // Initial sizing so the canvas fits the window immediately.
    adjust_size(&canvas, &gl);

    // Resize canvas whenever the window changes.
    let resize_closure = {
        let canvas = canvas.clone();
        let gl = gl.clone();
        Closure::wrap(Box::new(move || {
            adjust_size(&canvas, &gl);
        }) as Box<dyn FnMut()>)
    };
    window()
        .unwrap()
        .add_event_listener_with_callback("resize", resize_closure.as_ref().unchecked_ref())?;
    resize_closure.forget();

    // ---------- Visualization framework ----------

    trait Visualizer {
        fn name(&self) -> &'static str;
        fn init(&mut self, _gl: &GL) {}
        fn render(&mut self, gl: &GL, t: f32);
    }

    // Helper to draw a centered rectangle via scissor test.
    fn draw_center_rect(gl: &GL, rel_size: f32, r: f32, g_col: f32, b: f32) {
        let w = gl.drawing_buffer_width() as i32;
        let h = gl.drawing_buffer_height() as i32;
        let size = ((w.min(h) as f32) * rel_size) as i32;
        let x = (w - size) / 2;
        let y = (h - size) / 2;

        unsafe {
            gl.enable(GL::SCISSOR_TEST);
            gl.scissor(x, y, size, size);
            gl.clear_color(r, g_col, b, 1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);
            gl.disable(GL::SCISSOR_TEST);
        }
    }

    struct PulsingSquare;

    impl Visualizer for PulsingSquare {
        fn name(&self) -> &'static str {
            "Pulsing Square"
        }

        fn render(&mut self, gl: &GL, t: f32) {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);
            let rel = 0.2 + 0.1 * (t * 2.0).sin();
            draw_center_rect(gl, rel, 1.0, 1.0, 1.0);
        }
    }

    struct SlidingBars;

    impl Visualizer for SlidingBars {
        fn name(&self) -> &'static str {
            "Sliding Blue Bars"
        }

        fn render(&mut self, gl: &GL, t: f32) {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);

            let w = gl.drawing_buffer_width() as i32;
            let h = gl.drawing_buffer_height() as i32;
            let bar_width = (w as f32 * 0.15) as i32;
            let offset = ((w as f32 + bar_width as f32) * (t.sin() * 0.5 + 0.5)) as i32 - bar_width;

            unsafe {
                gl.enable(GL::SCISSOR_TEST);
                // Left bar
                gl.scissor(offset, 0, bar_width, h);
                gl.clear_color(0.0, 0.5, 1.0, 1.0);
                gl.clear(GL::COLOR_BUFFER_BIT);
                // Right bar (mirror)
                gl.scissor(w - offset - bar_width, 0, bar_width, h);
                gl.clear(GL::COLOR_BUFFER_BIT);
                gl.disable(GL::SCISSOR_TEST);
            }
        }
    }

    struct RedGreenStrobe;

    impl Visualizer for RedGreenStrobe {
        fn name(&self) -> &'static str {
            "Red / Green Strobe"
        }

        fn render(&mut self, gl: &GL, t: f32) {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);
            let color_switch = (t * 5.0).floor() as i32 % 2 == 0; // switch 5 Hz
            let (r, g_col) = if color_switch { (1.0, 0.0) } else { (0.0, 1.0) };
            draw_center_rect(gl, 0.4, r, g_col, 0.0);
        }
    }

    struct ExpandingCross;

    impl Visualizer for ExpandingCross {
        fn name(&self) -> &'static str {
            "Expanding Cross"
        }

        fn render(&mut self, gl: &GL, t: f32) {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);

            let w = gl.drawing_buffer_width() as i32;
            let h = gl.drawing_buffer_height() as i32;
            let thickness = ((w.min(h) as f32) * (0.02 + 0.01 * (t * 3.0).sin().abs())) as i32;
            let rel_len = 0.5;
            let len = ((w.min(h) as f32) * rel_len) as i32;

            let cx = w / 2;
            let cy = h / 2;

            unsafe {
                gl.enable(GL::SCISSOR_TEST);
                // Horizontal bar
                gl.scissor(cx - len / 2, cy - thickness / 2, len, thickness);
                gl.clear_color(1.0, 1.0, 0.0, 1.0);
                gl.clear(GL::COLOR_BUFFER_BIT);
                // Vertical bar
                gl.scissor(cx - thickness / 2, cy - len / 2, thickness, len);
                gl.clear(GL::COLOR_BUFFER_BIT);
                gl.disable(GL::SCISSOR_TEST);
            }
        }
    }

    struct CenterFlash;

    impl Visualizer for CenterFlash {
        fn name(&self) -> &'static str {
            "Center Flash"
        }

        fn render(&mut self, gl: &GL, t: f32) {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);
            let bright = ((t * 2.0).sin() * 0.5 + 0.5) as f32; // 1 Hz pulse
            draw_center_rect(gl, 0.6, bright, bright, bright);
        }
    }

    let mut viz_vec: Vec<Box<dyn Visualizer>> = vec![
        Box::new(PulsingSquare),
        Box::new(SlidingBars),
        Box::new(RedGreenStrobe),
        Box::new(ExpandingCross),
        Box::new(CenterFlash),
    ];

    for v in viz_vec.iter_mut() {
        v.init(&gl);
    }

    // Wrap in Rc<RefCell> so the animation closure can own mutable access.
    let visualizers = Rc::new(RefCell::new(viz_vec));

    const DURATION_MS: f64 = 20_000.0;

    // ---------- Animation loop ----------
    // `f` holds the animation-frame closure so that we can keep calling
    // `request_animation_frame` recursively. Storing it inside an `Option`
    // allows us to create the `Closure` first and then obtain a reference to
    // it from within itself.

    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    // Timer start point
    let start_time = window().unwrap().performance().unwrap().now();

    let mut current_index: usize = usize::MAX; // force update first frame

    let visualizers_clone = visualizers.clone();
    let gl_clone = gl.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let now = window().unwrap().performance().unwrap().now();
        let elapsed_ms = now - start_time;

        let len = visualizers_clone.borrow().len();
        if len == 0 {
            return;
        }

        let idx = ((elapsed_ms / DURATION_MS) as usize) % len;

        if idx != current_index {
            current_index = idx;
            // Update overlay with name when switching
            let name = visualizers_clone.borrow()[current_index].name();
            let label = format!(
                "{}/{} {}",
                current_index + 1,
                len,
                name
            );
            let _ = super::set_overlay_text(&label);
        }

        let local_t = ((elapsed_ms % DURATION_MS) / 1000.0) as f32; // seconds within current segment

        // Render current visualizer
        visualizers_clone.borrow_mut()[current_index].render(&gl_clone, local_t);

        // schedule next frame
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
