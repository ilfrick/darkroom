use crate::{params::IopParams, roi::RoiIn, Result};
use super::{ClBuffer, IopProcess};

pub struct Zonesystem;

impl IopProcess for Zonesystem {
    fn process(&self, _input: &[f32], _output: &mut [f32], _params: &IopParams, _roi: &RoiIn) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn process_cl(&self, _buf: &mut ClBuffer, _params: &IopParams) -> Result<()> {
        Err(crate::Error::Pipeline("not implemented".into()))
    }
    fn name(&self) -> &'static str { "zonesystem" }
}

#[no_mangle]
pub unsafe extern "C" fn darkroom_zonesystem_process(
    in_buf: *const f32,
    out_buf: *mut f32,
    npixels: usize,
    rzscale: f32,
    zonemap_offset: *const f32, // [size] floats
    zonemap_scale: *const f32,  // [size] floats
    size: usize,
) {
    let input = std::slice::from_raw_parts(in_buf, npixels * 4);
    let output = std::slice::from_raw_parts_mut(out_buf, npixels * 4);
    let offsets = std::slice::from_raw_parts(zonemap_offset, size);
    let scales = std::slice::from_raw_parts(zonemap_scale, size);

    let max_rz = (size as i32) - 2;

    for k in (0..npixels * 4).step_by(4) {
        let luma = input[k];
        let rz = (luma * rzscale) as i32;
        let rz = rz.clamp(0, max_rz) as usize;

        let zs = if rz > 0 && luma != 0.0 {
            offsets[rz] / luma
        } else {
            0.0
        } + scales[rz];

        output[k]     = input[k]     * zs;
        output[k + 1] = input[k + 1] * zs;
        output[k + 2] = input[k + 2] * zs;
        output[k + 3] = input[k + 3] * zs;
    }
}
