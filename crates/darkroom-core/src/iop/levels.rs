//! Levels IOP — black/white-point + gamma correction via pre-computed LUT.
//!
//! Replaces the OMP loop in src/iop/levels.c::process().
//!
//! Per-pixel algorithm (Lab input, 4 channels):
//!   L_in  = in.L / 100
//!   if L_in ≤ level_black: L_out = 0
//!   else:
//!     pct   = (L_in - level_black) / level_range
//!     L_out = pct < 1 ? lut[(pct * 65536) as usize] : 100 * pct^inv_gamma
//!   denom = max(in.L, 0.01)
//!   out.L = L_out
//!   out.a = in.a * L_out / denom   (contrast-preserving chroma scale)
//!   out.b = in.b * L_out / denom
//!   out.α = in.α

use crate::{
    iop::{ClBuffer, IopProcess},
    params::IopParams,
    roi::RoiIn,
    Error, Result,
};

// ── IopProcess impl ───────────────────────────────────────────────────────────

pub struct Levels;

impl IopProcess for Levels {
    fn name(&self) -> &'static str {
        "levels"
    }

    fn process(
        &self,
        _input: &[f32],
        _output: &mut [f32],
        _params: &IopParams,
        _roi: &RoiIn,
    ) -> Result<()> {
        // The 65536-entry LUT lives inside dt_iop_levels_data_t and cannot be
        // trivially cast from IopParams bytes. Use the C FFI entry point instead.
        Err(Error::Pipeline(
            "levels: use the C FFI entry point (LUT cannot be cast from raw params)".into(),
        ))
    }

    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(Error::OpenCl("levels: OpenCL path not yet ported".into()))
    }
}

// ── Core pixel loop ───────────────────────────────────────────────────────────

/// `lut` must have exactly 65536 entries.
#[inline]
pub fn process_pixels(
    input: &[f32],
    output: &mut [f32],
    level_black: f32,
    level_range: f32,
    inv_gamma: f32,
    lut: &[f32; 65536],
) {
    for (chunk_in, chunk_out) in input.chunks_exact(4).zip(output.chunks_exact_mut(4)) {
        let l_in = chunk_in[0] / 100.0;
        let l_out = if l_in <= level_black {
            0.0_f32
        } else {
            let pct = (l_in - level_black) / level_range;
            if pct < 1.0 {
                let idx = (pct * 65536.0) as usize;
                lut[idx.min(65535)]
            } else {
                100.0 * pct.powf(inv_gamma)
            }
        };

        let denom = chunk_in[0].max(0.01);
        chunk_out[0] = l_out;
        chunk_out[1] = chunk_in[1] * l_out / denom;
        chunk_out[2] = chunk_in[2] * l_out / denom;
        chunk_out[3] = chunk_in[3];
    }
}

// ── C FFI entry point ─────────────────────────────────────────────────────────

/// Called from src/iop/levels.c in place of the OMP loop.
///
/// `lut` points to `dt_iop_levels_data_t.lut` (65536 floats).
/// `level_range` is `d->levels[2] - d->levels[0]`, pre-computed in the C wrapper.
///
/// # Safety
/// All pointer arguments must be valid for the duration of this call.
/// `lut` must point to an array of at least 65536 floats.
#[no_mangle]
pub unsafe extern "C" fn darkroom_levels_process(
    in_buf: *const f32,
    out_buf: *mut f32,
    npixels: usize,
    level_black: f32,
    level_range: f32,
    inv_gamma: f32,
    lut: *const f32,
) {
    let input = std::slice::from_raw_parts(in_buf, npixels * 4);
    let output = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    let lut_arr: &[f32; 65536] = &*(lut as *const [f32; 65536]);
    process_pixels(input, output, level_black, level_range, inv_gamma, lut_arr);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_lut() -> Box<[f32; 65536]> {
        // identity: percentage pct → 100 * pct^1.0 = 100 * pct
        // Since lut[i] = 100 * i / 65536
        let mut t = Box::new([0.0f32; 65536]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = 100.0 * i as f32 / 65536.0;
        }
        t
    }

    #[test]
    fn below_black_point_zeroes_l() {
        let lut = identity_lut();
        // level_black=0.5 → any L_in/100 ≤ 0.5 clips to 0
        let input = vec![40.0f32, 10.0, -5.0, 1.0]; // L_in = 0.4 < level_black=0.5
        let mut output = vec![0.0f32; 4];
        process_pixels(&input, &mut output, 0.5, 0.5, 1.0, &lut);
        assert!(output[0].abs() < 1e-6, "L should be 0 below black point: {}", output[0]);
    }

    #[test]
    fn identity_params_approximate_passthrough() {
        let lut = identity_lut();
        // level_black=0, level_range=1, inv_gamma=1 → L_out ≈ L_in (LUT quantization ~0.15)
        let input = vec![60.0f32, 20.0, -10.0, 1.0];
        let mut output = vec![0.0f32; 4];
        process_pixels(&input, &mut output, 0.0, 1.0, 1.0, &lut);
        assert!((output[0] - 60.0).abs() < 0.2, "L round-trip: {}", output[0]);
    }

    #[test]
    fn ab_scale_proportional_to_l() {
        let lut = identity_lut();
        // L=50 in → L_out ≈ 50; denom=50; out.a = 10 * 50/50 = 10
        let input = vec![50.0f32, 10.0, -5.0, 1.0];
        let mut output = vec![0.0f32; 4];
        process_pixels(&input, &mut output, 0.0, 1.0, 1.0, &lut);
        // out.a/in.a ≈ out.L/in.L
        let expected_a = input[1] * output[0] / input[0];
        assert!((output[1] - expected_a).abs() < 0.1, "a scaling: {}", output[1]);
    }

    #[test]
    fn alpha_passes_through() {
        let lut = identity_lut();
        let input = vec![50.0f32, 10.0, -5.0, 0.75];
        let mut output = vec![0.0f32; 4];
        process_pixels(&input, &mut output, 0.0, 1.0, 1.0, &lut);
        assert!((output[3] - 0.75).abs() < 1e-7);
    }

    #[test]
    fn over_range_uses_powf() {
        let lut = identity_lut();
        // pct > 1: L_out = 100 * pct^inv_gamma
        // level_black=0, level_range=0.5 → pct = (1.0 - 0) / 0.5 = 2.0
        let input = vec![100.0f32, 0.0, 0.0, 1.0]; // L_in = 1.0
        let mut output = vec![0.0f32; 4];
        process_pixels(&input, &mut output, 0.0, 0.5, 2.0, &lut);
        let expected = 100.0 * 2.0_f32.powf(2.0); // = 400 (unclamped)
        assert!((output[0] - expected).abs() < 0.1, "powf path: {}", output[0]);
    }
}
