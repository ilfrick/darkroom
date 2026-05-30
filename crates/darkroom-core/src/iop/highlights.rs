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

/// CLIP mode for the sRAW path: simple per-component clamp to `clip`.
///
///   out[k] = min(clip, in[k])  for every float in the buffer.
///
/// Matches the `ch == 4` branch of process_clip() in src/iop/highlights.c.
/// `nfloats` is the total number of floats (npixels * 4 for RGBA).
#[no_mangle]
pub unsafe extern "C" fn darkroom_highlights_clip_sraw(
    in_buf: *const f32,
    out_buf: *mut f32,
    nfloats: usize,
    clip: f32,
) {
    if nfloats == 0 { return; }
    let input = std::slice::from_raw_parts(in_buf, nfloats);
    let output = std::slice::from_raw_parts_mut(out_buf, nfloats);
    for k in 0..nfloats {
        let v = input[k];
        // Match C `fminf(clip, v)` IEEE-754 Annex F NaN semantics: if exactly
        // one operand is NaN, return the non-NaN one; if both are NaN, return
        // NaN. Rust's `f32::min` gets this right when the receiver is NaN but
        // diverges for the case `clip.is_nan() && !v.is_nan()` (it returns
        // clip, fminf returns v). Explicit decomposition keeps us bit-for-bit
        // identical to the C path.
        output[k] = if v.is_nan() { clip }
                    else if clip.is_nan() { v }
                    else { v.min(clip) };
    }
}

/// Visualise clipping on a sRAW (RGBA) buffer.
///
/// For every pixel k, c in 0..3:
///   out[k+c] = (in[k+c] < clips[c]) ? 0.2 * in[k+c] : 1.0
///   out[k+3] = 0.0
///
/// Matches the `filters == 0` branch of process_visualize() in
/// src/iop/highlights.c.
#[no_mangle]
pub unsafe extern "C" fn darkroom_highlights_visualize_sraw(
    in_buf: *const f32,
    out_buf: *mut f32,
    npixels: usize,
    clips: *const f32, // 4 floats
) {
    if npixels == 0 { return; }
    let input = std::slice::from_raw_parts(in_buf, npixels * 4);
    let output = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    let clips = std::slice::from_raw_parts(clips, 4);

    for k in 0..npixels {
        let j = k * 4;
        // The C source uses `for_each_channel(c)` (which iterates 0..3 on this
        // platform — `DT_PIXEL_SIMD_CHANNELS = 4`) and then overrides
        // `out[k+3] = 0.0f`. Iterating 0..3 explicitly here and writing 0.0 to
        // index 3 yields the same final values; if `for_each_channel` ever
        // grows a 5-channel variant, this loop must be revisited.
        for c in 0..3 {
            let v = input[j + c];
            output[j + c] = if v < clips[c] { 0.2 * v } else { 1.0 };
        }
        output[j + 3] = 0.0;
    }
}

