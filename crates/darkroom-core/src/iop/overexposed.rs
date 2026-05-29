use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Overexposed;

impl IopProcess for Overexposed {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "overexposed" }
}

/// Highlight clipped pixels with a tint colour, for the per-channel
/// "any RGB" preview mode of the overexposed IOP.
///
/// For each pixel k:
///   if any of R, G, B in img_tmp >= upper        → out[k] = upper_color
///   else if R, G, B in img_tmp all <= lower      → out[k] = lower_color
///   else                                         → out[k] = in[k]
///
/// `in`, `out`, `img_tmp` are tightly-packed RGBA float buffers of length
/// `npixels * 4`. `upper_color` and `lower_color` are 4-float arrays.
///
/// Matches the DT_CLIPPING_PREVIEW_ANYRGB branch in
/// src/iop/overexposed.c (process()).
#[no_mangle]
pub unsafe extern "C" fn darkroom_overexposed_anyrgb(
    in_buf: *const f32,
    out_buf: *mut f32,
    img_tmp: *const f32,
    npixels: usize,
    upper: f32,
    lower: f32,
    upper_color: *const f32,
    lower_color: *const f32,
) {
    let inp = std::slice::from_raw_parts(in_buf, npixels * 4);
    let out = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    let tmp = std::slice::from_raw_parts(img_tmp, npixels * 4);
    let upc = std::slice::from_raw_parts(upper_color, 4);
    let loc = std::slice::from_raw_parts(lower_color, 4);

    for k in 0..npixels {
        let i = k * 4;
        let r = tmp[i];
        let g = tmp[i + 1];
        let b = tmp[i + 2];

        if r >= upper || g >= upper || b >= upper {
            out[i..i + 4].copy_from_slice(upc);
        } else if r <= lower && g <= lower && b <= lower {
            out[i..i + 4].copy_from_slice(loc);
        } else {
            out[i..i + 4].copy_from_slice(&inp[i..i + 4]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_clip_paints_upper_color() {
        let inp = vec![0.0_f32; 4];
        let tmp = vec![1.5_f32, 0.0, 0.0, 1.0]; // R out of bounds
        let mut out = vec![-1.0_f32; 4];
        let upc = [0.9_f32, 0.0, 0.0, 1.0];
        let loc = [0.0_f32, 0.0, 0.9, 1.0];
        unsafe {
            darkroom_overexposed_anyrgb(
                inp.as_ptr(), out.as_mut_ptr(), tmp.as_ptr(), 1,
                1.0, 0.01, upc.as_ptr(), loc.as_ptr(),
            );
        }
        assert_eq!(&out[..], &upc[..]);
    }

    #[test]
    fn lower_clip_paints_lower_color() {
        let inp = vec![0.5_f32, 0.5, 0.5, 1.0];
        let tmp = vec![0.001_f32, 0.001, 0.001, 1.0];
        let mut out = vec![-1.0_f32; 4];
        let upc = [0.9_f32, 0.0, 0.0, 1.0];
        let loc = [0.0_f32, 0.0, 0.9, 1.0];
        unsafe {
            darkroom_overexposed_anyrgb(
                inp.as_ptr(), out.as_mut_ptr(), tmp.as_ptr(), 1,
                1.0, 0.01, upc.as_ptr(), loc.as_ptr(),
            );
        }
        assert_eq!(&out[..], &loc[..]);
    }

    #[test]
    fn unclipped_pixel_uses_in_buf() {
        let inp = vec![0.3_f32, 0.4, 0.5, 0.9];
        let tmp = vec![0.5_f32, 0.5, 0.5, 1.0];
        let mut out = vec![-1.0_f32; 4];
        let upc = [0.9_f32, 0.0, 0.0, 1.0];
        let loc = [0.0_f32, 0.0, 0.9, 1.0];
        unsafe {
            darkroom_overexposed_anyrgb(
                inp.as_ptr(), out.as_mut_ptr(), tmp.as_ptr(), 1,
                1.0, 0.01, upc.as_ptr(), loc.as_ptr(),
            );
        }
        assert_eq!(&out[..], &inp[..]);
    }

    #[test]
    fn upper_takes_priority_over_lower() {
        // R is huge, G & B are tiny — upper triggers first (matches C ordering)
        let inp = vec![0.5_f32; 4];
        let tmp = vec![2.0_f32, 0.0, 0.0, 1.0];
        let mut out = vec![-1.0_f32; 4];
        let upc = [1.0_f32, 0.0, 0.0, 1.0];
        let loc = [0.0_f32, 0.0, 1.0, 1.0];
        unsafe {
            darkroom_overexposed_anyrgb(
                inp.as_ptr(), out.as_mut_ptr(), tmp.as_ptr(), 1,
                1.0, 0.5, upc.as_ptr(), loc.as_ptr(),
            );
        }
        assert_eq!(&out[..], &upc[..]);
    }

    #[test]
    fn multi_pixel_mixed_outcomes() {
        // 3 pixels: clip-upper, unclipped, clip-lower
        let inp = vec![
            0.0, 0.0, 0.0, 1.0,
            0.5, 0.5, 0.5, 1.0,
            0.5, 0.5, 0.5, 1.0,
        ];
        let tmp = vec![
            1.5, 0.0, 0.0, 1.0,
            0.5, 0.5, 0.5, 1.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        let mut out = vec![-1.0_f32; 12];
        let upc = [1.0_f32, 0.1, 0.1, 1.0];
        let loc = [0.1_f32, 0.1, 1.0, 1.0];
        unsafe {
            darkroom_overexposed_anyrgb(
                inp.as_ptr(), out.as_mut_ptr(), tmp.as_ptr(), 3,
                1.0, 0.01, upc.as_ptr(), loc.as_ptr(),
            );
        }
        assert_eq!(&out[0..4], &upc[..]);
        assert_eq!(&out[4..8], &inp[4..8]);
        assert_eq!(&out[8..12], &loc[..]);
    }
}
