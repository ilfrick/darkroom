use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Basecurve;

impl IopProcess for Basecurve {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "basecurve" }
}

const LUT_SIZE: usize = 0x10000; // 65536

/// Integer-truncation LUT lookup matching table[CLAMP((int)(f*0x10000), 0, 0xffff)].
/// Output is floored at 0.
#[inline(always)]
fn lut_lookup(table: &[f32], f: f32) -> f32 {
    let idx = ((f * LUT_SIZE as f32) as i32).clamp(0, (LUT_SIZE - 1) as i32) as usize;
    table[idx].max(0.0)
}

/// Unbounded extrapolation: coeff[1] * pow(v * coeff[0], coeff[2]).
/// Matches dt_iop_eval_exp() in imageop_math.h.
#[inline(always)]
fn eval_exp(coeff: &[f32], v: f32) -> f32 {
    coeff[1] * (v * coeff[0]).powf(coeff[2])
}

/// Fast exp approximation matching dt_fast_expf() in common/math.h.
/// Valid for x in [-100, 0]; behaviour outside that range is intentionally imprecise.
#[inline(always)]
fn fast_expf(x: f32) -> f32 {
    const I1: f32 = 0x3f800000_u32 as f32;
    const SCALE: f32 = (0x402DF854_u32 - 0x3f800000_u32) as f32;
    let k0 = (I1 + x * SCALE) as i32;
    f32::from_bits(if k0 > 0 { k0 as u32 } else { 0 })
}

/// Per-channel tone curve (integer-truncation LUT) for the legacy no-preserve-colors path.
///
/// Matches apply_legacy_curve() in src/iop/basecurve.c.
/// table:            65536 floats — single shared LUT for all RGB channels.
/// unbounded_coeffs: 3 floats — [coeff0, coeff1, coeff2] for eval_exp extrapolation.
/// mul:              pre-scalar applied to every channel value before the LUT lookup.
#[no_mangle]
pub unsafe extern "C" fn darkroom_basecurve_apply_legacy_curve(
    in_buf: *const f32,
    out_buf: *mut f32,
    npixels: usize,
    mul: f32,
    table: *const f32,
    unbounded_coeffs: *const f32,
) {
    let input = std::slice::from_raw_parts(in_buf, npixels * 4);
    let output = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    let lut = std::slice::from_raw_parts(table, LUT_SIZE);
    let coeffs = std::slice::from_raw_parts(unbounded_coeffs, 3);

    for k in 0..npixels {
        let base = k * 4;
        for i in 0..3 {
            let f = input[base + i] * mul;
            output[base + i] = if f < 1.0 {
                lut_lookup(lut, f)
            } else {
                eval_exp(coeffs, f).max(0.0)
            };
        }
        output[base + 3] = input[base + 3];
    }
}

