use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Invert;

impl IopProcess for Invert {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "invert" }
}

/// Non-mosaiced (4-channel RGBA) inversion: out[k][c] = color[c] - in[k][c].
///
/// Replaces the non-raw DT_OMP_FOR loop in src/iop/invert.c::process().
/// color points to 4 floats: { d->color[0], d->color[1], d->color[2], 1.0f }.
/// X-Trans and Bayer mosaic paths remain in C.
#[no_mangle]
pub unsafe extern "C" fn darkroom_invert_process(
    in_buf: *const f32,
    out_buf: *mut f32,
    npixels: usize,
    color: *const f32,
) {
    let input  = std::slice::from_raw_parts(in_buf,  npixels * 4);
    let output = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    let col    = std::slice::from_raw_parts(color, 4);
    for k in 0..npixels {
        output[k * 4]     = col[0] - input[k * 4];
        output[k * 4 + 1] = col[1] - input[k * 4 + 1];
        output[k * 4 + 2] = col[2] - input[k * 4 + 2];
        output[k * 4 + 3] = col[3] - input[k * 4 + 3];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invert_known_values() {
        let input = vec![0.2f32, 0.5, 0.8, 0.0,
                         1.0f32, 0.0, 0.3, 0.0];
        let color = vec![1.0f32, 1.0, 1.0, 1.0];
        let mut out = vec![0.0f32; 8];
        unsafe { darkroom_invert_process(input.as_ptr(), out.as_mut_ptr(), 2, color.as_ptr()); }
        assert!((out[0] - 0.8).abs() < 1e-6);
        assert!((out[1] - 0.5).abs() < 1e-6);
        assert!((out[2] - 0.2).abs() < 1e-6);
        assert!((out[3] - 1.0).abs() < 1e-6); // 1 - 0 = 1
        assert!((out[4] - 0.0).abs() < 1e-6); // 1 - 1 = 0
    }

    #[test]
    fn invert_identity_color_one() {
        let input = vec![0.5f32, 0.5, 0.5, 0.0];
        let color = vec![1.0f32, 1.0, 1.0, 1.0];
        let mut out = vec![0.0f32; 4];
        unsafe { darkroom_invert_process(input.as_ptr(), out.as_mut_ptr(), 1, color.as_ptr()); }
        assert!((out[0] - 0.5).abs() < 1e-6);
    }
}
