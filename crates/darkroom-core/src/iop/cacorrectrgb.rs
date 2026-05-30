use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Cacorrectrgb;

impl IopProcess for Cacorrectrgb {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "cacorrectrgb" }
}

/// Per-pixel manifold normalisation for the cacorrectrgb (RGB chromatic-
/// aberration correction) IOP.
///
/// For every pixel k, with weights stored in the alpha channel of the
/// `*_higher`/`*_lower` manifolds:
///   weighth = max(higher[k*4+3], 1e-2)
///   weightl = max(lower[k*4+3],  1e-2)
///   highg   = higher[k*4 + guide] / weighth     (normalise guide channel)
///   lowg    = lower[k*4 + guide]  / weightl
///   higher[k*4 + guide] = highg
///   lower[k*4 + guide]  = lowg
///   for the two non-guide channels c:
///     highc = higher[k*4+c] / weighth
///     lowc  = lower[k*4+c]  / weightl
///     higher[k*4+c] = exp2(highc) * highg
///     lower[k*4+c]  = exp2(lowc)  * lowg
///   if weighth < 0.05:
///     w = (weighth - 0.01) / 0.04
///     blend higher towards blurred_in by (1 - w)
///   ditto for weightl
///
/// Matches `normalize_manifolds()` in src/iop/cacorrectrgb.c.
/// `guide` is the index of the guide channel (0=R, 1=G, 2=B).
#[no_mangle]
pub unsafe extern "C" fn darkroom_cacorrectrgb_normalize_manifolds(
    blurred_in: *const f32,
    blurred_manifold_lower: *mut f32,
    blurred_manifold_higher: *mut f32,
    width: usize,
    height: usize,
    guide: u32,
) {
    let npx = width.saturating_mul(height);
    if npx == 0 { return; }

    // Hard guard rather than a silent clamp. Production builds reach this
    // function through the C FFI; a mis-wired call passing guide >= 3
    // would otherwise corrupt every pixel without raising a signal. Bail
    // out cleanly and leave the caller's buffers untouched.
    if guide >= 3 {
        debug_assert!(false, "guide channel must be 0..=2; got {guide}");
        return;
    }
    let guide = guide as usize;

    let inp = std::slice::from_raw_parts(blurred_in, npx * 4);
    let lo  = std::slice::from_raw_parts_mut(blurred_manifold_lower,  npx * 4);
    let hi  = std::slice::from_raw_parts_mut(blurred_manifold_higher, npx * 4);

    for k in 0..npx {
        let b = k * 4;
        let weighth = hi[b + 3].max(1e-2);
        let weightl = lo[b + 3].max(1e-2);

        // Guide channel normalisation
        let highg = hi[b + guide] / weighth;
        let lowg  = lo[b + guide] / weightl;
        hi[b + guide] = highg;
        lo[b + guide] = lowg;

        // Non-guide channels: normalise then unlog (multiply by guide).
        // `exp2f` in C and `f32::exp2` in Rust both lower to the same libm
        // primitive on x86_64/glibc but bit-equality across compilers /
        // optimisation levels is not guaranteed. Downstream regression
        // tests against the C reference should allow sub-LSB tolerance.
        for kc in 0..=1 {
            let c = (kc + guide + 1) % 3;
            let highc = hi[b + c] / weighth;
            let lowc  = lo[b + c] / weightl;
            hi[b + c] = highc.exp2() * highg;
            lo[b + c] = lowc.exp2()  * lowg;
        }

        // Low-confidence smooth blend toward blurred_in. The C source uses
        // `for_each_channel(c, …)` which expands to a 0..DT_PIXEL_SIMD_CHANNELS
        // loop — 4 in the default vectorised build, 3 only when
        // `DT_NO_VECTORIZATION` is defined. The production binary blends all
        // four channels (alpha included); this is a side-effect of the SIMD
        // hint, not a deliberate design choice, but it IS the actual
        // production behaviour we are matching here. If a future
        // non-vectorised build path becomes the canonical one, swap to 0..3.
        if weighth < 0.05 {
            let w = (weighth - 0.01) / (0.05 - 0.01);
            for c in 0..4 {
                hi[b + c] = w * hi[b + c] + (1.0 - w) * inp[b + c];
            }
        }
        if weightl < 0.05 {
            let w = (weightl - 0.01) / (0.05 - 0.01);
            for c in 0..4 {
                lo[b + c] = w * lo[b + c] + (1.0 - w) * inp[b + c];
            }
        }
    }

    // NB: the C source wraps the outer loop in DT_OMP_FOR. The Rust port
    // is single-threaded today; rayon::par_chunks_exact_mut is the right
    // tool once the FFI surface stabilises and we want the throughput back.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a 1-pixel input + manifolds with the supplied weights and
    /// channel values (guide channel set to log2(2) = 1.0 for both manifolds).
    fn one_pixel(
        in_rgb: [f32; 4],
        lower_rgba: [f32; 4],
        higher_rgba: [f32; 4],
    ) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        (in_rgb.to_vec(), lower_rgba.to_vec(), higher_rgba.to_vec())
    }

    #[test]
    fn high_weight_passes_normalisation_through() {
        // weight = 1.0 (alpha) → no smooth blend triggered (0.05 cutoff).
        // guide = 1 (G), guide-channel value pre-divided = 0.5 / 1.0 = 0.5.
        // Non-guide channels: input value 0.0 → exp2(0) * 0.5 = 0.5.
        let (inp, mut lo, mut hi) =
            one_pixel([0.1, 0.2, 0.3, 0.42], [0.0, 0.5, 0.0, 1.0], [0.0, 0.5, 0.0, 1.0]);
        unsafe {
            darkroom_cacorrectrgb_normalize_manifolds(
                inp.as_ptr(), lo.as_mut_ptr(), hi.as_mut_ptr(),
                1, 1, 1, // guide = G
            );
        }
        // Both manifolds: R = 0.5, G = 0.5, B = 0.5
        for v in &[hi[0], hi[1], hi[2], lo[0], lo[1], lo[2]] {
            assert!((v - 0.5).abs() < 1e-5, "got {}", v);
        }
    }

    #[test]
    fn low_weight_blends_toward_input() {
        // weight = 0.01 → blend factor w = 0 → fully replaced by blurred_in.
        // Non-guide channels start at 0 so exp2(0/0.01) = exp2(0) = 1, which
        // keeps the intermediate finite; with weighth = 0.01 the production C
        // path would also produce `inf` if the non-guide were left at 99
        // (exp2(9900) overflows). The test deliberately uses values inside
        // the well-defined regime.
        let (inp, mut lo, mut hi) =
            one_pixel(
                [0.10, 0.20, 0.30, 0.40],
                [0.5,  0.0,  0.0,  0.01],
                [0.5,  0.0,  0.0,  0.01],
            );
        unsafe {
            darkroom_cacorrectrgb_normalize_manifolds(
                inp.as_ptr(), lo.as_mut_ptr(), hi.as_mut_ptr(),
                1, 1, 0, // guide = R
            );
        }
        // After blend at w=0, both manifolds equal `inp` exactly on RGB + alpha.
        for c in 0..4 {
            assert!((hi[c] - inp[c]).abs() < 1e-5, "hi[{c}] = {}, inp = {}", hi[c], inp[c]);
            assert!((lo[c] - inp[c]).abs() < 1e-5, "lo[{c}] = {}, inp = {}", lo[c], inp[c]);
        }
    }

    #[test]
    fn weight_below_minimum_clamps_to_one_hundredth() {
        // Weight = 0.0 → clamped to 1e-2 before division → guide /= 1e-2 = 100.
        let (inp, mut lo, mut hi) =
            one_pixel([0.0; 4], [0.0, 1.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]);
        unsafe {
            darkroom_cacorrectrgb_normalize_manifolds(
                inp.as_ptr(), lo.as_mut_ptr(), hi.as_mut_ptr(),
                1, 1, 1,
            );
        }
        // weighth=weightl=1e-2, so guide value = 1.0/0.01 = 100. But then
        // weight < 0.05 triggers blend toward input (w = -0.25 here because
        // (0.01 - 0.01) / (0.05 - 0.01) = 0). Verify the blend ran.
        // w = (0.01 - 0.01) / 0.04 = 0 → fully replaced by input (all zeros).
        for c in 0..4 {
            assert_eq!(hi[c], 0.0, "hi[{c}] = {}", hi[c]);
            assert_eq!(lo[c], 0.0, "lo[{c}] = {}", lo[c]);
        }
    }

    #[test]
    fn guide_channel_two_blue_round_trip() {
        // Sanity check with guide = B (index 2).
        let (inp, mut lo, mut hi) = one_pixel(
            [0.1, 0.2, 0.3, 0.4],
            [0.0, 0.0, 0.5, 1.0],
            [0.0, 0.0, 0.5, 1.0],
        );
        unsafe {
            darkroom_cacorrectrgb_normalize_manifolds(
                inp.as_ptr(), lo.as_mut_ptr(), hi.as_mut_ptr(),
                1, 1, 2,
            );
        }
        // Blue = 0.5, R = exp2(0) * 0.5 = 0.5, G = exp2(0) * 0.5 = 0.5
        for c in 0..3 {
            assert!((hi[c] - 0.5).abs() < 1e-5);
            assert!((lo[c] - 0.5).abs() < 1e-5);
        }
    }

    #[test]
    fn out_of_range_guide_returns_without_touching_buffers() {
        // guide >= 3 is a wiring bug. The function must leave both manifolds
        // unchanged so a C caller can see the corrupted output rather than
        // silently processing as if `guide` were 2.
        let inp = vec![0.0_f32; 4];
        let mut lo = vec![1.0_f32, 2.0, 3.0, 4.0];
        let mut hi = vec![5.0_f32, 6.0, 7.0, 8.0];
        let lo_orig = lo.clone();
        let hi_orig = hi.clone();
        // Wrap in catch_unwind to swallow the debug_assert! panic that fires
        // in debug builds but not release.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            unsafe {
                darkroom_cacorrectrgb_normalize_manifolds(
                    inp.as_ptr(), lo.as_mut_ptr(), hi.as_mut_ptr(),
                    1, 1, 7, // out of range
                );
            }
        }));
        assert_eq!(lo, lo_orig);
        assert_eq!(hi, hi_orig);
    }

    #[test]
    fn zero_size_is_safe_noop() {
        let inp = vec![0.0_f32; 4];
        let mut lo = vec![0.0_f32; 4];
        let mut hi = vec![0.0_f32; 4];
        unsafe {
            darkroom_cacorrectrgb_normalize_manifolds(
                inp.as_ptr(), lo.as_mut_ptr(), hi.as_mut_ptr(),
                0, 0, 1,
            );
        }
        // Untouched.
        for v in inp.iter().chain(lo.iter()).chain(hi.iter()) { assert_eq!(*v, 0.0); }
    }
}
