//! Colisa IOP — contrast, brightness, saturation via pre-computed LUTs.
//!
//! Replaces the OMP loop in src/iop/colisa.c::process().
//!
//! Per-pixel algorithm (Lab input, 4 channels):
//!   L' = ctable[L/100 * 65536]  if L < 100, else eval_exp(cunbounded_coeffs, L/100)
//!   L''= ltable[L'/100 * 65536] if L'< 100, else eval_exp(lunbounded_coeffs, L'/100)
//!   a' = a * saturation
//!   b' = b * saturation
//!
//! The LUT tables and unbounded coefficients are owned by the C data struct and
//! passed in as raw pointers — Rust borrows them for the duration of the call.

use crate::{
    iop::{ClBuffer, IopProcess},
    params::IopParams,
    roi::RoiIn,
    Error, Result,
};

/// Matches `dt_iop_eval_exp()` from src/develop/imageop_math.h:
///   `coeff[1] * pow(x * coeff[0], coeff[2])`
#[inline(always)]
fn eval_exp(coeff: &[f32; 3], x: f32) -> f32 {
    coeff[1] * (x * coeff[0]).powf(coeff[2])
}

// ── IopProcess impl ───────────────────────────────────────────────────────────

pub struct Colisa;

impl IopProcess for Colisa {
    fn name(&self) -> &'static str {
        "colisa"
    }

    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        // Colisa params contain 65K-entry LUT tables that are not trivially
        // cast via IopParams::cast. Call through the C FFI path instead.
        Err(Error::Pipeline(
            "colisa: use the C FFI entry point (LUT tables cannot be cast from raw params)".into(),
        ))
    }

    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(Error::OpenCl("colisa: OpenCL path not yet ported".into()))
    }
}

// ── Core pixel loop ───────────────────────────────────────────────────────────

/// Apply contrast (via `ctable`) + brightness (via `ltable`) to L and
/// saturation scaling to a/b channels.
///
/// Both LUT slices must have exactly 65536 entries.
/// Both `unbounded_coeffs` slices must have exactly 3 entries.
#[inline]
pub fn process_pixels(
    input: &[f32],
    output: &mut [f32],
    ctable: &[f32; 65536],
    cunbounded: &[f32; 3],
    ltable: &[f32; 65536],
    lunbounded: &[f32; 3],
    saturation: f32,
) {
    for (chunk_in, chunk_out) in input.chunks_exact(4).zip(output.chunks_exact_mut(4)) {
        let l_in = chunk_in[0];

        // contrast LUT
        let l_contrast = if l_in < 100.0 {
            let idx = ((l_in / 100.0 * 65536.0) as usize).min(65535);
            ctable[idx]
        } else {
            eval_exp(cunbounded, l_in / 100.0)
        };

        // brightness LUT
        chunk_out[0] = if l_contrast < 100.0 {
            let idx = ((l_contrast / 100.0 * 65536.0) as usize).min(65535);
            ltable[idx]
        } else {
            eval_exp(lunbounded, l_contrast / 100.0)
        };

        chunk_out[1] = chunk_in[1] * saturation;
        chunk_out[2] = chunk_in[2] * saturation;
        chunk_out[3] = chunk_in[3];
    }
}

// ── C FFI entry point ─────────────────────────────────────────────────────────

/// Called from src/iop/colisa.c in place of the OMP loop.
///
/// `ctable` and `ltable` point to `dt_iop_colisa_data_t.ctable/ltable`
/// (each 65536 floats). `cunbounded_coeffs`/`lunbounded_coeffs` each have 3 floats.
///
/// # Safety
/// All pointer arguments must be valid for the duration of this call.
/// `ctable`/`ltable` must point to arrays of at least 65536 floats.
/// `cunbounded_coeffs`/`lunbounded_coeffs` must point to arrays of at least 3 floats.
#[no_mangle]
pub unsafe extern "C" fn darkroom_colisa_process(
    in_buf: *const f32,
    out_buf: *mut f32,
    npixels: usize,
    ctable: *const f32,
    cunbounded_coeffs: *const f32,
    ltable: *const f32,
    lunbounded_coeffs: *const f32,
    saturation: f32,
) {
    let input = std::slice::from_raw_parts(in_buf, npixels * 4);
    let output = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    // Safety: caller guarantees these are valid 65536/3-entry arrays.
    let ct: &[f32; 65536] = &*(ctable as *const [f32; 65536]);
    let cu: &[f32; 3] = &*(cunbounded_coeffs as *const [f32; 3]);
    let lt: &[f32; 65536] = &*(ltable as *const [f32; 65536]);
    let lu: &[f32; 3] = &*(lunbounded_coeffs as *const [f32; 3]);
    process_pixels(input, output, ct, cu, lt, lu, saturation);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_lut() -> Box<[f32; 65536]> {
        // identity LUT: index i → 100 * i/65536
        let mut t = Box::new([0.0f32; 65536]);
        for (i, v) in t.iter_mut().enumerate() {
            *v = 100.0 * i as f32 / 65536.0;
        }
        t
    }

    #[test]
    fn eval_exp_basic() {
        // coeff = [1, 1, 1] → y = 1 * pow(x * 1, 1) = x
        let coeff = [1.0f32, 1.0, 1.0];
        assert!((eval_exp(&coeff, 0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn identity_luts_pass_through_l() {
        let lut = identity_lut();
        let coeff = [1.0f32, 1.0, 1.0]; // eval_exp(c, x) = x
        let input = vec![50.0f32, 10.0, -5.0, 1.0];
        let mut output = vec![0.0f32; 4];
        process_pixels(&input, &mut output, &lut, &coeff, &lut, &coeff, 1.0);
        // With identity LUTs and saturation=1, L should round-trip approximately.
        assert!((output[0] - 50.0).abs() < 0.1, "L round-trip failed: {}", output[0]);
        assert!((output[1] - 10.0).abs() < 1e-5);
        assert!((output[2] - (-5.0)).abs() < 1e-5);
        assert!((output[3] - 1.0).abs() < 1e-7);
    }

    #[test]
    fn saturation_zero_zeroes_ab() {
        let lut = identity_lut();
        let coeff = [1.0f32, 1.0, 1.0];
        let input = vec![60.0f32, 30.0, -20.0, 1.0];
        let mut output = vec![0.0f32; 4];
        process_pixels(&input, &mut output, &lut, &coeff, &lut, &coeff, 0.0);
        assert!(output[1].abs() < 1e-7);
        assert!(output[2].abs() < 1e-7);
    }
}
