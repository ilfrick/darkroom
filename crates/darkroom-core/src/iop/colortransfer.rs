use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Colortransfer;

impl IopProcess for Colortransfer {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "colortransfer" }
}

/// Apply the L-histogram-matching pass of the colortransfer IOP.
///
/// This function migrates ONLY the first DT_OMP_FOR loop of the APPLY
/// branch in src/iop/colortransfer.c (line 327). It does not touch the
/// a/b channels — that is the second OMP loop (line 352) which depends on
/// k-means clustering and remains in C for now. Production callers must
/// follow this call with the C-side ab-cluster pass; failure to do so
/// leaves ab unchanged in the output. The unit tests below verify the
/// function in isolation only.
///
/// Per pixel:
///   src_bin     = clamp((histn as f32) * in_L / 100, 0, histn - 1)
///   target_bin  = cdf_lut[src_bin]      (already normalised to [0, histn-1])
///   out_L       = clamp(inverse_cdf[target_bin], 0, 100)
///
/// `cdf_lut` is the normalised cumulative-distribution lookup produced by
/// `capture_histogram()` in C (line 139 normalises values to [0, HISTN-1]
/// via `hist[k] = CLAMP(hist[k] * HISTN / hist[HISTN-1], 0, HISTN-1)`).
/// `inverse_cdf` is the inverse-CDF lookup produced by `invert_histogram()`
/// — values in [0, 100). We clamp the final output to [0, 100] defensively.
///
/// Both LUTs are exactly `histn` entries long.
#[no_mangle]
pub unsafe extern "C" fn darkroom_colortransfer_apply_l_histogram(
    in_buf: *const f32,
    out_buf: *mut f32,
    width: usize,
    height: usize,
    ch: usize,
    cdf_lut: *const i32,
    inverse_cdf: *const f32,
    histn: usize,
) {
    if ch == 0 || histn == 0 { return; }
    let npx = width * height;
    if npx == 0 { return; } // Guards against from_raw_parts(NULL, 0) UB.

    debug_assert!(
        width.checked_mul(height).and_then(|n| n.checked_mul(ch)).is_some(),
        "width * height * ch overflows usize"
    );

    let input = std::slice::from_raw_parts(in_buf, npx * ch);
    let output = std::slice::from_raw_parts_mut(out_buf, npx * ch);
    let cdf = std::slice::from_raw_parts(cdf_lut, histn);
    let inv = std::slice::from_raw_parts(inverse_cdf, histn);
    let last = (histn - 1) as i32;

    for k in 0..npx {
        let j = k * ch;
        let l = input[j];
        // First clamp: float-domain saturation of HISTN * L / 100 into [0, last].
        let bin_f = ((histn as f32) * l / 100.0).clamp(0.0, last as f32);
        // Second clamp is intentional, not paranoia: float→int cast can yield a
        // negative i32 for `-0.0` or denormal inputs that survive the float
        // clamp on some targets. Keep it as the load-bearing safety net.
        let src_bin = (bin_f as i32).clamp(0, last) as usize;

        // The CDF LUT is already normalised by capture_histogram (line 139 of
        // colortransfer.c) so this clamp is also defensive — it protects against
        // a non-normalised LUT being passed in by future callers.
        debug_assert!(
            cdf[src_bin] >= 0 && cdf[src_bin] < histn as i32,
            "cdf_lut[{}] = {} is out of [0, {}); caller must run capture_histogram first",
            src_bin, cdf[src_bin], histn
        );
        let target_bin = cdf[src_bin].clamp(0, last) as usize;
        output[j] = inv[target_bin].clamp(0.0, 100.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HISTN: usize = 1 << 11;

    /// Build the trivial identity pair: cdf[i] = i and inv[i] = 100*i/(HISTN-1).
    fn identity_histograms() -> (Vec<i32>, Vec<f32>) {
        let cdf: Vec<i32> = (0..HISTN as i32).collect();
        let inv: Vec<f32> = (0..HISTN).map(|i| 100.0 * i as f32 / (HISTN - 1) as f32).collect();
        (cdf, inv)
    }

    #[test]
    fn identity_histograms_preserve_luminance() {
        let (cdf, inv) = identity_histograms();
        let inp = vec![25.0_f32, 0.0, 0.0, 1.0];
        let mut out = vec![-1.0_f32; 4];
        unsafe {
            darkroom_colortransfer_apply_l_histogram(
                inp.as_ptr(), out.as_mut_ptr(), 1, 1, 4,
                cdf.as_ptr(), inv.as_ptr(), HISTN,
            );
        }
        assert!((out[0] - 25.0).abs() < 0.06, "got {}", out[0]);
    }

    #[test]
    fn negative_luminance_clamps_to_first_bin() {
        let (cdf, inv) = identity_histograms();
        let inp = vec![-10.0_f32, 0.0, 0.0, 0.0];
        let mut out = vec![-1.0_f32; 4];
        unsafe {
            darkroom_colortransfer_apply_l_histogram(
                inp.as_ptr(), out.as_mut_ptr(), 1, 1, 4,
                cdf.as_ptr(), inv.as_ptr(), HISTN,
            );
        }
        assert_eq!(out[0], 0.0);
    }

    #[test]
    fn above_one_hundred_clamps_to_last_bin() {
        let (cdf, inv) = identity_histograms();
        let inp = vec![999.0_f32, 0.0, 0.0, 0.0];
        let mut out = vec![-1.0_f32; 4];
        unsafe {
            darkroom_colortransfer_apply_l_histogram(
                inp.as_ptr(), out.as_mut_ptr(), 1, 1, 4,
                cdf.as_ptr(), inv.as_ptr(), HISTN,
            );
        }
        assert!((out[0] - 100.0).abs() < 1e-3, "got {}", out[0]);
    }

    #[test]
    fn inverse_cdf_output_is_clamped_to_unit_range() {
        // If the inverse CDF LUT has out-of-range values (e.g. a corrupted
        // data->hist) the function must still clamp.
        let cdf: Vec<i32> = vec![0; HISTN];
        let mut inv = vec![0.0_f32; HISTN];
        inv[0] = 200.0;
        let inp = vec![50.0_f32, 0.0, 0.0, 0.0];
        let mut out = vec![-1.0_f32; 4];
        unsafe {
            darkroom_colortransfer_apply_l_histogram(
                inp.as_ptr(), out.as_mut_ptr(), 1, 1, 4,
                cdf.as_ptr(), inv.as_ptr(), HISTN,
            );
        }
        assert_eq!(out[0], 100.0);
    }

    #[test]
    fn function_in_isolation_does_not_touch_ab_or_alpha() {
        // ⚠ This test verifies the L-histogram-matching pass alone. The full
        // colortransfer APPLY branch in C also runs a k-means-driven ab pass
        // (src/iop/colortransfer.c line 352) which rewrites out[..1..3]; that
        // pass is still in C for now, so the IOP as a whole DOES modify ab,
        // alpha. This test only proves that THIS function leaves them alone
        // — confirming the row stride math and the j+0-only writes.
        let (cdf, inv) = identity_histograms();
        let inp = vec![50.0_f32, -100.0, 100.0, 0.42];
        let mut out = vec![-7.0_f32, -7.0, -7.0, -7.0];
        unsafe {
            darkroom_colortransfer_apply_l_histogram(
                inp.as_ptr(), out.as_mut_ptr(), 1, 1, 4,
                cdf.as_ptr(), inv.as_ptr(), HISTN,
            );
        }
        assert!((out[0] - 50.0).abs() < 0.06);
        assert_eq!(out[1], -7.0);
        assert_eq!(out[2], -7.0);
        assert_eq!(out[3], -7.0);
    }

    #[test]
    fn zero_width_height_is_a_safe_noop() {
        let (cdf, inv) = identity_histograms();
        unsafe {
            darkroom_colortransfer_apply_l_histogram(
                std::ptr::null(), std::ptr::null_mut(), 0, 0, 4,
                cdf.as_ptr(), inv.as_ptr(), HISTN,
            );
        }
    }

    #[test]
    fn multi_pixel_row_stride_correct() {
        // 3-pixel row with ch=4 — ensure we touch out[0], out[4], out[8].
        let (cdf, inv) = identity_histograms();
        let inp = vec![
            10.0, -1.0, -1.0, -1.0,
            20.0, -1.0, -1.0, -1.0,
            30.0, -1.0, -1.0, -1.0,
        ];
        let mut out = vec![-7.0_f32; 12];
        unsafe {
            darkroom_colortransfer_apply_l_histogram(
                inp.as_ptr(), out.as_mut_ptr(), 3, 1, 4,
                cdf.as_ptr(), inv.as_ptr(), HISTN,
            );
        }
        assert!((out[0] - 10.0).abs() < 0.06);
        assert!((out[4] - 20.0).abs() < 0.06);
        assert!((out[8] - 30.0).abs() < 0.06);
        // Untouched neighbour slots still hold the sentinel
        assert_eq!(out[1], -7.0);
        assert_eq!(out[5], -7.0);
        assert_eq!(out[11], -7.0);
    }
}