/// Compute per-pixel exposure-fusion features into the alpha channel in-place.
///
/// Matches compute_features() in src/iop/basecurve.c.
/// Writes sat * well_exposedness into buf[k*4+3] for every pixel k.
#[no_mangle]
pub unsafe extern "C" fn darkroom_basecurve_compute_features(
    buf: *mut f32,
    npixels: usize,
) {
    let buf = std::slice::from_raw_parts_mut(buf, npixels * 4);

    for k in 0..npixels {
        let x = k * 4;
        let r = buf[x];
        let g = buf[x + 1];
        let b = buf[x + 2];

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let sat = 0.1_f32 + 0.1_f32 * (max - min) / max.max(1e-4_f32);

        const C: f32 = 0.54;
        let v = (r - C).abs().max((g - C).abs()).max((b - C).abs());
        const VAR_SQ: f32 = 0.5 * 0.5; // var = 0.5
        let exp_val = 0.2_f32 + fast_expf(-v * v / VAR_SQ);

        buf[x + 3] = sat * exp_val;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lut_lookup_identity_lut() {
        let lut: Vec<f32> = (0..LUT_SIZE).map(|k| k as f32 / LUT_SIZE as f32).collect();
        // f=0.5 → index = int(0.5 * 65536) = 32768 → lut[32768] = 32768/65536 = 0.5
        let out = lut_lookup(&lut, 0.5);
        assert!((out - 0.5).abs() < 1e-4, "out={out}");
    }

    #[test]
    fn lut_lookup_negative_clips_to_zero() {
        let lut = vec![0.42_f32; LUT_SIZE];
        assert_eq!(lut_lookup(&lut, -1.0), 0.42);
    }

    #[test]
    fn lut_lookup_floors_negative_lut_values() {
        let mut lut = vec![0.0_f32; LUT_SIZE];
        lut[0] = -0.5;
        assert_eq!(lut_lookup(&lut, 0.0), 0.0);
    }

    #[test]
    fn eval_exp_matches_formula() {
        let coeff = [2.0_f32, 3.0, 0.5];
        let v = 0.25_f32;
        let expected = 3.0 * (0.25 * 2.0_f32).powf(0.5);
        assert!((eval_exp(&coeff, v) - expected).abs() < 1e-5);
    }

    #[test]
    fn apply_legacy_curve_alpha_passthrough() {
        let lut: Vec<f32> = (0..LUT_SIZE).map(|k| k as f32 / LUT_SIZE as f32).collect();
        let coeffs = [1.0_f32, 1.0, 1.0];
        let input = vec![0.25_f32, 0.5, 0.75, 0.9999];
        let mut out = vec![0.0_f32; 4];
        unsafe {
            darkroom_basecurve_apply_legacy_curve(
                input.as_ptr(), out.as_mut_ptr(), 1, 1.0,
                lut.as_ptr(), coeffs.as_ptr(),
            );
        }
        assert!((out[0] - 0.25).abs() < 1e-3, "R={}", out[0]);
        assert!((out[1] - 0.5 ).abs() < 1e-3, "G={}", out[1]);
        assert!((out[2] - 0.75).abs() < 1e-3, "B={}", out[2]);
        assert_eq!(out[3], 0.9999); // alpha unchanged
    }

    #[test]
    fn apply_legacy_curve_unbounded_path() {
        // f >= 1.0 triggers eval_exp, result floored at 0
        let lut = vec![0.0_f32; LUT_SIZE];
        // coeff: coeff[1]*pow(v*coeff[0], coeff[2]) = 1*pow(2*1, 1) = 2
        let coeffs = [1.0_f32, 1.0, 1.0];
        let input = vec![2.0_f32, 2.0, 2.0, 1.0];
        let mut out = vec![0.0_f32; 4];
        unsafe {
            darkroom_basecurve_apply_legacy_curve(
                input.as_ptr(), out.as_mut_ptr(), 1, 1.0,
                lut.as_ptr(), coeffs.as_ptr(),
            );
        }
        assert!((out[0] - 2.0).abs() < 1e-5, "R={}", out[0]);
    }

    #[test]
    fn compute_features_grey_pixel() {
        // For a grey pixel r=g=b=0.54: max==min → sat=0.1, v=0 → exp_val=0.2+fast_expf(0)≈1.2
        let mut buf = vec![0.54_f32, 0.54, 0.54, 0.0];
        unsafe { darkroom_basecurve_compute_features(buf.as_mut_ptr(), 1); }
        // sat = 0.1, exp_val ≈ 0.2 + 1.0 = 1.2 (fast_expf(0) ≈ 1.0)
        let alpha = buf[3];
        assert!(alpha > 0.0 && alpha < 1.0, "alpha={alpha}");
    }

    #[test]
    fn compute_features_does_not_touch_rgb() {
        let mut buf = vec![0.3_f32, 0.5, 0.7, 0.0];
        let orig = [buf[0], buf[1], buf[2]];
        unsafe { darkroom_basecurve_compute_features(buf.as_mut_ptr(), 1); }
        assert_eq!(buf[0], orig[0]);
        assert_eq!(buf[1], orig[1]);
        assert_eq!(buf[2], orig[2]);
    }
}
