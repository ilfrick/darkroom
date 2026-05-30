use crate::{params::IopParams, raw, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Highlights;

impl IopProcess for Highlights {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "highlights" }
}

/// Build the per-pixel highlight-clipping raster mask for an sRAW (already-RGB)
/// input. For every pixel:
///
///   ref_c = max(0.5, 0.95 * clips[c])
///   mval  = max over c of (in[i + c] - ref_c) / ref_c
///   tmp[k] = max(0.0, mval)
///
/// Matches the `filters == 0` branch of `_provide_raster_mask()` in
/// src/iop/highlights.c. `in_buf` is an RGBA float buffer, `tmp_buf` is a
/// single-plane float mask of size `width * height`.
#[no_mangle]
pub unsafe extern "C" fn darkroom_highlights_mask_sraw(
    in_buf: *const f32,
    tmp_buf: *mut f32,
    width: usize,
    height: usize,
    clips: *const f32,
) {
    let n = width * height;
    let input = std::slice::from_raw_parts(in_buf, n * 4);
    let tmp = std::slice::from_raw_parts_mut(tmp_buf, n);
    let clips = std::slice::from_raw_parts(clips, 4);

    // Precompute the per-channel reference levels (max(0.5, 0.95*clip[c])).
    let mut refs = [0.0_f32; 3];
    for c in 0..3 {
        refs[c] = (0.95 * clips[c]).max(0.5);
    }

    for ox in 0..n {
        let ix = ox * 4;
        let mut mval = 0.0_f32;
        for c in 0..3 {
            let v = (input[ix + c] - refs[c]) / refs[c];
            if v > mval { mval = v; }
        }
        tmp[ox] = mval.max(0.0);
    }
}

