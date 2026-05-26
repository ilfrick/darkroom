//! IOP (Image OPerator) modules — Phase 1 target.
//!
//! Each IOP from src/iop/*.c will become a struct in this module
//! implementing `IopProcess`. Migration order follows RUST_MIGRATION_PLAN.md.

pub mod colorcontrast;
pub mod exposure;
pub mod vibrance;

use crate::{params::IopParams, roi::RoiIn, Result};

/// Placeholder for an OpenCL device buffer.
/// Will be replaced by a proper `opencl3`-backed type in Phase 1.
pub struct ClBuffer {
    _priv: (),
}

/// Core trait every IOP module must implement.
pub trait IopProcess: Send + Sync {
    /// CPU path: process `input` pixels (row-major RGBA f32) into `output`.
    fn process(
        &self,
        input: &[f32],
        output: &mut [f32],
        params: &IopParams,
        roi: &RoiIn,
    ) -> Result<()>;

    /// GPU path — must produce identical results to `process()`.
    fn process_cl(&self, buf: &mut ClBuffer, params: &IopParams) -> Result<()>;

    /// Human-readable name used in UI and logging.
    fn name(&self) -> &'static str;

    fn has_opencl(&self) -> bool {
        false
    }
}
