use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Colorin;

impl IopProcess for Colorin {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "colorin" }
}

const EPSILON: f32 = 216.0 / 24389.0;
const KAPPA: f32 = 24389.0 / 27.0;
const D50_INV: [f32; 3] = [1.0 / 0.9642, 1.0, 1.0 / 0.8249];

#[inline(always)]
fn xyz_to_lab_f(x: f32) -> f32 {
    if x > EPSILON { x.cbrt() } else { (KAPPA * x + 16.0) / 116.0 }
}

/// Camera-RGB → Lab via a 4×4 colour matrix (cam→XYZ) and D50 XYZ→Lab.
///
/// Replaces the per-pixel loop inside _cmatrix_fastpath_simple() in colorin.c.
///
/// `corr`:    4 white-balance correction coefficients
/// `cmatrix`: 16 floats, row-major 4×4 (dt_colormatrix_t); only the 3×3
///            top-left is used.
/// Output alpha is always 0.
#[no_mangle]
pub unsafe extern "C" fn darkroom_colorin_cmatrix_fastpath_simple(
    in_buf: *const f32,
    out_buf: *mut f32,
    npixels: usize,
    corr: *const f32,
    cmatrix: *const f32,
) {
    let input  = std::slice::from_raw_parts(in_buf, npixels * 4);
    let output = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    let cr = std::slice::from_raw_parts(corr, 4);
    // dt_colormatrix_t is float[4][4]: row stride = 4
    let cm = std::slice::from_raw_parts(cmatrix, 16);

    for k in 0..npixels {
        let b = k * 4;
        let cam = [
            input[b]     * cr[0],
            input[b + 1] * cr[1],
            input[b + 2] * cr[2],
        ];

        // XYZ[r] = cm[r*4+0]*cam[0] + cm[r*4+1]*cam[1] + cm[r*4+2]*cam[2]
        let xyz = [
            cm[0] * cam[0] + cm[1] * cam[1] + cm[2] * cam[2],
            cm[4] * cam[0] + cm[5] * cam[1] + cm[6] * cam[2],
            cm[8] * cam[0] + cm[9] * cam[1] + cm[10] * cam[2],
        ];

        // XYZ → Lab (D50 white point)
        let f = [
            xyz_to_lab_f(xyz[0] * D50_INV[0]),
            xyz_to_lab_f(xyz[1] * D50_INV[1]),
            xyz_to_lab_f(xyz[2] * D50_INV[2]),
        ];

        output[b]     = 116.0 * f[1] - 16.0;
        output[b + 1] = 500.0 * (f[0] - f[1]);
        output[b + 2] = -200.0 * (f[2] - f[1]);
        output[b + 3] = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_cmatrix() -> Vec<f32> {
        // Identity 4×4 matrix (cam == XYZ for testing)
        vec![
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ]
    }

    fn flat_corr() -> Vec<f32> { vec![1.0, 1.0, 1.0, 1.0] }

    #[test]
    fn black_pixel_maps_to_zero_lab() {
        let cm = identity_cmatrix();
        let corr = flat_corr();
        let input = vec![0.0f32, 0.0, 0.0, 1.0];
        let mut out = vec![0.0f32; 4];
        unsafe {
            darkroom_colorin_cmatrix_fastpath_simple(
                input.as_ptr(), out.as_mut_ptr(), 1, corr.as_ptr(), cm.as_ptr(),
            );
        }
        // XYZ=(0,0,0) → f=(16/116, 16/116, 16/116) → L=0, a=0, b=0
        assert!((out[0]).abs() < 1e-4, "L={}", out[0]);
        assert!((out[1]).abs() < 1e-4, "a={}", out[1]);
        assert!((out[2]).abs() < 1e-4, "b={}", out[2]);
        assert_eq!(out[3], 0.0);
    }

    #[test]
    fn d50_white_maps_to_l100() {
        // D50 white point: XYZ = (0.9642, 1.0, 0.8249)
        // With identity cmatrix, cam == XYZ, so input that white point should give L≈100
        let cm = identity_cmatrix();
        let corr = flat_corr();
        let input = vec![0.9642f32, 1.0, 0.8249, 1.0];
        let mut out = vec![0.0f32; 4];
        unsafe {
            darkroom_colorin_cmatrix_fastpath_simple(
                input.as_ptr(), out.as_mut_ptr(), 1, corr.as_ptr(), cm.as_ptr(),
            );
        }
        assert!((out[0] - 100.0).abs() < 0.01, "L={}", out[0]);
        assert!(out[1].abs() < 0.01, "a={}", out[1]);
        assert!(out[2].abs() < 0.01, "b={}", out[2]);
    }

    #[test]
    fn corr_scales_input() {
        let cm = identity_cmatrix();
        // Double the green channel
        let corr = vec![1.0f32, 2.0, 1.0, 1.0];
        let input = vec![0.1f32, 0.1, 0.1, 1.0];
        let mut out_scaled = vec![0.0f32; 4];
        let mut out_ref    = vec![0.0f32; 4];
        let corr_ref = flat_corr();
        let input_ref = vec![0.1f32, 0.2, 0.1, 1.0];
        unsafe {
            darkroom_colorin_cmatrix_fastpath_simple(
                input.as_ptr(), out_scaled.as_mut_ptr(), 1, corr.as_ptr(), cm.as_ptr(),
            );
            darkroom_colorin_cmatrix_fastpath_simple(
                input_ref.as_ptr(), out_ref.as_mut_ptr(), 1, corr_ref.as_ptr(), cm.as_ptr(),
            );
        }
        for c in 0..3 {
            assert!((out_scaled[c] - out_ref[c]).abs() < 1e-4, "c={} scaled={} ref={}", c, out_scaled[c], out_ref[c]);
        }
    }
}
