use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Colorequal;

impl IopProcess for Colorequal {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "colorequal" }
}

/// Initialise the per-pixel UV covariance matrix from raw UV values.
///
///   cov[k*4 + 0] = U*U
///   cov[k*4 + 1] = cov[k*4 + 2] = U*V
///   cov[k*4 + 3] = V*V
///
/// Matches `_init_covariance()` in src/iop/colorequal.c:482.
/// `uv_buf` is `pixels * 2` floats (interleaved U, V per pixel).
/// `cov_buf` must be `pixels * 4` floats.
#[no_mangle]
pub unsafe extern "C" fn darkroom_colorequal_init_covariance(
    uv_buf: *const f32,
    cov_buf: *mut f32,
    pixels: usize,
) {
    if pixels == 0 { return; }
    let uv  = std::slice::from_raw_parts(uv_buf,  pixels * 2);
    let cov = std::slice::from_raw_parts_mut(cov_buf, pixels * 4);
    for k in 0..pixels {
        let u = uv[2 * k];
        let v = uv[2 * k + 1];
        cov[4 * k]     = u * u;
        cov[4 * k + 1] = u * v;
        cov[4 * k + 2] = u * v;
        cov[4 * k + 3] = v * v;
    }
}

/// Finalise the covariance matrix by subtracting avg(x)*avg(y).
///
///   cov[k*4 + 0] -= U*U
///   cov[k*4 + 1] -= U*V
///   cov[k*4 + 2] -= U*V
///   cov[k*4 + 3] -= V*V
///
/// Matches `_finish_covariance()` in src/iop/colorequal.c:502.
/// `uv_buf` here contains the **blurred** averages of U and V (the output
/// of the guided-filter blur step performed by the caller before this call).
#[no_mangle]
pub unsafe extern "C" fn darkroom_colorequal_finish_covariance(
    uv_buf: *const f32,
    cov_buf: *mut f32,
    pixels: usize,
) {
    if pixels == 0 { return; }
    let uv  = std::slice::from_raw_parts(uv_buf,  pixels * 2);
    let cov = std::slice::from_raw_parts_mut(cov_buf, pixels * 4);
    for k in 0..pixels {
        let u = uv[2 * k];
        let v = uv[2 * k + 1];
        cov[4 * k]     -= u * u;
        cov[4 * k + 1] -= u * v;
        cov[4 * k + 2] -= u * v;
        cov[4 * k + 3] -= v * v;
    }
}

/// Compute the per-pixel guided-filter regression coefficients (a, b) for
/// the 2D UV space.
///
/// For each pixel k:
///   Σ = cov + ε * I₂     (2×2 regularised covariance)
///   if |det(Σ)| > 4*FLT_EPSILON:
///     Σ⁻¹ computed analytically
///     a[k*4 .. k*4+4] = 2×2 regression matrix from cov and Σ⁻¹
///   else:
///     a[k*4 .. k*4+4] = 0
///   b[k*2 + 0] = U - a[k*4+0]*U - a[k*4+1]*V
///   b[k*2 + 1] = V - a[k*4+2]*U - a[k*4+3]*V
///
/// Matches `_prepare_prefilter()` in src/iop/colorequal.c:523.
#[no_mangle]
pub unsafe extern "C" fn darkroom_colorequal_prepare_prefilter(
    uv_buf: *const f32,
    cov_buf: *const f32,
    a_buf: *mut f32,
    b_buf: *mut f32,
    pixels: usize,
    eps: f32,
) {
    if pixels == 0 { return; }
    let uv  = std::slice::from_raw_parts(uv_buf,  pixels * 2);
    let cov = std::slice::from_raw_parts(cov_buf, pixels * 4);
    let a   = std::slice::from_raw_parts_mut(a_buf, pixels * 4);
    let b   = std::slice::from_raw_parts_mut(b_buf, pixels * 2);

    for k in 0..pixels {
        let sigma = [
            cov[4 * k]     + eps,
            cov[4 * k + 1],
            cov[4 * k + 2],
            cov[4 * k + 3] + eps,
        ];
        let det = sigma[0] * sigma[3] - sigma[1] * sigma[2];

        if det.abs() > 4.0 * f32::EPSILON {
            let sigma_inv = [
                 sigma[3] / det,
                -sigma[1] / det,
                -sigma[2] / det,
                 sigma[0] / det,
            ];
            a[4 * k]     = cov[4 * k]     * sigma_inv[0] + cov[4 * k + 1] * sigma_inv[1];
            a[4 * k + 1] = cov[4 * k]     * sigma_inv[2] + cov[4 * k + 1] * sigma_inv[3];
            a[4 * k + 2] = cov[4 * k + 2] * sigma_inv[0] + cov[4 * k + 3] * sigma_inv[1];
            a[4 * k + 3] = cov[4 * k + 2] * sigma_inv[2] + cov[4 * k + 3] * sigma_inv[3];
        } else {
            a[4 * k] = 0.0; a[4 * k + 1] = 0.0;
            a[4 * k + 2] = 0.0; a[4 * k + 3] = 0.0;
        }

        let u = uv[2 * k];
        let v = uv[2 * k + 1];
        b[2 * k]     = u - a[4 * k]     * u - a[4 * k + 1] * v;
        b[2 * k + 1] = v - a[4 * k + 2] * u - a[4 * k + 3] * v;
    }
}

