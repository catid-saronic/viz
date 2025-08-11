
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

    // Offscreen framebuffer for post-processing
    struct Post {
        prog: WebGlProgram,
        vbo: web_sys::WebGlBuffer,
        fbo: WebGlFramebuffer,
        tex: WebGlTexture,
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

            let prog = link_program(gl, vsrc, fsrc)?;

            // Fullscreen large triangle VBO
            let verts: [f32; 6] = [ -1.0, -1.0, 3.0, -1.0, -1.0, 3.0 ];
            let vbo = gl.create_buffer().ok_or("vbo")?;
            gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vbo));
            unsafe {
                let fa = js_sys::Float32Array::view(&verts);
                gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &fa, GL::STATIC_DRAW);
            }

            // Create texture and FBO
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

            Ok(Self { prog, vbo, fbo, tex, w, h })
        }

        fn resize(&mut self, gl: &GL, w: i32, h: i32) -> Result<(), JsValue> {
            if self.w == w && self.h == h { return Ok(()); }
            self.w = w; self.h = h;
            gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex));
            gl.tex_image_2d_with_i32_and_i32_and_i32_and_format_and_type_and_opt_u8_array(
                GL::TEXTURE_2D, 0, GL::RGBA as i32, w, h, 0, GL::RGBA, GL::UNSIGNED_BYTE, None
            )?;
            Ok(())
        }

        fn begin(&self, gl: &GL) {
            gl.bind_framebuffer(GL::FRAMEBUFFER, Some(&self.fbo));
            gl.viewport(0, 0, self.w, self.h);
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);
        }

        fn draw(&self, gl: &GL, time: f32) {
            // Post-process pass: default framebuffer
            gl.bind_framebuffer(GL::FRAMEBUFFER, None);
            gl.viewport(0, 0, self.w, self.h);
            gl.use_program(Some(&self.prog));

            // uniforms
            let loc_res = gl.get_uniform_location(&self.prog, "u_resolution");
            gl.uniform2f(loc_res.as_ref(), self.w as f32, self.h as f32);
            let loc_time = gl.get_uniform_location(&self.prog, "u_time");
            gl.uniform1f(loc_time.as_ref(), time);
            let loc_src = gl.get_uniform_location(&self.prog, "u_src");
            gl.active_texture(GL::TEXTURE0);
            gl.bind_texture(GL::TEXTURE_2D, Some(&self.tex));
            gl.uniform1i(loc_src.as_ref(), 0);

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
        fn render(&mut self, gl: &GL, t: f32);
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

    // Vertex shader shared among visualizers – scales & rotates positions, outputs line width by default 1px.
    const VERT_SRC: &str = r#"#version 300 es
    precision mediump float;
    layout(location = 0) in vec2 a_pos;
    uniform float u_scale;
    uniform float u_rot;
    uniform vec2 u_aspect; // aspect-correct scale so shapes are not squished
    void main() {
        float c = cos(u_rot);
        float s = sin(u_rot);
        vec2 p = vec2(c * a_pos.x - s * a_pos.y, s * a_pos.x + c * a_pos.y);
        p *= u_scale;
        p *= u_aspect; // maintain consistent appearance across aspect ratios
        gl_Position = vec4(p, 0.0, 1.0);
    }
    "#;

    // ---------- New Line-based Visualizers ----------

    struct PulseCircle {
        prog: Option<WebGlProgram>,
    }

    impl Default for PulseCircle {
        fn default() -> Self {
            Self { prog: None }
        }
    }

    impl Visualizer for PulseCircle {
        fn name(&self) -> &'static str {
            "Pulsing Circle"
        }

        fn init(&mut self, gl: &GL) {
            let frag_src = r#"#version 300 es
            precision mediump float;
            uniform float u_time;
            out vec4 o;
            void main() {
                float bright = 0.5 + 0.5 * sin(u_time);
                o = vec4(vec3(bright), 1.0);
            }"#;
            self.prog = Some(link_program(gl, VERT_SRC, frag_src).unwrap());
        }

        fn render(&mut self, gl: &GL, t: f32) {
            gl.clear_color(0.0, 0.0, 0.0, 1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);

            let prog = self.prog.as_ref().unwrap();
            gl.use_program(Some(prog));

            let loc_scale = gl.get_uniform_location(prog, "u_scale");
            gl.uniform1f(loc_scale.as_ref(), 0.8);
            let loc_rot = gl.get_uniform_location(prog, "u_rot");
            gl.uniform1f(loc_rot.as_ref(), 0.0);
            // aspect correction based on drawing buffer size
            let w = gl.drawing_buffer_width() as f32;
            let h = gl.drawing_buffer_height() as f32;
            let (sx, sy) = if w >= h { (h / w, 1.0) } else { (1.0, w / h) };
            let loc_aspect = gl.get_uniform_location(prog, "u_aspect");
            gl.uniform2f(loc_aspect.as_ref(), sx, sy);
            let loc_time = gl.get_uniform_location(prog, "u_time");
            gl.uniform1f(loc_time.as_ref(), t);

            // build circle vbo on the fly
            let mut verts: Vec<f32> = Vec::with_capacity(SEGMENTS * 2);
            for i in 0..SEGMENTS {
                let th = (i as f32 / SEGMENTS as f32) * std::f32::consts::PI * 2.0;
                verts.push(th.cos());
                verts.push(th.sin());
            }
            let vbo = gl.create_buffer().unwrap();
            gl.bind_buffer(GL::ARRAY_BUFFER, Some(&vbo));
            unsafe {
                let fa = js_sys::Float32Array::view(&verts);
                gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER, &fa, GL::STATIC_DRAW);
            }
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_with_i32(0, 2, GL::FLOAT, false, 0, 0);
            gl.draw_arrays(GL::LINE_LOOP, 0, SEGMENTS as i32);
            gl.disable_vertex_attrib_array(0);
        }
    }

    struct RotatingSquare { prog: Option<WebGlProgram> }

    impl Default for RotatingSquare { fn default() -> Self { Self { prog: None } } }

    impl Visualizer for RotatingSquare {
        fn name(&self) -> &'static str { "Rotating Square" }
        fn init(&mut self, gl: &GL) {
            let frag_src = r#"#version 300 es
            precision mediump float;
            out vec4 o; void main(){ o=vec4(1.0,0.3,0.0,1.0);}"#;
            self.prog = Some(link_program(gl, VERT_SRC, frag_src).unwrap());
        }
        fn render(&mut self, gl: &GL, t: f32) {
            gl.clear_color(0.0,0.0,0.0,1.0);
            gl.clear(GL::COLOR_BUFFER_BIT);
            let prog=self.prog.as_ref().unwrap();
            gl.use_program(Some(prog));
            let scale=gl.get_uniform_location(prog,"u_scale");
            gl.uniform1f(scale.as_ref(),0.6);
            let rot=gl.get_uniform_location(prog,"u_rot");
            gl.uniform1f(rot.as_ref(),t);
            let w = gl.drawing_buffer_width() as f32; let h = gl.drawing_buffer_height() as f32;
            let (sx, sy) = if w >= h { (h / w, 1.0) } else { (1.0, w / h) };
            let loc_aspect = gl.get_uniform_location(prog, "u_aspect");
            gl.uniform2f(loc_aspect.as_ref(), sx, sy);

            // square vertices
            const SQ: [f32;8] = [ -1.0,-1.0, 1.0,-1.0, 1.0,1.0, -1.0,1.0 ];
            let vbo=gl.create_buffer().unwrap();
            gl.bind_buffer(GL::ARRAY_BUFFER,Some(&vbo));
            unsafe{ let fa=js_sys::Float32Array::view(&SQ); gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER,&fa,GL::STATIC_DRAW);}            
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0);
            gl.draw_arrays(GL::LINE_LOOP,0,4);
            gl.disable_vertex_attrib_array(0);
        }
    }

    struct StarLines { prog: Option<WebGlProgram> }
    impl Default for StarLines { fn default() -> Self { Self{prog:None} } }
    impl Visualizer for StarLines {
        fn name(&self)-> &'static str { "Twinkling Star" }
        fn init(&mut self, gl:&GL){
            let frag_src=r#"#version 300 es
            precision mediump float;
            uniform float u_time; out vec4 o; void main(){ float blink=abs(sin(u_time*5.0)); o=vec4(1.0,1.0*blink,0.0,1.0);}"#;
            self.prog=Some(link_program(gl,VERT_SRC,frag_src).unwrap());
        }
        fn render(&mut self, gl:&GL,t:f32){
            gl.clear_color(0.0,0.0,0.0,1.0); gl.clear(GL::COLOR_BUFFER_BIT);
            let prog=self.prog.as_ref().unwrap(); gl.use_program(Some(prog));
            gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),0.7);
            gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(),t*0.5);
            gl.uniform1f(gl.get_uniform_location(prog,"u_time").as_ref(),t);
            let w = gl.drawing_buffer_width() as f32; let h = gl.drawing_buffer_height() as f32;
            let (sx, sy) = if w >= h { (h / w, 1.0) } else { (1.0, w / h) };
            let loc_aspect = gl.get_uniform_location(prog, "u_aspect");
            gl.uniform2f(loc_aspect.as_ref(), sx, sy);

            // star geometry 5-point lines
            const V:[f32;10]=[0.0,1.0, -0.5878,-0.809, 0.9511,0.309, -0.9511,0.309, 0.5878,-0.809];
            let vbo=gl.create_buffer().unwrap(); gl.bind_buffer(GL::ARRAY_BUFFER,Some(&vbo)); unsafe{let fa=js_sys::Float32Array::view(&V); gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER,&fa,GL::STATIC_DRAW);}            
            gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0);
            gl.draw_arrays(GL::LINE_LOOP,0,5);
            gl.disable_vertex_attrib_array(0);
        }
    }

    struct RadiatingSpokes { prog: Option<WebGlProgram> }
    impl Default for RadiatingSpokes { fn default()->Self{Self{prog:None}} }
    impl Visualizer for RadiatingSpokes {
        fn name(&self)-> &'static str { "Radiating Spokes" }
        fn init(&mut self, gl:&GL){
            let frag_src="#version 300 es\nprecision mediump float;out vec4 o; void main(){o=vec4(0.0,0.8,1.0,1.0);}";
            self.prog=Some(link_program(gl,VERT_SRC,frag_src).unwrap());
        }
        fn render(&mut self, gl:&GL,t:f32){
            gl.clear_color(0.0,0.0,0.0,1.0); gl.clear(GL::COLOR_BUFFER_BIT);
            let prog=self.prog.as_ref().unwrap(); gl.use_program(Some(prog));
            gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),1.0);
            gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(),0.0);
            let w = gl.drawing_buffer_width() as f32; let h = gl.drawing_buffer_height() as f32;
            let (sx, sy) = if w >= h { (h / w, 1.0) } else { (1.0, w / h) };
            let loc_aspect = gl.get_uniform_location(prog, "u_aspect");
            gl.uniform2f(loc_aspect.as_ref(), sx, sy);

            // build spokes dynamically
            const SPOKES: usize = 20;
            let mut verts: Vec<f32> = Vec::with_capacity(SPOKES * 4);
            for i in 0..SPOKES {
                let ang = (i as f32 / SPOKES as f32 + t*0.02) * std::f32::consts::PI * 2.0;
                verts.push(0.0);
                verts.push(0.0);
                verts.push(ang.cos());
                verts.push(ang.sin());
            }
            let vbo=gl.create_buffer().unwrap(); gl.bind_buffer(GL::ARRAY_BUFFER,Some(&vbo));
            unsafe{ let fa=js_sys::Float32Array::view(&verts); gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER,&fa,GL::DYNAMIC_DRAW);}            
            gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0);
            gl.draw_arrays(GL::LINES,0,(SPOKES*2) as i32);
            gl.disable_vertex_attrib_array(0);
        }
    }

    struct ExpandingCrossLines { prog: Option<WebGlProgram> }
    impl Default for ExpandingCrossLines { fn default()->Self{Self{prog:None}} }
    impl Visualizer for ExpandingCrossLines {
        fn name(&self)-> &'static str { "Pulsing Cross" }
        fn init(&mut self, gl:&GL){
            let frag_src="#version 300 es\nprecision mediump float;out vec4 o;void main(){o=vec4(1.0,1.0,0.0,1.0);}";
            self.prog=Some(link_program(gl,VERT_SRC,frag_src).unwrap());
        }
        fn render(&mut self, gl:&GL,t:f32){
            gl.clear_color(0.0,0.0,0.0,1.0); gl.clear(GL::COLOR_BUFFER_BIT);
            let prog=self.prog.as_ref().unwrap(); gl.use_program(Some(prog));
            let scale=0.3+0.1*(t*2.0).sin().abs();
            gl.uniform1f(gl.get_uniform_location(prog,"u_scale").as_ref(),scale);
            gl.uniform1f(gl.get_uniform_location(prog,"u_rot").as_ref(),0.0);
            let w = gl.drawing_buffer_width() as f32; let h = gl.drawing_buffer_height() as f32;
            let (sx, sy) = if w >= h { (h / w, 1.0) } else { (1.0, w / h) };
            let loc_aspect = gl.get_uniform_location(prog, "u_aspect");
            gl.uniform2f(loc_aspect.as_ref(), sx, sy);

            let verts:[f32;8]=[ -1.0,0.0, 1.0,0.0, 0.0,-1.0, 0.0,1.0 ];
            let vbo=gl.create_buffer().unwrap(); gl.bind_buffer(GL::ARRAY_BUFFER,Some(&vbo)); unsafe{let fa=js_sys::Float32Array::view(&verts); gl.buffer_data_with_array_buffer_view(GL::ARRAY_BUFFER,&fa,GL::STATIC_DRAW);}            
            gl.enable_vertex_attrib_array(0); gl.vertex_attrib_pointer_with_i32(0,2,GL::FLOAT,false,0,0);
            gl.draw_arrays(GL::LINES,0,4);
            gl.disable_vertex_attrib_array(0);
        }
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
        }
        let elapsed_in_segment = now - *segment_start_ms.borrow();
        if elapsed_in_segment >= DURATION_MS {
            let mut idx_ref = current_index.borrow_mut();
            *idx_ref = (*idx_ref + 1) % len;
            *segment_start_ms.borrow_mut() = now;
            let name = visualizers_clone.borrow()[*idx_ref].name();
            let label = format!("{}/{} {}", *idx_ref + 1, len, name);
            let _ = super::set_overlay_text(&label);
        }
        let local_t = ((now - *segment_start_ms.borrow()) / 1000.0) as f32;
        let idx_now = *current_index.borrow();

        // Render current visualizer into offscreen target, then apply post-process to screen
        post.borrow().begin(&gl_clone);
        visualizers_clone.borrow_mut()[idx_now].render(&gl_clone, local_t);
        post.borrow().draw(&gl_clone, (now as f32) / 1000.0);

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