/// Build the per-pixel highlight-clipping raster mask for a mosaic
/// (Bayer / X-Trans) input. For every pixel, look up its CFA colour via
/// `fcol(irow, icol, filters, xtrans)`, where `irow = row + roi_y` and
/// `icol = col + roi_x`, then apply the same formula as the sRAW path
/// using the per-colour reference.
///
/// Matches the `filters != 0` branch of `_provide_raster_mask()` in
/// src/iop/highlights.c. `in_buf` is a single-plane raw float buffer of
/// size `width * height`; the xtrans pattern is read only when `filters == 9`.
#[no_mangle]
pub unsafe extern "C" fn darkroom_highlights_mask_mosaic(
    in_buf: *const f32,
    tmp_buf: *mut f32,
    width: usize,
    height: usize,
    filters: u32,
    xtrans: *const u8, // 6*6 = 36 bytes
    clips: *const f32,
    irow_offset: i32,
    icol_offset: i32,
) {
    let n = width * height;
    let input = std::slice::from_raw_parts(in_buf, n);
    let tmp = std::slice::from_raw_parts_mut(tmp_buf, n);
    let clips = std::slice::from_raw_parts(clips, 4);

    // Reconstruct the 6x6 xtrans table from the raw byte pointer.
    let xt_bytes = std::slice::from_raw_parts(xtrans, 36);
    let mut xt = [[0_u8; 6]; 6];
    for r in 0..6 {
        for c in 0..6 { xt[r][c] = xt_bytes[r * 6 + c]; }
    }

    let mut refs = [0.0_f32; 4];
    for c in 0..4 {
        refs[c] = (0.95 * clips[c]).max(0.5);
    }

    for row in 0..height {
        for col in 0..width {
            let ox = row * width + col;
            let irow = row as i32 + irow_offset;
            let icol = col as i32 + icol_offset;
            let c = raw::fcol(irow, icol, filters, &xt);
            let r = refs[c];
            tmp[ox] = ((input[ox] - r) / r).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sraw_mask_zero_for_clean_pixels() {
        // Pixels well below threshold → mval = 0
        let inp = vec![0.1_f32, 0.2, 0.3, 1.0];
        let mut tmp = vec![-1.0_f32; 1];
        let clips = [1.0_f32, 1.0, 1.0, 1.0];
        unsafe {
            darkroom_highlights_mask_sraw(inp.as_ptr(), tmp.as_mut_ptr(), 1, 1, clips.as_ptr());
        }
        // refs = max(0.5, 0.95) = 0.95; (0.1-0.95)/0.95 ≈ -0.89 → max with 0 = 0
        assert_eq!(tmp[0], 0.0);
    }

    #[test]
    fn sraw_mask_positive_for_clipped_pixels() {
        // One channel exceeds the reference
        let inp = vec![2.0_f32, 0.2, 0.3, 1.0];
        let mut tmp = vec![0.0_f32; 1];
        let clips = [1.0_f32, 1.0, 1.0, 1.0];
        unsafe {
            darkroom_highlights_mask_sraw(inp.as_ptr(), tmp.as_mut_ptr(), 1, 1, clips.as_ptr());
        }
        // refs = 0.95; (2.0 - 0.95) / 0.95 ≈ 1.105
        assert!((tmp[0] - (2.0_f32 - 0.95) / 0.95).abs() < 1e-5);
    }

    #[test]
    fn sraw_mask_uses_max_channel() {
        // Multiple channels deviate; mask should pick the largest
        let inp = vec![0.6_f32, 0.7, 5.0, 1.0];
        let mut tmp = vec![0.0_f32; 1];
        let clips = [1.0_f32, 1.0, 1.0, 1.0];
        unsafe {
            darkroom_highlights_mask_sraw(inp.as_ptr(), tmp.as_mut_ptr(), 1, 1, clips.as_ptr());
        }
        let expected = (5.0_f32 - 0.95) / 0.95;
        assert!((tmp[0] - expected).abs() < 1e-5);
    }

    #[test]
    fn sraw_mask_respects_lower_bound_on_reference() {
        // Very small clip → ref clamped at 0.5
        let inp = vec![0.55_f32, 0.0, 0.0, 1.0];
        let mut tmp = vec![0.0_f32; 1];
        let clips = [0.1_f32, 0.1, 0.1, 0.1]; // 0.95*0.1 = 0.095, below 0.5
        unsafe {
            darkroom_highlights_mask_sraw(inp.as_ptr(), tmp.as_mut_ptr(), 1, 1, clips.as_ptr());
        }
        // ref clamped to 0.5; (0.55 - 0.5) / 0.5 = 0.1
        assert!((tmp[0] - 0.1).abs() < 1e-5);
    }

    #[test]
    fn mosaic_mask_uses_bayer_quadrant_clips() {
        // 2x2 RGGB Bayer image; each pixel gets its colour-specific reference
        let inp = vec![2.0_f32, 0.0, 0.0, 3.0]; // R, G, G, B
        let mut tmp = vec![0.0_f32; 4];
        let clips = [1.0_f32, 0.5, 2.0, 1.0]; // R=1, G=0.5, B=2
        let xt = [[0_u8; 6]; 6];
        let rggb: u32 = 0x94949494;
        unsafe {
            darkroom_highlights_mask_mosaic(
                inp.as_ptr(), tmp.as_mut_ptr(),
                2, 2,
                rggb, xt.as_ptr() as *const u8,
                clips.as_ptr(),
                0, 0,
            );
        }
        // refs: R = max(0.5, 0.95*1.0) = 0.95
        //       G = max(0.5, 0.95*0.5) = 0.5
        //       B = max(0.5, 0.95*2.0) = 1.9
        // (0,0)=R: (2.0-0.95)/0.95 ≈ 1.105
        // (0,1)=G: (0.0-0.5)/0.5 = -1 → 0
        // (1,0)=G: (0.0-0.5)/0.5 = -1 → 0
        // (1,1)=B: (3.0-1.9)/1.9 ≈ 0.579
        assert!((tmp[0] - (2.0_f32 - 0.95) / 0.95).abs() < 1e-5);
        assert_eq!(tmp[1], 0.0);
        assert_eq!(tmp[2], 0.0);
        assert!((tmp[3] - (3.0_f32 - 1.9) / 1.9).abs() < 1e-5);
    }
}
