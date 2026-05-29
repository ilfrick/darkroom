use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct ColorOut;

impl IopProcess for ColorOut {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "colorout" }
}

// Matches lab_f_inv() in colorspaces_inline_conversions.h.
// epsilon = cbrt(216/24389), kappa = 24389/27.
#[inline(always)]
fn lab_f_inv(x: f32) -> f32 {
    const EPSILON: f32 = 0.20689655172413796;
    const KAPPA: f32 = 24389.0 / 27.0;
    if x > EPSILON { x * x * x } else { (116.0 * x - 16.0) / KAPPA }
}

// Matches dt_Lab_to_XYZ() — D50 white point, Lab→XYZ per CIE standard.
#[inline(always)]
fn lab_to_xyz(lab: &[f32]) -> [f32; 3] {
    // D50 = { 0.9642, 1.0, 0.8249 }
    const D50: [f32; 3] = [0.9642, 1.0, 0.8249];
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = lab[1] / 500.0 + fy;
    let fz = fy - lab[2] / 200.0;
    [D50[0] * lab_f_inv(fx), D50[1] * lab_f_inv(fy), D50[2] * lab_f_inv(fz)]
}

// Matches _transform_cmatrix_linear() — Lab→XYZ then transposed-matrix multiply.
// cmatrix: pre-transposed 3×4 colormatrix (12 floats, row-major).
//   cmatrix[row*4 + out_ch] so rgb[c] = cmatrix[0][c]*X + cmatrix[1][c]*Y + cmatrix[2][c]*Z
#[no_mangle]
pub unsafe extern "C" fn darkroom_colorout_cmatrix_linear(
    in_buf: *const f32,
    out_buf: *mut f32,
    npixels: usize,
    cmatrix: *const f32,
) {
    let input = std::slice::from_raw_parts(in_buf, npixels * 4);
    let output = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    let cm = std::slice::from_raw_parts(cmatrix, 12);
    for k in 0..npixels {
        let xyz = lab_to_xyz(&input[k * 4..]);
        // rgb[c] = cm[0+c]*X + cm[4+c]*Y + cm[8+c]*Z  (transposed multiply)
        output[k * 4]     = cm[0] * xyz[0] + cm[4] * xyz[1] + cm[8]  * xyz[2];
        output[k * 4 + 1] = cm[1] * xyz[0] + cm[5] * xyz[1] + cm[9]  * xyz[2];
        output[k * 4 + 2] = cm[2] * xyz[0] + cm[6] * xyz[1] + cm[10] * xyz[2];
        output[k * 4 + 3] = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lab_f_inv_identity_at_epsilon() {
        // For x > epsilon, should be x^3
        let x = 0.5f32;
        assert!((lab_f_inv(x) - x * x * x).abs() < 1e-6);
    }

    #[test]
    fn lab_f_inv_linear_below_epsilon() {
        let x = 0.1f32;
        let expected = (116.0 * x - 16.0) / (24389.0 / 27.0);
        assert!((lab_f_inv(x) - expected).abs() < 1e-6);
    }

    #[test]
    fn d65_white_in_lab_gives_d50_xyz() {
        // L=100, a=0, b=0 is D50 white in Lab
        let lab = [100.0f32, 0.0, 0.0, 0.0];
        let xyz = lab_to_xyz(&lab);
        // Should be close to D50 = [0.9642, 1.0, 0.8249]
        assert!((xyz[0] - 0.9642).abs() < 1e-4, "X={}", xyz[0]);
        assert!((xyz[1] - 1.0).abs() < 1e-4,    "Y={}", xyz[1]);
        assert!((xyz[2] - 0.8249).abs() < 1e-4,  "Z={}", xyz[2]);
    }

    #[test]
    fn identity_cmatrix_passes_xyz_through() {
        // Use a 3×4 identity-like cmatrix (transposed form):
        // row 0 = [1,0,0,0], row 1 = [0,1,0,0], row 2 = [0,0,1,0]
        // rgb[0] = 1*X + 0*Y + 0*Z = X, etc.
        let cm = [
            1.0f32, 0.0, 0.0, 0.0,   // row 0
            0.0f32, 1.0, 0.0, 0.0,   // row 1
            0.0f32, 0.0, 1.0, 0.0,   // row 2
        ];
        // L=100, a=0, b=0 → XYZ = D50
        let input = vec![100.0f32, 0.0, 0.0, 1.0];
        let mut out = vec![0.0f32; 4];
        unsafe {
            darkroom_colorout_cmatrix_linear(
                input.as_ptr(), out.as_mut_ptr(), 1, cm.as_ptr()
            );
        }
        assert!((out[0] - 0.9642).abs() < 1e-4, "R={}", out[0]);
        assert!((out[1] - 1.0).abs()    < 1e-4,  "G={}", out[1]);
        assert!((out[2] - 0.8249).abs() < 1e-4,  "B={}", out[2]);
        assert_eq!(out[3], 0.0); // alpha always zeroed
    }
}
