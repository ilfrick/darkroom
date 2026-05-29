use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Hotpixels;

impl IopProcess for Hotpixels {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "hotpixels" }
}

/// Bayer-sensor hot-pixel correction.
///
/// For every interior pixel (row, col) in 2..h-2 x 2..w-2 of the single-plane
/// raw buffer:
///   * If `input[k] > threshold`, examine the four same-color Bayer neighbours
///     at offsets (-2,0), (+2,0), (0,-2), (0,+2).
///   * Let `mid = input[k] * multiplier`. Count how many neighbours satisfy
///     `mid > neighbour`, and track the maximum of those neighbours.
///   * If `count >= min_neighbours`, replace `output[k]` with that maximum
///     value and increment the returned `fixed` counter.
///
/// `mark_fixed` is the UI debug overlay: when enabled, the corrected pixel
/// stamps its original value into the same row at column offsets ±2, ±4 …
/// ±10 (same Bayer colour positions) so the user can see where corrections
/// happened. Matches the markfixed branch in src/iop/hotpixels.c exactly.
///
/// Returns the count of pixels that were replaced.
#[no_mangle]
pub unsafe extern "C" fn darkroom_hotpixels_bayer(
    in_buf: *const f32,
    out_buf: *mut f32,
    width: usize,
    height: usize,
    threshold: f32,
    multiplier: f32,
    min_neighbours: i32,
    mark_fixed: i32,
) -> i32 {
    let mark_fixed = mark_fixed != 0;
    if width < 5 || height < 5 {
        return 0;
    }
    let n = width * height;
    let input = std::slice::from_raw_parts(in_buf, n);
    let output = std::slice::from_raw_parts_mut(out_buf, n);
    let width_x2 = width * 2;
    let mut fixed: i32 = 0;

    for row in 2..(height - 2) {
        let row_start = row * width;
        for col in 2..(width - 2) {
            let k = row_start + col;
            let v = input[k];
            if v <= threshold { continue; }

            let mid = v * multiplier;
            let mut count = 0;
            let mut maxin = 0.0_f32;

            // four same-colour Bayer neighbours
            let neighbours = [
                input[k - 2],
                input[k - width_x2],
                input[k + 2],
                input[k + width_x2],
            ];
            for &other in &neighbours {
                if mid > other {
                    count += 1;
                    if other > maxin { maxin = other; }
                }
            }

            if count >= min_neighbours {
                output[k] = maxin;
                fixed += 1;

                if mark_fixed {
                    // Stamp original value at column offsets -10..-2 (step 2)
                    let mut i: isize = -2;
                    while i >= -10 && (col as isize + i) >= 0 {
                        output[(k as isize + i) as usize] = v;
                        i -= 2;
                    }
                    // and +2..+10 (step 2)
                    let mut i: isize = 2;
                    while i <= 10 && (col as isize + i) < width as isize {
                        output[(k as isize + i) as usize] = v;
                        i += 2;
                    }
                }
            }
        }
    }

    fixed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(width: usize, height: usize, val: f32) -> Vec<f32> {
        vec![val; width * height]
    }

    #[test]
    fn no_pixels_above_threshold_no_change() {
        let w = 8; let h = 8;
        let input = flat(w, h, 0.1);
        let mut output = input.clone();
        let n = unsafe {
            darkroom_hotpixels_bayer(
                input.as_ptr(), output.as_mut_ptr(),
                w, h, 0.5, 0.5, 4, 0,
            )
        };
        assert_eq!(n, 0);
        assert_eq!(output, input);
    }

    #[test]
    fn isolated_hot_pixel_gets_replaced() {
        let w = 8; let h = 8;
        let mut input = flat(w, h, 0.1);
        // Hot pixel at (4, 4) — well above threshold and much brighter than its
        // four same-colour Bayer neighbours (all 0.1).
        let k = 4 * w + 4;
        input[k] = 10.0;
        let mut output = input.clone();
        let n = unsafe {
            darkroom_hotpixels_bayer(
                input.as_ptr(), output.as_mut_ptr(),
                w, h, 1.0, 0.5, 4, 0,
            )
        };
        assert_eq!(n, 1, "expected exactly one fix");
        // mid = 10*0.5 = 5; all neighbours = 0.1 → max = 0.1
        assert!((output[k] - 0.1).abs() < 1e-6);
    }

    #[test]
    fn fewer_than_min_neighbours_keeps_pixel() {
        let w = 8; let h = 8;
        let mut input = flat(w, h, 0.1);
        let k = 4 * w + 4;
        input[k] = 10.0;
        // Also raise three neighbours above mid so only ONE neighbour satisfies
        // mid > other. With min_neighbours = 4, no fix.
        input[k - 2] = 9.0;
        input[k - 16] = 9.0; // width*2
        input[k + 2] = 9.0;
        let mut output = input.clone();
        let n = unsafe {
            darkroom_hotpixels_bayer(
                input.as_ptr(), output.as_mut_ptr(),
                w, h, 1.0, 0.5, 4, 0,
            )
        };
        assert_eq!(n, 0);
        assert_eq!(output[k], 10.0);
    }

    #[test]
    fn permissive_min_three_still_fixes_clean_hot_pixel() {
        // With a clean isolated hot pixel (all 4 same-colour neighbours below
        // mid), min_neighbours = 3 must also fix it — the permissive flag
        // only changes whether 3-of-4 is good enough; a 4-of-4 case is always
        // good enough regardless of the flag.
        let w = 8; let h = 8;
        let mut input = flat(w, h, 0.1);
        let k = 4 * w + 4;
        input[k] = 10.0;
        let mut output = input.clone();
        let n = unsafe {
            darkroom_hotpixels_bayer(
                input.as_ptr(), output.as_mut_ptr(),
                w, h, 1.0, 0.5, 3, 0,
            )
        };
        assert_eq!(n, 1);
    }

    #[test]
    fn border_pixels_are_skipped() {
        let w = 8; let h = 8;
        let mut input = flat(w, h, 0.1);
        // Hot pixel in the border (row=0) — must not be touched (out of range).
        input[0] = 100.0;
        let mut output = input.clone();
        let n = unsafe {
            darkroom_hotpixels_bayer(
                input.as_ptr(), output.as_mut_ptr(),
                w, h, 1.0, 0.5, 4, 0,
            )
        };
        assert_eq!(n, 0);
        assert_eq!(output[0], 100.0);
    }

    #[test]
    fn mark_fixed_stamps_original_value_in_row() {
        // Need col > 10 and width - col > 10 so both ±10 markers fit, matching
        // the boundary checks in the original C code (`i >= -col`, `i < width-col`).
        let w = 24; let h = 16;
        let row = 8;
        let col = 12;
        let mut input = flat(w, h, 0.1);
        let k = row * w + col;
        input[k] = 10.0;
        let mut output = input.clone();
        let n = unsafe {
            darkroom_hotpixels_bayer(
                input.as_ptr(), output.as_mut_ptr(),
                w, h, 1.0, 0.5, 4, 1, // mark_fixed = true
            )
        };
        assert_eq!(n, 1);
        // Centre replaced with neighbours' max
        assert!((output[k] - 0.1).abs() < 1e-6);
        // Same-row marker stamps at col offsets ±2, ±4, ±6, ±8, ±10 (step 2)
        for off in [-10_isize, -8, -6, -4, -2, 2, 4, 6, 8, 10].iter() {
            let idx = (k as isize + off) as usize;
            assert!((output[idx] - 10.0).abs() < 1e-6,
                    "marker missing at offset {off}: got {}", output[idx]);
        }
    }

    #[test]
    fn returns_count_of_fixes() {
        let w = 16; let h = 16;
        let mut input = flat(w, h, 0.1);
        // Two hot pixels spaced widely so their marker regions don't overlap.
        input[3 * w + 3] = 10.0;
        input[12 * w + 12] = 10.0;
        let mut output = input.clone();
        let n = unsafe {
            darkroom_hotpixels_bayer(
                input.as_ptr(), output.as_mut_ptr(),
                w, h, 1.0, 0.5, 4, 0,
            )
        };
        assert_eq!(n, 2);
    }
}
