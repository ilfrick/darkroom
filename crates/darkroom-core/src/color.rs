//! Shared color-space utilities used across multiple IOP modules.

// ── HSL ↔ RGB (from src/common/colorspaces.h) ────────────────────────────────

fn hue2rgb(m1: f32, m2: f32, hue: f32) -> f32 {
    if hue < 1.0 {
        m1 + (m2 - m1) * hue
    } else if hue < 3.0 {
        m2
    } else if hue < 4.0 {
        m1 + (m2 - m1) * (4.0 - hue)
    } else {
        m1
    }
}

/// Returns (h, s, l) in [0,1]. Matches C rgb2hsl().
pub fn rgb2hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let pmax = r.max(g).max(b);
    let pmin = r.min(g).min(b);
    let lv = (pmin + pmax) * 0.5;
    let delta = pmax - pmin;
    if delta == 0.0 {
        return (0.0, 0.0, lv);
    }
    const EPS: f32 = 1.52587890625e-05;
    let sv = if lv < 0.5 {
        delta / (pmax + pmin).max(EPS)
    } else {
        delta / (2.0 - pmax - pmin).max(EPS)
    };
    let mut hv = if pmax == r {
        (g - b) / delta
    } else if pmax == g {
        2.0 + (b - r) / delta
    } else {
        4.0 + (r - g) / delta
    };
    hv /= 6.0;
    if hv < 0.0 {
        hv += 1.0;
    } else if hv > 1.0 {
        hv -= 1.0;
    }
    (hv, sv, lv)
}

/// Returns (r, g, b, 0.0). Matches C hsl2rgb() — alpha always 0.
pub fn hsl2rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32, f32) {
    if s == 0.0 {
        return (l, l, l, 0.0);
    }
    let m2 = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let m1 = 2.0 * l - m2;
    let h6 = h * 6.0;
    let r = hue2rgb(m1, m2, if h6 < 4.0 { h6 + 2.0 } else { h6 - 4.0 });
    let g = hue2rgb(m1, m2, h6);
    let b = hue2rgb(m1, m2, if h6 > 2.0 { h6 - 2.0 } else { h6 + 4.0 });
    (r, g, b, 0.0)
}

// ── Lab ↔ XYZ (D50 white point) ───────────────────────────────────────────────

const D50: [f32; 3] = [0.9642, 1.0, 0.8249];
const D50_INV: [f32; 3] = [1.0 / 0.9642, 1.0, 1.0 / 0.8249];
const LAB_EPSILON: f32 = 216.0 / 24389.0;
const LAB_KAPPA: f32 = 24389.0 / 27.0;
// cbrt(216/24389) — threshold for lab_f_inv
const LAB_CBRT_EPSILON: f32 = 0.20689655172413796;

fn lab_f_inv(x: f32) -> f32 {
    if x > LAB_CBRT_EPSILON {
        x * x * x
    } else {
        (116.0 * x - 16.0) / LAB_KAPPA
    }
}

/// Matches C dt_Lab_to_XYZ().
pub fn lab_to_xyz(lab: [f32; 4]) -> [f32; 4] {
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = lab[1] / 500.0 + fy;
    let fz = fy - lab[2] / 200.0;
    [
        D50[0] * lab_f_inv(fx),
        D50[1] * lab_f_inv(fy),
        D50[2] * lab_f_inv(fz),
        lab[3],
    ]
}

/// Matches C dt_XYZ_to_Lab().
pub fn xyz_to_lab(xyz: [f32; 4]) -> [f32; 4] {
    let f: [f32; 3] = std::array::from_fn(|i| {
        let x = xyz[i] * D50_INV[i];
        if x > LAB_EPSILON {
            x.cbrt()
        } else {
            (LAB_KAPPA * x + 16.0) / 116.0
        }
    });
    [
        116.0 * f[1] - 16.0,
        500.0 * (f[0] - f[1]),
        -200.0 * (f[2] - f[1]),
        xyz[3],
    ]
}

