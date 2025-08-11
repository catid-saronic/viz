#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn to_p(uv: (f64, f64), res: (f64, f64), scale: f64, rot: f64) -> (f64, f64) {
    let (u, v) = uv;
    let (w, h) = res;
    let m = w.min(h);
    let ax = m / w;
    let ay = m / h;
    // map uv to [-1,1] then apply aspect square scale and user scale
    let mut x = (u * 2.0 - 1.0) * ax * scale;
    let mut y = (v * 2.0 - 1.0) * ay * scale;
    // rotation
    let c = rot.cos();
    let s = rot.sin();
    let rx = c * x - s * y;
    let ry = s * x + c * y;
    x = rx; y = ry;
    (x, y)
}

fn uv_from_sq(uv_sq: (f64, f64), res: (f64, f64)) -> (f64, f64) {
    let (u, v) = uv_sq;
    let (w, h) = res;
    let m = w.min(h);
    let ax = m / w;
    let ay = m / h;
    let uu = (u - 0.5) / ax + 0.5;
    let vv = (v - 0.5) / ay + 0.5;
    (uu, vv)
}

fn approx_eq2(a: (f64, f64), b: (f64, f64), eps: f64) -> bool {
    (a.0 - b.0).abs() < eps && (a.1 - b.1).abs() < eps
}

#[wasm_bindgen_test]
fn aspect_invariant_top_mapping() {
    // Two different aspect ratios
    let res1 = (1920.0, 1080.0); // wide
    let res2 = (1080.0, 1920.0); // tall
    let scale = 1.0;
    let rot = std::f64::consts::PI / 6.0;

    // Choose uv_sq points in the canonical square space
    let samples = [
        (0.5, 0.5),
        (0.6, 0.5),
        (0.5, 0.6),
        (0.2, 0.8),
        (0.8, 0.2),
    ];

    for &uv_sq in &samples {
        let uv1 = uv_from_sq(uv_sq, res1);
        let uv2 = uv_from_sq(uv_sq, res2);
        let p1 = to_p(uv1, res1, scale, rot);
        let p2 = to_p(uv2, res2, scale, rot);
        assert!(approx_eq2(p1, p2, 1e-9), "p1={:?} p2={:?}", p1, p2);
    }
}

