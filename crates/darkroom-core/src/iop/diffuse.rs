use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Diffuse;

impl IopProcess for Diffuse {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "diffuse" }
}

/// Compute the diffuse-reconstruction mask: 1 for every pixel where at least
/// one of R, G, B exceeds the threshold, 0 otherwise.
///
/// Matches `build_mask()` in src/iop/diffuse.c. `in_buf` is an RGBA float
/// buffer of length `npixels * 4`; `mask` is a single-byte mask buffer of
/// length `npixels`.
#[no_mangle]
pub unsafe extern "C" fn darkroom_diffuse_build_mask(
    in_buf: *const f32,
    mask: *mut u8,
    npixels: usize,
    threshold: f32,
) {
    let input = std::slice::from_raw_parts(in_buf, npixels * 4);
    let mask  = std::slice::from_raw_parts_mut(mask, npixels);

    for k in 0..npixels {
        let i = k * 4;
        let hit = input[i] > threshold
               || input[i + 1] > threshold
               || input[i + 2] > threshold;
        mask[k] = if hit { 1 } else { 0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_below_threshold_produces_zero_mask() {
        let input = vec![0.1_f32, 0.2, 0.3, 1.0];
        let mut mask = vec![0xff_u8; 1];
        unsafe { darkroom_diffuse_build_mask(input.as_ptr(), mask.as_mut_ptr(), 1, 0.5); }
        assert_eq!(mask[0], 0);
    }

    #[test]
    fn any_channel_above_threshold_sets_mask() {
        // R alone exceeds the threshold
        let input = vec![0.6_f32, 0.0, 0.0, 1.0];
        let mut mask = vec![0_u8; 1];
        unsafe { darkroom_diffuse_build_mask(input.as_ptr(), mask.as_mut_ptr(), 1, 0.5); }
        assert_eq!(mask[0], 1);

        // G alone
        let input = vec![0.0_f32, 0.6, 0.0, 1.0];
        let mut mask = vec![0_u8; 1];
        unsafe { darkroom_diffuse_build_mask(input.as_ptr(), mask.as_mut_ptr(), 1, 0.5); }
        assert_eq!(mask[0], 1);

        // B alone
        let input = vec![0.0_f32, 0.0, 0.6, 1.0];
        let mut mask = vec![0_u8; 1];
        unsafe { darkroom_diffuse_build_mask(input.as_ptr(), mask.as_mut_ptr(), 1, 0.5); }
        assert_eq!(mask[0], 1);
    }

    #[test]
    fn equal_to_threshold_is_not_a_hit() {
        // Predicate is strict `>` so equality fails
        let input = vec![0.5_f32, 0.5, 0.5, 1.0];
        let mut mask = vec![0_u8; 1];
        unsafe { darkroom_diffuse_build_mask(input.as_ptr(), mask.as_mut_ptr(), 1, 0.5); }
        assert_eq!(mask[0], 0);
    }

    #[test]
    fn alpha_channel_does_not_influence_mask() {
        // High alpha but RGB below threshold → mask 0
        let input = vec![0.1_f32, 0.1, 0.1, 1.0];
        let mut mask = vec![0_u8; 1];
        unsafe { darkroom_diffuse_build_mask(input.as_ptr(), mask.as_mut_ptr(), 1, 0.5); }
        assert_eq!(mask[0], 0);
    }

    #[test]
    fn multi_pixel_mixed_result() {
        let input = vec![
            0.1, 0.1, 0.1, 1.0,  // below
            0.9, 0.1, 0.1, 1.0,  // R above
            0.1, 0.9, 0.1, 1.0,  // G above
            0.1, 0.1, 0.1, 1.0,  // below
        ];
        let mut mask = vec![0_u8; 4];
        unsafe { darkroom_diffuse_build_mask(input.as_ptr(), mask.as_mut_ptr(), 4, 0.5); }
        assert_eq!(mask, vec![0, 1, 1, 0]);
    }
}