// ── Lab ↔ ProPhoto RGB ────────────────────────────────────────────────────────

// Transposed matrices from colorspaces_inline_conversions.h:439-462.
// applied as: out[i] = sum_j M_T[j][i] * in[j]
//
// xyz_to_prophotorgb_transpose row j, col i:
//   j=0: [1.3459433, -0.5445989, 0.0,       0.0]
//   j=1: [-0.2556075, 1.5081673, 0.0,       0.0]
//   j=2: [-0.0511118, 0.0205351, 1.2118128, 0.0]
//
// rgb[0] = 1.3459433*X - 0.2556075*Y - 0.0511118*Z
// rgb[1] = -0.5445989*X + 1.5081673*Y + 0.0205351*Z
// rgb[2] = 0.0*X + 0.0*Y + 1.2118128*Z

pub fn xyz_to_prophotorgb(xyz: [f32; 4]) -> [f32; 4] {
    [
        1.3459433 * xyz[0] - 0.2556075 * xyz[1] - 0.0511118 * xyz[2],
        -0.5445989 * xyz[0] + 1.5081673 * xyz[1] + 0.0205351 * xyz[2],
        1.2118128 * xyz[2],
        xyz[3],
    ]
}

// prophotorgb_to_xyz_transpose:
//   j=0: [0.7976749, 0.2880402, 0.0, 0.0]
//   j=1: [0.1351917, 0.7118741, 0.0, 0.0]
//   j=2: [0.0313534, 0.0000857, 0.8252100, 0.0]
//
// XYZ[0] = 0.7976749*r + 0.1351917*g + 0.0313534*b
// XYZ[1] = 0.2880402*r + 0.7118741*g + 0.0000857*b
// XYZ[2] = 0.0*r + 0.0*g + 0.8252100*b

pub fn prophotorgb_to_xyz(rgb: [f32; 4]) -> [f32; 4] {
    [
        0.7976749 * rgb[0] + 0.1351917 * rgb[1] + 0.0313534 * rgb[2],
        0.2880402 * rgb[0] + 0.7118741 * rgb[1] + 0.0000857 * rgb[2],
        0.8252100 * rgb[2],
        rgb[3],
    ]
}

pub fn lab_to_prophotorgb(lab: [f32; 4]) -> [f32; 4] {
    xyz_to_prophotorgb(lab_to_xyz(lab))
}

pub fn prophotorgb_to_lab(rgb: [f32; 4]) -> [f32; 4] {
    xyz_to_lab(prophotorgb_to_xyz(rgb))
}

// ── rgb_norm (from src/common/rgb_norms.h) ────────────────────────────────────

/// ProPhoto RGB luminance = Y from prophotorgb_to_XYZ.
pub fn prophoto_luminance(r: f32, g: f32, b: f32) -> f32 {
    0.2880402 * r + 0.7118741 * g + 0.0000857 * b
}

/// Matches dt_rgb_norm() using hardcoded ProPhoto profile (tonecurve always uses ProPhoto).
pub fn rgb_norm(r: f32, g: f32, b: f32, mode: i32) -> f32 {
    match mode {
        1 => prophoto_luminance(r, g, b),
        2 => r.max(g).max(b),
        3 => (r + g + b) / 3.0,
        4 => r + g + b,
        5 => (r * r + g * g + b * b).sqrt(),
        6 => {
            let r2 = r * r;
            let g2 = g * g;
            let b2 = b * b;
            let den = r2 + g2 + b2;
            if den > 0.0 {
                (r * r2 + g * g2 + b * b2) / den
            } else {
                (r + g + b) / 3.0
            }
        }
        _ => (r + g + b) / 3.0,
    }
}

// ── eval_exp (unbounded LUT extrapolation) ────────────────────────────────────

/// coeff[1] * (x * coeff[0])^coeff[2] — darktable's eval_exp for LUT tails.
pub fn eval_exp(coeff: &[f32], x: f32) -> f32 {
    coeff[1] * (x * coeff[0]).powf(coeff[2])
}
