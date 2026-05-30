use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Rasterfile;

impl IopProcess for Rasterfile {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "rasterfile" }
}

/// Visualisation overlay for the single-channel (CFA-blurred raw) path of
/// the rasterfile IOP.
///
/// For each pixel k:
///   out[k] = 0.2 * clamp(sqrt(out[k]), 0.0, 0.5)  + (mask[k] if mask else 0.0)
///
/// `out_buf` is read AND written in place. `mask` may be null. Matches the
/// `ch == 1` branch of the visual block in src/iop/rasterfile.c process().
#[no_mangle]
pub unsafe extern "C" fn darkroom_rasterfile_visual_single(
    out_buf: *mut f32,
    mask: *const f32,
    npixels: usize,
) {
    let out = std::slice::from_raw_parts_mut(out_buf, npixels);
    let mask_slice = if mask.is_null() {
        None
    } else {
        Some(std::slice::from_raw_parts(mask, npixels))
    };

    for k in 0..npixels {
        let s = out[k].max(0.0).sqrt().clamp(0.0, 0.5);
        let m = mask_slice.map_or(0.0, |s| s[k]);
        out[k] = 0.2 * s + m;
    }
}

/// Visualisation overlay for the RGBA path of the rasterfile IOP.
///
/// For each pixel k:
///   val = 0.2 * clamp(sqrt(0.33 * (R + G + B)), 0.0, 0.5) + mask[k]
///   out[4k + 0..3] = val          (R, G, B set to the same grey)
/// The alpha channel is left untouched.
///
/// Matches the `ch != 1` branch of the visual block in src/iop/rasterfile.c
/// process(). `for_three_channels` writes to indices 0,1,2 only.
#[no_mangle]
pub unsafe extern "C" fn darkroom_rasterfile_visual_rgba(
    out_buf: *mut f32,
    mask: *const f32,
    npixels: usize,
) {
    let out = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    let mask_slice = if mask.is_null() {
        None
    } else {
        Some(std::slice::from_raw_parts(mask, npixels))
    };

    for k in 0..npixels {
        let i = k * 4;
        let avg = 0.33 * (out[i] + out[i + 1] + out[i + 2]);
        let s = avg.max(0.0).sqrt().clamp(0.0, 0.5);
        let m = mask_slice.map_or(0.0, |s| s[k]);
        let val = 0.2 * s + m;
        out[i]     = val;
        out[i + 1] = val;
        out[i + 2] = val;
        // i + 3 (alpha) intentionally left as-is
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_no_mask_just_grades_sqrt() {
        // sqrt(0.25) = 0.5 → clamp(_, 0, 0.5) = 0.5 → 0.2 * 0.5 = 0.1
        let mut out = vec![0.25_f32];
        unsafe { darkroom_rasterfile_visual_single(out.as_mut_ptr(), std::ptr::null(), 1); }
        assert!((out[0] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn single_clamps_sqrt_to_half() {
        // sqrt(4) = 2 → clamp(_, 0, 0.5) = 0.5 → 0.2 * 0.5 = 0.1
        let mut out = vec![4.0_f32];
        unsafe { darkroom_rasterfile_visual_single(out.as_mut_ptr(), std::ptr::null(), 1); }
        assert!((out[0] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn single_adds_mask_value() {
        // sqrt(0) clamp = 0 → 0.2 * 0 + mask = mask
        let mut out = vec![0.0_f32];
        let mask = vec![0.42_f32];
        unsafe { darkroom_rasterfile_visual_single(out.as_mut_ptr(), mask.as_ptr(), 1); }
        assert!((out[0] - 0.42).abs() < 1e-6);
    }

    #[test]
    fn rgba_writes_same_value_to_first_three_channels() {
        // out = [0.5, 0.5, 0.5, alpha]; 0.33 * 1.5 = 0.495; sqrt ≈ 0.704 → clamp to 0.5
        // val = 0.2 * 0.5 + 0 = 0.1
        let mut out = vec![0.5_f32, 0.5, 0.5, 0.42];
        unsafe { darkroom_rasterfile_visual_rgba(out.as_mut_ptr(), std::ptr::null(), 1); }
        assert!((out[0] - 0.1).abs() < 1e-5);
        assert!((out[1] - 0.1).abs() < 1e-5);
        assert!((out[2] - 0.1).abs() < 1e-5);
        // alpha unchanged
        assert_eq!(out[3], 0.42);
    }

    #[test]
    fn rgba_adds_mask_per_pixel() {
        let mut out = vec![0.0_f32, 0.0, 0.0, 1.0];
        let mask = vec![0.3_f32];
        unsafe { darkroom_rasterfile_visual_rgba(out.as_mut_ptr(), mask.as_ptr(), 1); }
        // sqrt(0) = 0 → 0.2 * 0 = 0 → + 0.3 = 0.3
        assert!((out[0] - 0.3).abs() < 1e-5);
        assert!((out[1] - 0.3).abs() < 1e-5);
        assert!((out[2] - 0.3).abs() < 1e-5);
    }
}
