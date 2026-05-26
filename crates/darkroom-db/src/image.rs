//! Image record — replaces src/common/image.c in Phase 2.

use darkroom_sys::dt_imgid_t;

/// Mirror of the C `dt_image_t` struct (partial — fields added per Phase 2 progress).
#[repr(C)]
pub struct DtImage {
    pub id: dt_imgid_t,
    pub film_id: i32,
    pub width: i32,
    pub height: i32,
    pub flags: u32,
}