/// Linearly-interpolated lookup in the precomputed sigmoid saturation-weight
/// table. Mirrors `_get_satweight()` in colorequal.c:461.
///
/// `satweights` has `2 * satsize + 1` entries, initialised by
/// `_init_satweights(contrast)` in C. The argument `sat` is the raw
/// difference `saturation[k] - sat_shift`; values outside `[-1, 1)` are
/// clamped before indexing.
#[inline(always)]
fn get_satweight(sat: f32, satweights: &[f32], satsize: usize) -> f32 {
    // CLAMP(sat, -1, 1 - 1/SATSIZE) then map to [0, 2*SATSIZE]
    let sat_clamp = sat.clamp(-1.0, 1.0 - (1.0 / satsize as f32));
    let isat = satsize as f32 * (1.0 + sat_clamp);
    let base = isat.floor();
    let i = base as usize;
    satweights[i] + (isat - base) * (satweights[i + 1] - satweights[i])
}

/// Apply the guided-filter regression to correct UV, blending with the
/// original based on a sigmoid saturation weight.
///
/// For each pixel k:
///   u_corr = a[k*4+0]*U + a[k*4+1]*V + b[k*2+0]
///   v_corr = a[k*4+2]*U + a[k*4+3]*V + b[k*2+1]
///   w = get_satweight(saturation[k] - sat_shift, satweights, satsize)
///   UV[k*2+0] = U + w * (u_corr - U)      ← lerp toward correction
///   UV[k*2+1] = V + w * (v_corr - V)
///
/// `satweights` is the precomputed logistic table (length `2*satsize+1`),
/// filled by `_init_satweights(contrast)` in C. The Rust port does not
/// recompute it; the caller passes the live C static array pointer.
///
/// Matches `_apply_prefilter()` in src/iop/colorequal.c:573.
#[no_mangle]
pub unsafe extern "C" fn darkroom_colorequal_apply_prefilter(
    uv_buf: *mut f32,
    saturation: *const f32,
    a_buf: *const f32,
    b_buf: *const f32,
    npixels: usize,
    sat_shift: f32,
    satweights: *const f32,
    satsize: usize,
) {
    if npixels == 0 || satsize == 0 { return; }
    let uv  = std::slice::from_raw_parts_mut(uv_buf, npixels * 2);
    let sat = std::slice::from_raw_parts(saturation, npixels);
    let a   = std::slice::from_raw_parts(a_buf, npixels * 4);
    let b   = std::slice::from_raw_parts(b_buf, npixels * 2);
    let sw  = std::slice::from_raw_parts(satweights, 2 * satsize + 1);

    for k in 0..npixels {
        let u = uv[2 * k];
        let v = uv[2 * k + 1];
        let u_corr = a[4 * k]     * u + a[4 * k + 1] * v + b[2 * k];
        let v_corr = a[4 * k + 2] * u + a[4 * k + 3] * v + b[2 * k + 1];
        let w = get_satweight(sat[k] - sat_shift, sw, satsize);
        uv[2 * k]     = u + w * (u_corr - u);
        uv[2 * k + 1] = v + w * (v_corr - v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_covariance_computes_outer_product() {
        // UV = [(2, 3)] → cov = [4, 6, 6, 9]
        let uv  = vec![2.0_f32, 3.0];
        let mut cov = vec![0.0_f32; 4];
        unsafe { darkroom_colorequal_init_covariance(uv.as_ptr(), cov.as_mut_ptr(), 1); }
        assert_eq!(cov, vec![4.0, 6.0, 6.0, 9.0]);
    }

    #[test]
    fn finish_covariance_subtracts_product() {
        // cov was [10, 8, 8, 12]; avg = (2, 3) → subtract [4,6,6,9] → [6,2,2,3]
        let uv  = vec![2.0_f32, 3.0];
        let mut cov = vec![10.0_f32, 8.0, 8.0, 12.0];
        unsafe { darkroom_colorequal_finish_covariance(uv.as_ptr(), cov.as_mut_ptr(), 1); }
        assert_eq!(cov, vec![6.0, 2.0, 2.0, 3.0]);
    }

    #[test]
    fn prepare_prefilter_identity_when_cov_is_eps_times_identity() {
        // cov = 0; σ = ε*I; σ⁻¹ = (1/ε)*I; a = cov * σ⁻¹ = 0; b = UV
        let uv  = vec![0.5_f32, 0.7];
        let cov = vec![0.0_f32; 4];
        let mut a = vec![99.0_f32; 4];
        let mut b = vec![99.0_f32; 2];
        unsafe {
            darkroom_colorequal_prepare_prefilter(
                uv.as_ptr(), cov.as_ptr(), a.as_mut_ptr(), b.as_mut_ptr(), 1, 1e-4,
            );
        }
        // a should be 0
        for v in &a { assert!(v.abs() < 1e-6, "a={v}"); }
        // b = uv since a is 0
        assert!((b[0] - 0.5).abs() < 1e-6);
        assert!((b[1] - 0.7).abs() < 1e-6);
    }

    #[test]
    fn prepare_prefilter_singular_matrix_zeroes_a() {
        // All-zero cov + tiny eps → near-singular matrix
        let uv  = vec![1.0_f32, 1.0];
        let cov = vec![0.0_f32; 4];
        let mut a = vec![1.0_f32; 4];
        let mut b = vec![0.0_f32; 2];
        // eps = 0 → det = 0 → singular path
        unsafe {
            darkroom_colorequal_prepare_prefilter(
                uv.as_ptr(), cov.as_ptr(), a.as_mut_ptr(), b.as_mut_ptr(), 1, 0.0,
            );
        }
        for v in &a { assert_eq!(*v, 0.0, "a={v}"); }
    }

    /// Build a satweights table with the same formula as C _init_satweights.
    fn make_satweights(satsize: usize, contrast: f64) -> Vec<f32> {
        let factor = -60.0 - 40.0 * contrast;
        let n = 2 * satsize + 1;
        (0..n).map(|idx| {
            let i = idx as i64 - satsize as i64;
            let val = 0.5 / satsize as f64 * i as f64;
            (1.0 / (1.0 + (factor * val).exp())) as f32
        }).collect()
    }

    #[test]
    fn apply_prefilter_identity_correction_is_noop() {
        // a = identity, b = 0 → u_corr = u, v_corr = v → no change regardless of satweight
        const SATSIZE: usize = 4096;
        let sw = make_satweights(SATSIZE, 0.0);
        let mut uv = vec![0.3_f32, 0.5];
        let sat = vec![0.5_f32];
        let a = vec![1.0_f32, 0.0, 0.0, 1.0];
        let b = vec![0.0_f32, 0.0];
        unsafe {
            darkroom_colorequal_apply_prefilter(
                uv.as_mut_ptr(), sat.as_ptr(), a.as_ptr(), b.as_ptr(),
                1, 0.0, sw.as_ptr(), SATSIZE,
            );
        }
        assert!((uv[0] - 0.3).abs() < 1e-5);
        assert!((uv[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn apply_prefilter_uses_sigmoid_not_linear_ramp() {
        // With contrast=0 the sigmoid at sat-sat_shift=0.0 should be 0.5 (midpoint)
        // not 1.0 (which the old linear ramp would give at sat_shift=0.0, sat=0.0).
        const SATSIZE: usize = 4096;
        let sw = make_satweights(SATSIZE, 0.0);
        let weight = get_satweight(0.0, &sw, SATSIZE);
        // Logistic at 0 → 0.5 (regardless of contrast)
        assert!((weight - 0.5).abs() < 0.01, "sigmoid midpoint should be ~0.5, got {weight}");
    }
}
