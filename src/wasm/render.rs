
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{
    window, HtmlCanvasElement, WebGl2RenderingContext as GL, WebGlProgram, WebGlShader,
    WebGlTexture, WebGlFramebuffer
};

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
        let win = window().unwrap();
        let css_w = win.inner_width().unwrap().as_f64().unwrap();
        let css_h = win.inner_height().unwrap().as_f64().unwrap();
        let dpr = win.device_pixel_ratio();

        let px_w = (css_w * dpr).round() as u32;
        let px_h = (css_h * dpr).round() as u32;

        // Only update if a change is actually required to avoid needless work.
        if canvas.width() != px_w || canvas.height() != px_h {
            let elem: &web_sys::HtmlElement = canvas.unchecked_ref();
            let style = elem.style();
            let _ = style.set_property("width", &format!("{}px", css_w));
            let _ = style.set_property("height", &format!("{}px", css_h));

            canvas.set_width(px_w);
            canvas.set_height(px_h);
            gl.viewport(0, 0, px_w as i32, px_h as i32);
        }
    };

    // Initial sizing so the canvas fits the window immediately.
    adjust_size(&canvas, &gl);

    // Offscreen framebuffer for post-processing
    struct Post {
        prog: WebGlProgram,
        vbo: web_sys::WebGlBuffer,
        fbo_scene: WebGlFramebuffer,
        tex_scene: WebGlTexture,
        fbo_mask: WebGlFramebuffer,
        tex_mask: WebGlTexture,
        w: i32,
        h: i32,
    }

    impl Post {
        fn new(gl: &GL, w: i32, h: i32) -> Result<Self, JsValue> {
            let vsrc = r#"#version 300 es
            layout(location=0) in vec2 a_pos;
            void main(){ gl_Position = vec4(a_pos,0.0,1.0); }
            "#;
            let fsrc = r#"#version 300 es
            precision mediump float;
            out vec4 o;
            uniform sampler2D u_src;
            uniform vec2 u_resolution;
            uniform float u_time;

            // Hash/Noise helpers
            float hash(vec2 p){ return fract(sin(dot(p, vec2(127.1,311.7))) * 43758.5453123); }

            vec3 sample_src(vec2 uv){
                // subtle chromatic aberration based on distance from center
                vec2 c = uv - 0.5;
                float r = length(c);
                float ca = 0.002 * r;
                vec3 col;
                col.r = texture(u_src, uv + ca * normalize(c)).r;
                col.g = texture(u_src, uv).g;
                col.b = texture(u_src, uv - ca * normalize(c)).b;
                return col;
            }

            void main(){
                vec2 uv = gl_FragCoord.xy / u_resolution;
                vec2 center = vec2(0.5);
                vec2 p = (uv - center);

                // Accumulate displacement
                vec2 disp = vec2(0.0);

                // 1) Waves – large-scale ripple across screen
                float wave = sin(uv.y*12.0 + u_time*1.5) * 0.003;
                wave += sin((uv.x+uv.y)*10.0 - u_time*1.2) * 0.002;
                disp += vec2(wave, 0.0);

                // 2) Warp spirals – two drifting centers
                vec2 s1 = vec2(0.3+0.2*sin(u_time*0.4), 0.4+0.2*cos(u_time*0.35));
                vec2 s2 = vec2(0.7+0.2*cos(u_time*0.37), 0.6+0.2*sin(u_time*0.31));
                for(int i=0;i<2;i++){
                    vec2 c = (i==0)? s1 : s2;
                    vec2 d = uv - c;
                    float r = length(d)+1e-4;
                    float ang = 0.15 * sin(u_time*0.8 + r*25.0);
                    mat2 rot = mat2(cos(ang),-sin(ang),sin(ang),cos(ang));
                    disp += (rot * d - d) * smoothstep(0.25, 0.0, r);
                }

                // 3) Bubbles – wobbling radial in/out around moving seeds
                for(int i=0; i<3; ++i){
                    vec2 seed = vec2(hash(vec2(float(i),0.123)), hash(vec2(float(i)+2.3,4.2)));
                    seed = 0.2 + 0.6*seed + 0.05*vec2(sin(u_time*(1.0+float(i)*0.3)+float(i)), cos(u_time*(1.2+float(i)*0.17)+float(i)));
                    vec2 d = uv - seed;
                    float r = length(d);
                    float r0 = 0.18 + 0.05*sin(u_time*1.7+float(i));
                    float amp = 0.008 * sin((r-r0)*40.0 - u_time*3.0);
                    disp += normalize(d) * amp * smoothstep(r0, 0.0, r);
                }

                // Apply displacement
                vec2 suv = clamp(uv + disp, 0.0, 1.0);
                vec3 col = sample_src(suv);

                // 4) Edge flame – detect edges via Sobel on displaced UV
                vec2 px = 1.0 / u_resolution;
                float l00 = dot(texture(u_src, suv + px*vec2(-1.0,-1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l10 = dot(texture(u_src, suv + px*vec2( 0.0,-1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l20 = dot(texture(u_src, suv + px*vec2( 1.0,-1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l01 = dot(texture(u_src, suv + px*vec2(-1.0, 0.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l21 = dot(texture(u_src, suv + px*vec2( 1.0, 0.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l02 = dot(texture(u_src, suv + px*vec2(-1.0, 1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l12 = dot(texture(u_src, suv + px*vec2( 0.0, 1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l22 = dot(texture(u_src, suv + px*vec2( 1.0, 1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float gx = (l20 + 2.0*l21 + l22) - (l00 + 2.0*l01 + l02);
                float gy = (l02 + 2.0*l12 + l22) - (l00 + 2.0*l10 + l20);
                float edge = clamp(length(vec2(gx,gy))*1.5, 0.0, 1.0);
                float flicker = 0.6 + 0.4*sin(u_time*15.0 + suv.x*30.0 + suv.y*25.0);
                vec3 flame = vec3(1.0, 0.5, 0.05) * pow(edge, 0.8) * flicker;
                col = col + flame * 0.6;

                // 5) Solid stripes in low-luminance regions (background)
                float baseLum = dot(texture(u_src, uv).rgb, vec3(0.2126,0.7152,0.0722));
                float bands = floor((uv.y + 0.2*sin(u_time*0.25)) * 12.0);
                if (mod(bands, 2.0) < 1.0 && baseLum < 0.18) {
                    vec3 stripeCol = vec3(0.06, 0.06, 0.08) + 0.6*vec3(0.25+0.25*sin(u_time+bands*0.15), 0.35+0.2*sin(u_time*0.7), 0.6);
                    col = stripeCol; // solid fill region
                }

                // Vignette for cohesion
                float v = smoothstep(0.95, 0.4, length(uv-0.5));
                col *= v;

                o = vec4(col, 1.0);
            }
            "#;

            // Post fragment shader with stripes clipped by mask
            let fsrc = r#"#version 300 es
            precision mediump float;
            out vec4 o;
            uniform sampler2D u_src;
            uniform sampler2D u_mask;
            uniform vec2 u_resolution;
            uniform float u_time;
            uniform float u_stripe_theta0;
            uniform float u_stripe_theta_speed;
            uniform float u_stripe_density;
            uniform float u_stripe_thickness;
            uniform vec2  u_stripe_drift_speed;
            uniform float u_color_speed;
            // Polka dot uniforms
            uniform float u_fill_mode; // 0 = stripes, 1 = polka
            uniform float u_dot_theta0;
            uniform float u_dot_theta_speed;
            uniform vec2  u_dot_drift_speed;
            uniform float u_dot_density;       // average dots per unit
            uniform float u_dot_radius_min;    // min radius in UV units
            uniform float u_dot_radius_max;    // max radius in UV units

            vec3 sample_src(vec2 uv){
                vec2 c = uv - 0.5; float r = length(c); float ca = 0.002 * r;
                vec3 col; col.r = texture(u_src, uv + ca * normalize(c)).r; col.g = texture(u_src, uv).g; col.b = texture(u_src, uv - ca * normalize(c)).b; return col;
            }

            vec3 hsv2rgb(vec3 c){
                vec3 p = abs(fract(c.xxx + vec3(0.0, 2.0/6.0, 4.0/6.0)) * 6.0 - 3.0);
                vec3 rgb = c.z * mix(vec3(1.0), clamp(p - 1.0, 0.0, 1.0), c.y);
                return rgb;
            }

            // Hash helpers for polka jitter
            float hash11(float n) { return fract(sin(n)*43758.5453123); }
            float hash12(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
            vec2  hash22(vec2 p) { return fract(sin(vec2(dot(p,vec2(127.1,311.7)), dot(p,vec2(269.5,183.3))))*43758.5453); }

            void main(){
                vec2 res = u_resolution;
                // Compute a centered, square-normalized coordinate uv in [0,1]^2
                float side = min(res.x, res.y);
                vec2 origin = 0.5*(res - vec2(side));
                vec2 uv = (gl_FragCoord.xy - origin) / side;
                // Outside the centered square: black bars
                if (any(lessThan(uv, vec2(0.0))) || any(greaterThan(uv, vec2(1.0)))) {
                    o = vec4(0.0,0.0,0.0,1.0);
                    return;
                }
                // Aspect-correct square space where effects stay consistent across viewport sizes
                // uv is already normalized to the centered square; use it directly
                vec2 a = vec2(min(res.x, res.y)) / res; // components <= 1
                vec2 uv_sq = uv;

                // Build displacement in square space
                vec2 disp = vec2(0.0);
                float wave = sin(uv_sq.y*12.0 + u_time*1.5) * 0.003; wave += sin((uv_sq.x+uv_sq.y)*10.0 - u_time*1.2) * 0.002; disp += vec2(wave, 0.0);
                vec2 s1 = vec2(0.3+0.2*sin(u_time*0.4), 0.4+0.2*cos(u_time*0.35));
                vec2 s2 = vec2(0.7+0.2*cos(u_time*0.37), 0.6+0.2*sin(u_time*0.31));
                for(int i=0;i<2;i++){ vec2 c = (i==0)? s1 : s2; vec2 d = uv_sq - c; float r = length(d)+1e-4; float ang = 0.15 * sin(u_time*0.8 + r*25.0); mat2 rot = mat2(cos(ang),-sin(ang),sin(ang),cos(ang)); disp += (rot * d - d) * smoothstep(0.25, 0.0, r); }
                for(int i=0; i<3; ++i){ vec2 seed = vec2(fract(sin(float(i)*12.9898+78.233)*43758.5453), fract(sin(float(i)*19.123+11.73)*24634.6345)); seed = 0.2 + 0.6*seed + 0.05*vec2(sin(u_time*(1.0+float(i)*0.3)+float(i)), cos(u_time*(1.2+float(i)*0.17)+float(i))); vec2 d = uv_sq - seed; float r = length(d); float r0 = 0.18 + 0.05*sin(u_time*1.7+float(i)); float amp = 0.008 * sin((r-r0)*40.0 - u_time*3.0); disp += normalize(d) * amp * smoothstep(r0, 0.0, r); }

                // Apply displacement in square space, convert back to texture space for sampling
                vec2 suv_sq = clamp(uv_sq + disp, 0.0, 1.0);
                // Map square UVs back into the inscribed square band of the rectangular textures
                vec2 suv = (suv_sq - 0.5) / a + 0.5;

                vec3 base = sample_src(suv);
                float mask = texture(u_mask, suv).r;

                // Diagonal zebra stripes (aspect-invariant)
                float t = u_time;
                float theta = u_stripe_theta0 + u_stripe_theta_speed * t;
                mat2 R = mat2(cos(theta), -sin(theta), sin(theta), cos(theta));
                vec2 q = R * (suv_sq - 0.5) + u_stripe_drift_speed * t;
                float s = fract(q.y * u_stripe_density);
                float stripeMask = step(s, clamp(u_stripe_thickness, 0.02, 0.98));
                float hue = fract(q.x * (u_stripe_density*0.5) + t * u_color_speed);
                vec3 rainbow = hsv2rgb(vec3(hue, 0.9, 1.0));
                vec3 stripes = stripeMask * rainbow;

                // Polka dots pattern (aspect-invariant)
                float theta_d = u_dot_theta0 + u_dot_theta_speed * t;
                mat2 RD = mat2(cos(theta_d), -sin(theta_d), sin(theta_d), cos(theta_d));
                vec2 pd = RD * (suv_sq - 0.5) + u_dot_drift_speed * t + 0.5;
                // Grid cell and local coords
                float dens = max(2.0, u_dot_density);
                vec2 g = pd * dens;
                vec2 cell = floor(g);
                vec2 f = fract(g);
                // Random center jitter within cell
                vec2 j = (hash22(cell) - 0.5) * 0.8; // up to 40% of cell size
                vec2 center = 0.5 + j;
                float rmin = max(0.005, u_dot_radius_min);
                float rmax = max(rmin+0.002, u_dot_radius_max);
                float r = mix(rmin, rmax, hash12(cell+13.17));
                float d = length(f - center);
                float dotMask = step(d, r);
                float hue_d = fract((cell.x + cell.y*1.37) * 0.15 + t * u_color_speed);
                vec3 dotColor = hsv2rgb(vec3(hue_d, 0.9, 1.0));
                vec3 polka = dotMask * dotColor;

                // Pick pattern: u_fill_mode 0 -> stripes, 1 -> polka
                vec3 pattern = mix(stripes, polka, clamp(u_fill_mode, 0.0, 1.0));

                // Flaming edges from source
                vec2 px = 1.0 / u_resolution;
                float l00 = dot(texture(u_src, suv + px*vec2(-1.0,-1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l10 = dot(texture(u_src, suv + px*vec2( 0.0,-1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l20 = dot(texture(u_src, suv + px*vec2( 1.0,-1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l01 = dot(texture(u_src, suv + px*vec2(-1.0, 0.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l21 = dot(texture(u_src, suv + px*vec2( 1.0, 0.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l02 = dot(texture(u_src, suv + px*vec2(-1.0, 1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l12 = dot(texture(u_src, suv + px*vec2( 0.0, 1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float l22 = dot(texture(u_src, suv + px*vec2( 1.0, 1.0)).rgb, vec3(0.2126,0.7152,0.0722));
                float gx = (l20 + 2.0*l21 + l22) - (l00 + 2.0*l01 + l02);
                float gy = (l02 + 2.0*l12 + l22) - (l00 + 2.0*l10 + l20);
                float edge = clamp(length(vec2(gx,gy))*1.5, 0.0, 1.0);
                float flicker = 0.6 + 0.4*sin(u_time*15.0 + suv.x*30.0 + suv.y*25.0);
                vec3 flame = vec3(1.0, 0.5, 0.05) * pow(edge, 0.8) * flicker;

                vec3 col = mix(vec3(0.0), pattern, mask);
                col += flame * 0.6;
                float v = smoothstep(0.95, 0.4, length(uv_sq-0.5));
                col *= v;
                o = vec4(col, 1.0);
            }
            "#;

            let prog = link_program(gl, vsrc, fsrc)?;

            // Fullscreen large triangle VBO
            let verts: [f32; 6] = [ -1.0, -1.0, 3.0, -1.0, -1.0, 3.0 ];
            let vbo = gl.create_buffer().ok_or("vbo")?;
            gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vbo));
            unsafe {
                let fa = js_sys::Float32Array::view(&verts);
                gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &fa, GL::STATIC_DRAW);
            }

            // Create scene texture and FBO
            let tex = gl.create_texture().ok_or("tex")?;
            gl.bind_texture(GL::TEXTURE_2D, Some(&tex));
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::LINEAR as i32);
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::LINEAR as i32);
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::CLAMP_TO_EDGE as i32);
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::CLAMP_TO_EDGE as i32);
            gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                GL::TEXTURE_2D, 0, GL::RGBA as i32, w, h, 0, GL::RGBA, GL::UNSIGNED_BYTE, None
            )?;

            let fbo = gl.create_framebuffer().ok_or("fbo")?;
            gl.bind_framebuffer(GL::FRAMEBUFFER, Some(&fbo));
            gl.framebuffer_texture_2d(GL::FRAMEBUFFER, GL::COLOR_ATTACHMENT0, GL::TEXTURE_2D, Some(&tex), 0);
            gl.bind_framebuffer(GL::FRAMEBUFFER, None);

            // Mask texture and FBO
            let tex_m = gl.create_texture().ok_or("masktex")?;
            gl.bind_texture(GL::TEXTURE_2D, Some(&tex_m));
            // Use NEAREST filtering for the mask to avoid edge expansion artifacts
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MIN_FILTER, GL::NEAREST as i32);
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_MAG_FILTER, GL::NEAREST as i32);
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_S, GL::CLAMP_TO_EDGE as i32);
            gl.tex_parameteri(GL::TEXTURE_2D, GL::TEXTURE_WRAP_T, GL::CLAMP_TO_EDGE as i32);
            gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                GL::TEXTURE_2D, 0, GL::RGBA as i32, w, h, 0, GL::RGBA, GL::UNSIGNED_BYTE, None
            )?;

            let fbo_m = gl.create_framebuffer().ok_or("mfbo")?;
            gl.bind_framebuffer(GL::FRAMEBUFFER, Some(&fbo_m));
            gl.framebuffer_texture_2d(GL::FRAMEBUFFER, GL::COLOR_ATTACHMENT0, GL::TEXTURE_2D, Some(&tex_m), 0);
            gl.bind_framebuffer(GL::FRAMEBUFFER, None);

            Ok(Self { prog, vbo, fbo_scene: fbo, tex_scene: tex, fbo_mask: fbo_m, tex_mask: tex_m, w, h })
        }

        fn resize(&mut self, gl: &GL, w: i32, h: i32) -> Result<(), JsValue> {
            if self.w == w && self.h == h { return Ok(()); }
            self.w = w; self.h = h;
            gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex_scene));
            gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                GL::TEXTURE_2D, 0, GL::RGBA as i32, w, h, 0, GL::RGBA, GL::UNSIGNED_BYTE, None
            )?;
            gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex_mask));
            gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                GL::TEXTURE_2D, 0, GL::RGBA as i32, w, h, 0, GL::RGBA, GL::UNSIGNED_BYTE, None
            )?;
            Ok(())
        }

        fn begin_scene(&self, gl: &GL) {
            gl.bind_framebuffer(GL::FRAMEBUFFER, Some(&self.fbo_scene));
            gl.viewport(0, 0, self.w, self.h);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);
        }

        fn begin_mask(&self, gl: &GL) {
            gl.bind_framebuffer(GL::FRAMEBUFFER, Some(&self.fbo_mask));
            gl.viewport(0, 0, self.w, self.h);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);
        }

        fn draw(&self, gl: &GL, time: f32, sp: &PatternParams) {
            // Post-process pass: default framebuffer
            gl.bind_framebuffer(GL::FRAMEBUFFER, None);
            gl.viewport(0, 0, self.w, self.h);
            gl.use_program(Some(&self.prog));

            // uniforms
            let loc_res = gl.get_uniform_location(&self.prog, "u_resolution");
            gl.uniform2f(loc_res.as_ref(), self.w as f32, self.h as f32);
            let loc_time = gl.get_uniform_location(&self.prog, "u_time");
            gl.uniform1f(loc_time.as_ref(), time);
            // stripe params
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_stripe_theta0").as_ref(), sp.theta0);
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_stripe_theta_speed").as_ref(), sp.theta_speed);
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_stripe_density").as_ref(), sp.density);
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_stripe_thickness").as_ref(), sp.thickness);
            gl.uniform2f(gl.get_uniform_location(&self.prog, "u_stripe_drift_speed").as_ref(), sp.drift_x, sp.drift_y);
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_color_speed").as_ref(), sp.color_speed);
            // polka
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_fill_mode").as_ref(), if sp.mode_polka { 1.0 } else { 0.0 });
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_dot_theta0").as_ref(), sp.dot_theta0);
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_dot_theta_speed").as_ref(), sp.dot_theta_speed);
            gl.uniform2f(gl.get_uniform_location(&self.prog, "u_dot_drift_speed").as_ref(), sp.dot_drift_x, sp.dot_drift_y);
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_dot_density").as_ref(), sp.dot_density);
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_dot_radius_min").as_ref(), sp.dot_rmin);
            gl.uniform1f(gl.get_uniform_location(&self.prog, "u_dot_radius_max").as_ref(), sp.dot_rmax);
            let loc_src = gl.get_uniform_location(&self.prog, "u_src");
            gl.active_texture(GL::TEXTURE0);
            gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex_scene));
            gl.uniform1i(loc_src.as_ref(), 0);
            let loc_mask = gl.get_uniform_location(&self.prog, "u_mask");
            gl.active_texture(GL::TEXTURE1);
            gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex_mask));
            gl.uniform1i(loc_mask.as_ref(), 1);

            // geometry
            gl.bind_buffer(GL::ARRAY_BUFFER, Some(&self.vbo));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_with_i32(0, 2, GL::FLOAT, false, 0, 0);
            gl.draw_arrays(GL::TRIANGLES, 0, 3);
            gl.disable_vertex_attrib_array(0);
        }
    }

    // (moved) Resize handling is set up after post-process initialization

    // ---------- Visualization framework ----------

    trait Visualizer {
        fn name(&self) -> &'static str;
        fn init(&mut self, _gl: &GL) {}
        fn render_mask(&mut self, gl: &GL, t: f32);
        fn render_color(&mut self, gl: &GL, t: f32);
    }

    // ---------- WebGL helpers ----------
    fn compile_shader(gl: &GL, src: &str, shader_type: u32) -> Result<WebGlShader, JsValue> {
        let shader = gl
            .create_shader(shader_type)
            .ok_or("could not create shader")?;
        gl.shader_source(&shader, src);
        gl.compile_shader(&shader);
        if !gl
            .get_shader_parameter(&shader, GL::COMPILE_STATUS)
            .as_bool()
            .unwrap_or(false)
        {
            return Err(JsValue::from(gl.get_shader_info_log(&shader).unwrap_or_default()));
        }
        Ok(shader)
    }

    fn link_program(gl: &GL, vert_src: &str, frag_src: &str) -> Result<WebGlProgram, JsValue> {
        let vert = compile_shader(gl, vert_src, GL::VERTEX_SHADER)?;
        let frag = compile_shader(gl, frag_src, GL::FRAGMENT_SHADER)?;
        let prog = gl.create_program().ok_or("could not create program")?;
        gl.attach_shader(&prog, &vert);
        gl.attach_shader(&prog, &frag);
        gl.link_program(&prog);
        if !gl
            .get_program_parameter(&prog, GL::LINK_STATUS)
            .as_bool()
            .unwrap_or(false)
        {
            return Err(JsValue::from(
                gl.get_program_info_log(&prog).unwrap_or_default(),
            ));
        }
        Ok(prog)
    }

    // Basic circle line geometry prepared once and shared.
    const SEGMENTS: usize = 128;

    // Fullscreen vertex shader used by SDF-based visualizers
    const VERT_FS: &str = r#"#version 300 es
    layout(location=0) in vec2 a_pos;
    void main(){ gl_Position = vec4(a_pos, 0.0, 1.0); }
    "#;

    // ---------- New Line-based Visualizers ----------

    struct PulseCircle { prog_color: Option<WebGlProgram>, prog_mask: Option<WebGlProgram>, vbo: Option<web_sys::WebGlBuffer> }
    impl Default for PulseCircle { fn default() -> Self { Self { prog_color: None, prog_mask: None, vbo: None } } }
    impl Visualizer for PulseCircle {
        fn name(&self) -> &'static str { "Pulsing Circle" }
        fn init(&mut self, gl: &GL) {
            let frag_common = r#"
                precision mediump float;
                uniform vec2 u_resolution; uniform float u_time; uniform float u_scale; uniform float u_rot; out vec4 o;
                float sdCircle(vec2 p, float r){ return length(p)-r; }
                vec2 toP(vec2 uv){ vec2 res=u_resolution; vec2 a=vec2(min(res.x,res.y))/res; vec2 p=(uv*2.0-1.0)*a*u_scale; float c=cos(u_rot), s=sin(u_rot); return mat2(c,-s,s,c)*p; }
            "#;
            let frag_color = format!("#version 300 es\n{}\nvoid main(){{ vec2 uv=gl_FragCoord.xy/u_resolution; vec2 p=toP(uv); float d=sdCircle(p,0.7); float a=smoothstep(0.0,-0.005,d); float clip=1.0 - smoothstep(0.85, 1.0, length(p)); a*=clip; float bright=0.5+0.5*sin(u_time); o=vec4(vec3(bright), a); }}", frag_common);
            let frag_mask = format!("#version 300 es\n{}\nvoid main(){{ vec2 uv=gl_FragCoord.xy/u_resolution; vec2 p=toP(uv); float d=sdCircle(p,0.7); float a=step(d,0.0); float clip=1.0 - smoothstep(0.85, 1.0, length(p)); a*=clip; o=vec4(a,a,a,1.0); }}", frag_common);
            self.prog_color = Some(link_program(gl, VERT_FS, &frag_color).unwrap());
            self.prog_mask = Some(link_program(gl, VERT_FS, &frag_mask).unwrap());
            // FS triangle
            let verts: [f32; 6] = [ -1.0, -1.0, 3.0, -1.0, -1.0, 3.0 ];
            let vbo = gl.create_buffer().unwrap(); gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vbo)); unsafe{let fa=js_sys::Float32Array::view(&verts); gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER,&fa,GL::STATIC_DRAW);} self.vbo=Some(vbo);
        }
        fn render_mask(&mut self, gl: &GL, t: f32){
            let prog=self.prog_mask.as_ref().unwrap(); gl.use_program(Some(prog));
            let (w,h)=(gl.drawing_buffer_width() as f32, gl.drawing_buffer_height() as f32);
            gl.uniform2f(gl.get_uniform_location(prog,"u_resolution").as_ref(), w,h);
            gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(), t);
            gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(), 1.0);
            gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(), 0.0);
            gl.bind_buffer(GL::ARRAY_BUFFER,self.vbo.as_ref()); gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0); gl.draw_arrays(GL::TRIANGLES,0,3); gl.disable_vertex_attrib_array(0);
        }
        fn render_color(&mut self, gl: &GL, t: f32){
            let prog=self.prog_color.as_ref().unwrap(); gl.use_program(Some(prog));
            let (w,h)=(gl.drawing_buffer_width() as f32, gl.drawing_buffer_height() as f32);
            gl.uniform2f(gl.get_uniform_location(prog,"u_resolution").as_ref(), w,h);
            gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(), t);
            gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(), 1.0);
            gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(), 0.0);
            gl.bind_buffer(GL::ARRAY_BUFFER,self.vbo.as_ref()); gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0); gl.draw_arrays(GL::TRIANGLES,0,3); gl.disable_vertex_attrib_array(0);
        }
    }

    struct RotatingSquare { prog_color: Option<WebGlProgram>, prog_mask: Option<WebGlProgram>, vbo: Option<web_sys::WebGlBuffer> }
    impl Default for RotatingSquare { fn default() -> Self { Self { prog_color: None, prog_mask: None, vbo: None } } }
    impl Visualizer for RotatingSquare {
        fn name(&self) -> &'static str { "Rotating Square" }
        fn init(&mut self, gl: &GL) {
            let frag_common = r#"
                precision mediump float;
                uniform vec2 u_resolution; uniform float u_time; uniform float u_scale; uniform float u_rot; out vec4 o;
                float sdBox(vec2 p, vec2 b){ vec2 d=abs(p)-b; return length(max(d,0.0))+min(max(d.x,d.y),0.0); }
                vec2 toP(vec2 uv){ vec2 res=u_resolution; vec2 a=vec2(min(res.x,res.y))/res; vec2 p=(uv*2.0-1.0)*a*u_scale; float c=cos(u_rot), s=sin(u_rot); return mat2(c,-s,s,c)*p; }
            "#;
            let frag_color = format!("#version 300 es\n{}\nvoid main(){{ vec2 uv=gl_FragCoord.xy/u_resolution; vec2 p=toP(uv); float d=sdBox(p, vec2(0.6)); float a=smoothstep(0.0,-0.005,d); float clip=1.0 - smoothstep(0.85, 1.0, length(p)); a*=clip; o=vec4(1.0,0.3,0.0,a); }}", frag_common);
            let frag_mask = format!("#version 300 es\n{}\nvoid main(){{ vec2 uv=gl_FragCoord.xy/u_resolution; vec2 p=toP(uv); float d=sdBox(p, vec2(0.6)); float a=step(d,0.0); float clip=1.0 - smoothstep(0.85, 1.0, length(p)); a*=clip; o=vec4(a,a,a,1.0); }}", frag_common);
            self.prog_color = Some(link_program(gl, VERT_FS, &frag_color).unwrap());
            self.prog_mask = Some(link_program(gl, VERT_FS, &frag_mask).unwrap());
            let verts:[f32;6]=[-1.0,-1.0,3.0,-1.0,-1.0,3.0]; let vbo=gl.create_buffer().unwrap(); gl.bind_buffer(GL::ARRAY_BUFFER,Some(&vbo)); unsafe{let fa=js_sys::Float32Array::view(&verts); gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER,&fa,GL::STATIC_DRAW);} self.vbo=Some(vbo);
        }
        fn render_mask(&mut self, gl:&GL, t:f32){ let prog=self.prog_mask.as_ref().unwrap(); gl.use_program(Some(prog)); let (w,h)=(gl.drawing_buffer_width() as f32, gl.drawing_buffer_height() as f32); gl.uniform2f(gl.get_uniform_location(prog,"u_resolution").as_ref(),w,h); gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(),t); gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),1.0); gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(), t); gl.bind_buffer(GL::ARRAY_BUFFER,self.vbo.as_ref()); gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0); gl.draw_arrays(GL::TRIANGLES,0,3); gl.disable_vertex_attrib_array(0); }
        fn render_color(&mut self, gl:&GL, t:f32){ let prog=self.prog_color.as_ref().unwrap(); gl.use_program(Some(prog)); let (w,h)=(gl.drawing_buffer_width() as f32, gl.drawing_buffer_height() as f32); gl.uniform2f(gl.get_uniform_location(prog,"u_resolution").as_ref(),w,h); gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(),t); gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),1.0); gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(), t); gl.bind_buffer(GL::ARRAY_BUFFER,self.vbo.as_ref()); gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0); gl.draw_arrays(GL::TRIANGLES,0,3); gl.disable_vertex_attrib_array(0); }
    }

    struct StarLines { prog_color: Option<WebGlProgram>, prog_mask: Option<WebGlProgram>, vbo: Option<web_sys::WebGlBuffer> }
    impl Default for StarLines { fn default()->Self{ Self{ prog_color:None, prog_mask:None, vbo:None } } }
    impl Visualizer for StarLines {
        fn name(&self)-> &'static str { "Twinkling Star" }
        fn init(&mut self, gl:&GL){
            let frag_common = r#"
                precision mediump float; out vec4 o;
                uniform vec2 u_resolution; uniform float u_time; uniform float u_scale; uniform float u_rot;
                vec2 toP(vec2 uv){ vec2 res=u_resolution; vec2 a=vec2(min(res.x,res.y))/res; vec2 p=(uv*2.0-1.0)*a*u_scale; float c=cos(u_rot),s=sin(u_rot); return mat2(c,-s,s,c)*p; }
            "#;
            // star via angular radius modulation
            let frag_color = format!("#version 300 es\n{}\nvoid main(){{ vec2 uv=gl_FragCoord.xy/u_resolution; vec2 p=toP(uv); float th=atan(p.y,p.x); float r=length(p); float k=5.0; float r1=0.75, r2=0.35; float rr = mix(r1, r2, 0.5+0.5*cos(th*k)); float a = smoothstep(rr, rr-0.01, r); float clip=1.0 - smoothstep(0.85, 1.0, r); a*=clip; float blink=abs(sin(u_time*5.0)); vec3 col=vec3(1.0, blink, 0.0); o=vec4(col, a); }}", frag_common);
            let frag_mask = format!("#version 300 es\n{}\nvoid main(){{ vec2 uv=gl_FragCoord.xy/u_resolution; vec2 p=toP(uv); float th=atan(p.y,p.x); float r=length(p); float k=5.0; float r1=0.75, r2=0.35; float rr = mix(r1, r2, 0.5+0.5*cos(th*k)); float a = step(r, rr); float clip=1.0 - smoothstep(0.85, 1.0, r); a*=clip; o=vec4(a,a,a,1.0); }}", frag_common);
            self.prog_color=Some(link_program(gl, VERT_FS, &frag_color).unwrap());
            self.prog_mask=Some(link_program(gl, VERT_FS, &frag_mask).unwrap());
            let verts:[f32;6]=[-1.0,-1.0,3.0,-1.0,-1.0,3.0]; let vbo=gl.create_buffer().unwrap(); gl.bind_buffer(GL::ARRAY_BUFFER,Some(&vbo)); unsafe{let fa=js_sys::Float32Array::view(&verts); gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER,&fa,GL::STATIC_DRAW);} self.vbo=Some(vbo);
        }
        fn render_mask(&mut self, gl:&GL,t:f32){ let prog=self.prog_mask.as_ref().unwrap(); gl.use_program(Some(prog)); let (w,h)=(gl.drawing_buffer_width() as f32, gl.drawing_buffer_height() as f32); gl.uniform2f(gl.get_uniform_location(prog,"u_resolution").as_ref(),w,h); gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(),t); gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),1.0); gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(), t*0.5); gl.bind_buffer(GL::ARRAY_BUFFER,self.vbo.as_ref()); gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0); gl.draw_arrays(GL::TRIANGLES,0,3); gl.disable_vertex_attrib_array(0);}        
        fn render_color(&mut self, gl:&GL,t:f32){ let prog=self.prog_color.as_ref().unwrap(); gl.use_program(Some(prog)); let (w,h)=(gl.drawing_buffer_width() as f32, gl.drawing_buffer_height() as f32); gl.uniform2f(gl.get_uniform_location(prog,"u_resolution").as_ref(),w,h); gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(),t); gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),1.0); gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(), t*0.5); gl.bind_buffer(GL::ARRAY_BUFFER,self.vbo.as_ref()); gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0); gl.draw_arrays(GL::TRIANGLES,0,3); gl.disable_vertex_attrib_array(0);}        
    }

    struct RadiatingSpokes { prog_color: Option<WebGlProgram>, prog_mask: Option<WebGlProgram>, vbo: Option<web_sys::WebGlBuffer> }
    impl Default for RadiatingSpokes { fn default()->Self{Self{prog_color:None, prog_mask:None, vbo:None}} }
    impl Visualizer for RadiatingSpokes {
        fn name(&self)-> &'static str { "Radiating Spokes" }
        fn init(&mut self, gl:&GL){
            let frag_common = r#"
                precision mediump float; out vec4 o;
                uniform vec2 u_resolution; uniform float u_time; uniform float u_scale; uniform float u_rot;
                vec2 toP(vec2 uv){ vec2 res=u_resolution; vec2 a=vec2(min(res.x,res.y))/res; vec2 p=(uv*2.0-1.0)*a*u_scale; float c=cos(u_rot),s=sin(u_rot); return mat2(c,-s,s,c)*p; }
            "#;
            let frag_color = format!("#version 300 es\n{}\nvoid main(){{ vec2 uv=gl_FragCoord.xy/u_resolution; vec2 p=toP(uv); float th=atan(p.y,p.x); float r=length(p); float n=18.0; float w=0.12; float band = abs(sin(th*n + u_time*0.6)); float m = smoothstep(w,w-0.01,band) * smoothstep(0.9,0.2,r); float clip=1.0 - smoothstep(0.85, 1.0, r); m*=clip; o=vec4(0.0,0.8,1.0,m); }}", frag_common);
            let frag_mask = format!("#version 300 es\n{}\nvoid main(){{ vec2 uv=gl_FragCoord.xy/u_resolution; vec2 p=toP(uv); float th=atan(p.y,p.x); float r=length(p); float n=18.0; float w=0.12; float band = abs(sin(th*n + u_time*0.6)); float a = step(band,w) * step(r,0.95); float clip=1.0 - smoothstep(0.85, 1.0, r); a*=clip; o=vec4(a,a,a,1.0); }}", frag_common);
            self.prog_color=Some(link_program(gl, VERT_FS, &frag_color).unwrap());
            self.prog_mask=Some(link_program(gl, VERT_FS, &frag_mask).unwrap());
            let verts:[f32;6]=[-1.0,-1.0,3.0,-1.0,-1.0,3.0]; let vbo=gl.create_buffer().unwrap(); gl.bind_buffer(GL::ARRAY_BUFFER,Some(&vbo)); unsafe{let fa=js_sys::Float32Array::view(&verts); gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER,&fa,GL::STATIC_DRAW);} self.vbo=Some(vbo);
        }
        fn render_mask(&mut self, gl:&GL,t:f32){ let prog=self.prog_mask.as_ref().unwrap(); gl.use_program(Some(prog)); let (w,h)=(gl.drawing_buffer_width() as f32, gl.drawing_buffer_height() as f32); gl.uniform2f(gl.get_uniform_location(prog,"u_resolution").as_ref(),w,h); gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(),t); gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),1.0); gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(), 0.0); gl.bind_buffer(GL::ARRAY_BUFFER,self.vbo.as_ref()); gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0); gl.draw_arrays(GL::TRIANGLES,0,3); gl.disable_vertex_attrib_array(0); }
        fn render_color(&mut self, gl:&GL,t:f32){ let prog=self.prog_color.as_ref().unwrap(); gl.use_program(Some(prog)); let (w,h)=(gl.drawing_buffer_width() as f32, gl.drawing_buffer_height() as f32); gl.uniform2f(gl.get_uniform_location(prog,"u_resolution").as_ref(),w,h); gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(),t); gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),1.0); gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(), 0.0); gl.bind_buffer(GL::ARRAY_BUFFER,self.vbo.as_ref()); gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0); gl.draw_arrays(GL::TRIANGLES,0,3); gl.disable_vertex_attrib_array(0); }
    }

    struct ExpandingCrossLines { prog_color: Option<WebGlProgram>, prog_mask: Option<WebGlProgram>, vbo: Option<web_sys::WebGlBuffer> }
    impl Default for ExpandingCrossLines { fn default()->Self{Self{prog_color:None, prog_mask:None, vbo:None}} }
    impl Visualizer for ExpandingCrossLines {
        fn name(&self)-> &'static str { "Pulsing Plus" }
        fn init(&mut self, gl:&GL){
            let frag_common = r#"
                precision mediump float; out vec4 o;
                uniform vec2 u_resolution; uniform float u_time; uniform float u_scale; uniform float u_rot;
                vec2 toP(vec2 uv){ vec2 res=u_resolution; vec2 a=vec2(min(res.x,res.y))/res; vec2 p=(uv*2.0-1.0)*a*u_scale; float c=cos(u_rot),s=sin(u_rot); return mat2(c,-s,s,c)*p; }
                float sdBox(vec2 p, vec2 b){ vec2 d=abs(p)-b; return length(max(d,0.0))+min(max(d.x,d.y),0.0); }
            "#;
            let frag_color = format!("#version 300 es\n{}\nvoid main(){{ vec2 uv=gl_FragCoord.xy/u_resolution; vec2 p=toP(uv); float th=0.25+0.1*abs(sin(u_time*2.0)); float d=min(sdBox(p, vec2(0.8, th)), sdBox(p, vec2(th, 0.8))); float a=smoothstep(0.0,-0.005,d); float clip=1.0 - smoothstep(0.85, 1.0, length(p)); a*=clip; o=vec4(1.0,1.0,0.0,a); }}", frag_common);
            let frag_mask = format!("#version 300 es\n{}\nvoid main(){{ vec2 uv=gl_FragCoord.xy/u_resolution; vec2 p=toP(uv); float th=0.25+0.1*abs(sin(u_time*2.0)); float a = step(min(sdBox(p, vec2(0.8, th)), sdBox(p, vec2(th, 0.8))), 0.0); float clip=1.0 - smoothstep(0.85, 1.0, length(p)); a*=clip; o=vec4(a,a,a,1.0); }}", frag_common);
            self.prog_color=Some(link_program(gl, VERT_FS, &frag_color).unwrap());
            self.prog_mask=Some(link_program(gl, VERT_FS, &frag_mask).unwrap());
            let verts:[f32;6]=[-1.0,-1.0,3.0,-1.0,-1.0,3.0]; let vbo=gl.create_buffer().unwrap(); gl.bind_buffer(GL::ARRAY_BUFFER,Some(&vbo)); unsafe{let fa=js_sys::Float32Array::view(&verts); gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER,&fa,GL::STATIC_DRAW);} self.vbo=Some(vbo);
        }
        fn render_mask(&mut self, gl:&GL,t:f32){ let prog=self.prog_mask.as_ref().unwrap(); gl.use_program(Some(prog)); let (w,h)=(gl.drawing_buffer_width() as f32, gl.drawing_buffer_height() as f32); gl.uniform2f(gl.get_uniform_location(prog,"u_resolution").as_ref(),w,h); gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(),t); gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),1.0); gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(),0.0); gl.bind_buffer(GL::ARRAY_BUFFER,self.vbo.as_ref()); gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0); gl.draw_arrays(GL::TRIANGLES,0,3); gl.disable_vertex_attrib_array(0);}        
        fn render_color(&mut self, gl:&GL,t:f32){ let prog=self.prog_color.as_ref().unwrap(); gl.use_program(Some(prog)); let (w,h)=(gl.drawing_buffer_width() as f32, gl.drawing_buffer_height() as f32); gl.uniform2f(gl.get_uniform_location(prog,"u_resolution").as_ref(),w,h); gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(),t); gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),1.0); gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(),0.0); gl.bind_buffer(GL::ARRAY_BUFFER,self.vbo.as_ref()); gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0); gl.draw_arrays(GL::TRIANGLES,0,3); gl.disable_vertex_attrib_array(0);}        
    }

    let mut viz_vec: Vec<Box<dyn Visualizer>> = vec![
        Box::new(PulseCircle::default()),
        Box::new(RotatingSquare::default()),
        Box::new(StarLines::default()),
        Box::new(RadiatingSpokes::default()),
        Box::new(ExpandingCrossLines::default()),
    ];

    for v in viz_vec.iter_mut() {
        v.init(&gl);
    }

    // Wrap in Rc<RefCell> so the animation closure can own mutable access.
    let visualizers = Rc::new(RefCell::new(viz_vec));

    const DURATION_MS: f64 = 20_000.0;

    // Parameters controlling fill patterns, randomized on each visualizer change
    #[derive(Clone, Copy)]
    struct PatternParams {
        // stripes
        theta0: f32, theta_speed: f32, density: f32, thickness: f32, drift_x: f32, drift_y: f32,
        // polka
        mode_polka: bool,
        dot_theta0: f32, dot_theta_speed: f32, dot_drift_x: f32, dot_drift_y: f32,
        dot_density: f32, dot_rmin: f32, dot_rmax: f32,
        // shared
        color_speed: f32,
    }
    impl Default for PatternParams {
        fn default() -> Self {
            Self {
                theta0: 0.0, theta_speed: 0.1, density: 16.0, thickness: 0.5, drift_x: 0.05, drift_y: 0.03,
                mode_polka: false,
                dot_theta0: 0.0, dot_theta_speed: 0.08, dot_drift_x: 0.03, dot_drift_y: -0.02,
                dot_density: 10.0, dot_rmin: 0.05, dot_rmax: 0.18,
                color_speed: 0.1,
            }
        }
    }
    fn frand() -> f32 { js_sys::Math::random() as f32 }
    fn randomize_params(p: &Rc<RefCell<PatternParams>>) {
        let mut s = p.borrow_mut();
        s.theta0 = frand() * std::f32::consts::PI;
        s.theta_speed = 0.05 + frand() * 0.3; // rad/s
        s.density = 8.0 + frand() * 24.0;     // lines per unit
        s.thickness = 0.15 + frand() * 0.7;   // 0..1 fraction
        s.drift_x = (frand() * 2.0 - 1.0) * 0.15; // units/s
        s.drift_y = (frand() * 2.0 - 1.0) * 0.15;
        s.color_speed = 0.05 + frand() * 0.4; // hue cycles/s
        // switch mode randomly
        s.mode_polka = frand() > 0.5;
        // polka params
        s.dot_theta0 = frand() * std::f32::consts::TAU;
        s.dot_theta_speed = 0.02 + frand() * 0.2;
        s.dot_drift_x = (frand()*2.0 - 1.0) * 0.2;
        s.dot_drift_y = (frand()*2.0 - 1.0) * 0.2;
        s.dot_density = 6.0 + frand() * 20.0;
        let rmin = 0.03 + frand() * 0.12;
        let rmax = rmin + 0.03 + frand() * 0.2;
        s.dot_rmin = rmin; s.dot_rmax = rmax;
    }

    let stripe_params = Rc::new(RefCell::new(PatternParams::default()));

    // ---------- Animation loop ----------
    // `f` holds the animation-frame closure so that we can keep calling
    // `request_animation_frame` recursively. Storing it inside an `Option`
    // allows us to create the `Closure` first and then obtain a reference to
    // it from within itself.

    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    let start_time = window().unwrap().performance().unwrap().now();
    let current_index: Rc<RefCell<usize>> = Rc::new(RefCell::new(usize::MAX)); // force first update
    let segment_start_ms: Rc<RefCell<f64>> = Rc::new(RefCell::new(start_time));

    let visualizers_clone = visualizers.clone();
    let gl_clone = gl.clone();

    // Initialize post-process pipeline
    let post = Rc::new(RefCell::new(Post::new(
        &gl_clone,
        gl_clone.drawing_buffer_width() as i32,
        gl_clone.drawing_buffer_height() as i32,
    )?));

    // Resize: adjust canvas and the offscreen texture size
    {
        let canvas = canvas.clone();
        let gl = gl.clone();
        let post_rc = post.clone();
        let resize_closure = Closure::wrap(Box::new(move || {
            adjust_size(&canvas, &gl);
            let w = gl.drawing_buffer_width() as i32;
            let h = gl.drawing_buffer_height() as i32;
            let _ = post_rc.borrow_mut().resize(&gl, w, h);
        }) as Box<dyn FnMut()>);
        window()
            .unwrap()
            .add_event_listener_with_callback("resize", resize_closure.as_ref().unchecked_ref())?;
        resize_closure.forget();
    }

    {
        let visualizers_k = visualizers.clone();
        let current_index_k = current_index.clone();
        let segment_start_k = segment_start_ms.clone();
        let stripe_params_k = stripe_params.clone();
        let keydown = Closure::wrap(Box::new(move |ev: web_sys::KeyboardEvent| {
            let key = ev.key();
            let code = ev.code();
            if key == " " || code == "Space" {
                ev.prevent_default();
                if let Some(t) = ev.target() {
                    if let Some(el) = t.dyn_ref::<web_sys::Element>() {
                        let tag = el.tag_name();
                        if tag == "INPUT" || tag == "TEXTAREA" || el.get_attribute("contenteditable").is_some() {
                            return;
                        }
                    }
                }
                let len = visualizers_k.borrow().len();
                if len > 0 {
                    let mut idx = current_index_k.borrow_mut();
                    let next = if *idx == usize::MAX { 0 } else { (*idx + 1) % len };
                    *idx = next;
                    *segment_start_k.borrow_mut() = window().unwrap().performance().unwrap().now();
                    randomize_params(&stripe_params_k);
                    let name = visualizers_k.borrow()[*idx].name();
                    let label = format!("{}/{} {}", *idx + 1, len, name);
                    let _ = super::set_overlay_text(&label);
                }
            }
        }) as Box<dyn FnMut(_)>);
        window().unwrap().add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())?;
        keydown.forget();
    }

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let now = window().unwrap().performance().unwrap().now();
        let len = visualizers_clone.borrow().len();
        if len == 0 {
            return;
        }

        if *current_index.borrow() == usize::MAX {
            *current_index.borrow_mut() = 0;
            *segment_start_ms.borrow_mut() = now;
            let name = visualizers_clone.borrow()[0].name();
            let label = format!("{}/{} {}", 1, len, name);
            let _ = super::set_overlay_text(&label);
            randomize_params(&stripe_params);
        }
        let elapsed_in_segment = now - *segment_start_ms.borrow();
        if elapsed_in_segment >= DURATION_MS {
            let mut idx_ref = current_index.borrow_mut();
            *idx_ref = (*idx_ref + 1) % len;
            *segment_start_ms.borrow_mut() = now;
            let name = visualizers_clone.borrow()[*idx_ref].name();
            let label = format!("{}/{} {}", *idx_ref + 1, len, name);
            let _ = super::set_overlay_text(&label);
            randomize_params(&stripe_params);
        }
        let local_t = ((now - *segment_start_ms.borrow()) / 1000.0) as f32;
        let idx_now = *current_index.borrow();

        // Render mask then scene into offscreen targets, then apply post-process to screen
        post.borrow().begin_mask(&gl_clone);
        visualizers_clone.borrow_mut()[idx_now].render_mask(&gl_clone, local_t);
        post.borrow().begin_scene(&gl_clone);
        visualizers_clone.borrow_mut()[idx_now].render_color(&gl_clone, local_t);
        let sp = *stripe_params.borrow();
        post.borrow().draw(&gl_clone, (now as f32) / 1000.0, &sp);

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