/// Visualise clipping on a single-plane mosaic (Bayer / X-Trans) buffer.
///
/// For every output pixel (row, col):
///   irow = row + irow_offset   // = roi_out.y - roi_in.y
///   icol = col + icol_offset   // = roi_out.x - roi_in.x
///   if (irow, icol) is in [0, input_height) x [0, input_width):
///     c = fcol(irow, icol, filters, xtrans)
///     v = in[irow * input_width + icol]
///     out[k] = (v < clips[c]) ? 0.2 * v : 1.0
///   else:
///     out[k] = 0.0
///
/// `xtrans` is a flat 36-byte 6x6 pattern; read only when filters==9.
/// Matches the `filters != 0` branch of process_visualize() in
/// src/iop/highlights.c.
#[no_mangle]
pub unsafe extern "C" fn darkroom_highlights_visualize_mosaic(
    in_buf: *const f32,
    out_buf: *mut f32,
    out_width: usize,
    out_height: usize,
    in_width: usize,
    in_height: usize,
    filters: u32,
    xtrans: *const u8,
    clips: *const f32, // 4 floats
    irow_offset: i32,
    icol_offset: i32,
) {
    if out_width == 0 || out_height == 0 { return; }
    let in_total = in_width.saturating_mul(in_height);
    if in_total == 0 { return; }

    let input = std::slice::from_raw_parts(in_buf, in_total);
    let output = std::slice::from_raw_parts_mut(out_buf, out_width * out_height);
    let clips = std::slice::from_raw_parts(clips, 4);

    let xt_bytes = std::slice::from_raw_parts(xtrans, 36);
    let mut xt = [[0_u8; 6]; 6];
    for r in 0..6 {
        for c in 0..6 { xt[r][c] = xt_bytes[r * 6 + c]; }
    }

    // Width/height arrive as `usize`; the bounds check has to compare against
    // signed i32 because the irow/icol can go negative under non-trivial
    // roi offsets. Assert the dimensions fit so the cast is lossless — a
    // silent wrap would make the bounds check trivially false and zero the
    // entire output without warning.
    let in_w_i = i32::try_from(in_width).expect("in_width exceeds i32::MAX");
    let in_h_i = i32::try_from(in_height).expect("in_height exceeds i32::MAX");

    for row in 0..out_height {
        for col in 0..out_width {
            let ox = row * out_width + col;
            let irow = row as i32 + irow_offset;
            let icol = col as i32 + icol_offset;
            if icol >= 0 && irow >= 0 && irow < in_h_i && icol < in_w_i {
                let ix = (irow as usize) * in_width + (icol as usize);
                let c = crate::raw::fcol(irow, icol, filters, &xt);
                let v = input[ix];
                output[ox] = if v < clips[c] { 0.2 * v } else { 1.0 };
            } else {
                output[ox] = 0.0;
            }
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
    fn clip_sraw_clamps_each_float_independently() {
        let inp = vec![0.5_f32, 1.5, 0.7, 0.9, 2.0, 0.0, 0.6, 1.2];
        let mut out = vec![0.0_f32; inp.len()];
        unsafe { darkroom_highlights_clip_sraw(inp.as_ptr(), out.as_mut_ptr(), inp.len(), 1.0); }
        let expected = vec![0.5_f32, 1.0, 0.7, 0.9, 1.0, 0.0, 0.6, 1.0];
        assert_eq!(out, expected);
    }

    #[test]
    fn visualize_sraw_marks_unclipped_as_dim_clipped_as_white() {
        let inp = vec![0.5_f32, 0.7, 0.9, 0.42, 1.5, 0.2, 2.0, 1.0];
        let mut out = vec![-1.0_f32; inp.len()];
        let clips = [1.0_f32; 4];
        unsafe {
            darkroom_highlights_visualize_sraw(
                inp.as_ptr(), out.as_mut_ptr(), 2, clips.as_ptr(),
            );
        }
        // pixel 0: all RGB below clip → 0.2 * v; alpha forced to 0
        assert!((out[0] - 0.1).abs() < 1e-6);
        assert!((out[1] - 0.14).abs() < 1e-6);
        assert!((out[2] - 0.18).abs() < 1e-6);
        assert_eq!(out[3], 0.0);
        // pixel 1: R=1.5 → 1.0, G=0.2 → 0.04, B=2.0 → 1.0; alpha 0
        assert_eq!(out[4], 1.0);
        assert!((out[5] - 0.04).abs() < 1e-6);
        assert_eq!(out[6], 1.0);
        assert_eq!(out[7], 0.0);
    }

    #[test]
    fn visualize_mosaic_handles_out_of_bounds_pixels() {
        // 2x2 output, in is also 2x2; with irow_offset = -1 the first row of
        // output reaches into the negative-row region, which must yield 0.
        let inp = vec![2.0_f32, 0.5, 0.5, 2.0];
        let mut out = vec![-7.0_f32; 4];
        let clips = [1.0_f32; 4];
        let xt = [[0_u8; 6]; 6];
        unsafe {
            darkroom_highlights_visualize_mosaic(
                inp.as_ptr(), out.as_mut_ptr(),
                2, 2, 2, 2,
                0x94949494, xt.as_ptr() as *const u8,
                clips.as_ptr(),
                -1, 0, // shift output one row up
            );
        }
        // out row 0 (maps to in row -1) → both 0.0
        assert_eq!(out[0], 0.0);
        assert_eq!(out[1], 0.0);
        // out row 1 (maps to in row 0): in[0]=2.0 ≥ clip → 1.0; in[1]=0.5 < clip → 0.1
        assert_eq!(out[2], 1.0);
        assert!((out[3] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn visualize_mosaic_handles_xtrans_pattern() {
        // Real Fujifilm X-Trans 6x6 (0=R, 1=G, 2=B). filters == 9 routes
        // through raw::fc_xtrans rather than fc_bayer.
        let xt_pattern: [[u8; 6]; 6] = [
            [1, 2, 1, 1, 0, 1],
            [0, 1, 0, 2, 1, 2],
            [1, 2, 1, 1, 0, 1],
            [1, 0, 1, 1, 2, 1],
            [2, 1, 2, 0, 1, 0],
            [1, 0, 1, 1, 2, 1],
        ];
        let xt_flat: Vec<u8> = xt_pattern.iter().flatten().copied().collect();
        // Single 1×1 input — pixel at (0,0) is colour 1 (G).
        let inp = vec![0.5_f32];
        let mut out = vec![-1.0_f32; 1];
        // Per-colour clips: R=1.0, G=0.4, B=1.0, alpha=1.0
        let clips = [1.0_f32, 0.4, 1.0, 1.0];
        unsafe {
            darkroom_highlights_visualize_mosaic(
                inp.as_ptr(), out.as_mut_ptr(),
                1, 1, 1, 1,
                9, // filters == 9 → X-Trans branch
                xt_flat.as_ptr(),
                clips.as_ptr(),
                0, 0,
            );
        }
        // 0.5 > clip_G=0.4 → 1.0
        assert_eq!(out[0], 1.0);
    }

    #[test]
    fn clip_sraw_matches_fminf_nan_semantics() {
        // C fminf(clip, NaN) returns clip (non-NaN). Rust's f32::min returns
        // clip too. Verify the wrapper does the same in both orderings.
        let inp = vec![f32::NAN, 0.5];
        let mut out = vec![-1.0_f32; 2];
        unsafe { darkroom_highlights_clip_sraw(inp.as_ptr(), out.as_mut_ptr(), 2, 0.8); }
        // out[0] = fminf(0.8, NaN) = 0.8
        assert_eq!(out[0], 0.8);
        // out[1] = fminf(0.8, 0.5) = 0.5
        assert_eq!(out[1], 0.5);

        // fminf(NaN, value) returns value; Rust f32::min disagrees here.
        let inp = vec![0.5_f32];
        let mut out = vec![-1.0_f32; 1];
        unsafe { darkroom_highlights_clip_sraw(inp.as_ptr(), out.as_mut_ptr(), 1, f32::NAN); }
        // C: fminf(NaN, 0.5) = 0.5. Our wrapper must return 0.5, not NaN.
        assert_eq!(out[0], 0.5);
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
