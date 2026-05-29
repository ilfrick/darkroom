use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Temperature;

impl IopProcess for Temperature {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "temperature" }
}

/// Non-mosaiced (RGB/RGBA) white-balance multiply.
///
/// Replaces the DT_OMP_FOR loop in the `else` branch of temperature.c::process().
/// coeffs[0..4] = d->coeffs — one scalar multiplier per RGBA channel.
#[no_mangle]
pub unsafe extern "C" fn darkroom_temperature_process_rgb(
    in_buf: *const f32,
    out_buf: *mut f32,
    npixels: usize,
    coeffs: *const f32,
) {
    let input = std::slice::from_raw_parts(in_buf, npixels * 4);
    let output = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    let c = std::slice::from_raw_parts(coeffs, 4);
    for k in 0..npixels {
        output[k * 4]     = input[k * 4]     * c[0];
        output[k * 4 + 1] = input[k * 4 + 1] * c[1];
        output[k * 4 + 2] = input[k * 4 + 2] * c[2];
        output[k * 4 + 3] = input[k * 4 + 3] * c[3];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_multiply_scales_all_channels() {
        let input = vec![0.5f32, 0.25, 0.125, 1.0,
                         1.0f32, 0.0,  0.5,   0.5];
        let mut out = vec![0.0f32; 8];
        let coeffs = [2.0f32, 4.0, 8.0, 1.0];
        unsafe {
            darkroom_temperature_process_rgb(
                input.as_ptr(), out.as_mut_ptr(), 2, coeffs.as_ptr()
            );
        }
        // pixel 0
        assert!((out[0] - 1.0).abs() < 1e-6);  // 0.5 * 2
        assert!((out[1] - 1.0).abs() < 1e-6);  // 0.25 * 4
        assert!((out[2] - 1.0).abs() < 1e-6);  // 0.125 * 8
        assert!((out[3] - 1.0).abs() < 1e-6);  // 1.0 * 1
        // pixel 1
        assert!((out[4] - 2.0).abs() < 1e-6);  // 1.0 * 2
        assert!((out[5] - 0.0).abs() < 1e-6);  // 0.0 * 4
        assert!((out[6] - 4.0).abs() < 1e-6);  // 0.5 * 8
        assert!((out[7] - 0.5).abs() < 1e-6);  // 0.5 * 1
    }

    #[test]
    fn unity_coefficients_are_passthrough() {
        let input: Vec<f32> = (0..8).map(|i| i as f32 * 0.1).collect();
        let mut out = vec![0.0f32; 8];
        let coeffs = [1.0f32; 4];
        unsafe {
            darkroom_temperature_process_rgb(
                input.as_ptr(), out.as_mut_ptr(), 2, coeffs.as_ptr()
            );
        }
        for (a, b) in input.iter().zip(out.iter()) {
            assert!((a - b).abs() < 1e-7);
        }
    }
}
