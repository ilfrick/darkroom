#pragma once
/*
 * C declarations for functions exported by the darkroom-core Rust crate
 * (crates/darkroom-core/src/iop/exposure.rs).
 *
 * This header is hand-maintained for now. When more IOPs migrate to Rust,
 * regenerate it with:
 *   cbindgen --config crates/darkroom-core/cbindgen.toml \
 *             --output src/rust_ffi/darkroom_core.h
 */

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Color-contrast IOP — affine transform on Lab a/b channels.
 *
 * Replaces the two OMP loops in src/iop/colorcontrast.c::process().
 * unbound != 0: no clamping; unbound == 0: a/b clamped to [-128, 128].
 */
void darkroom_colorcontrast_process(const float *in_buf,
                                    float *out_buf,
                                    size_t npixels,
                                    float a_steepness,
                                    float a_offset,
                                    float b_steepness,
                                    float b_offset,
                                    int unbound);

/*
 * Vibrance IOP — saturation-weighted chroma boost.
 *
 * Replaces the OMP loop in src/iop/vibrance.c::process().
 * amount must be pre-scaled by 0.01 (done in C commit_params).
 */
void darkroom_vibrance_process(const float *in_buf,
                               float *out_buf,
                               size_t npixels,
                               float amount);

/*
 * Exposure IOP pixel loop.
 *
 * Replaces the inner loop in src/iop/exposure.c::process():
 *   for(size_t k = 0; k < ch * npixels; k++)
 *       out[k] = (in[k] - black) * scale;
 *
 * in_buf and out_buf must be non-overlapping arrays of length npixels*channels.
 */
void darkroom_exposure_process(const float *in_buf,
                               float *out_buf,
                               size_t npixels,
                               size_t channels,
                               float black,
                               float scale);

#ifdef __cplusplus
} /* extern "C" */
#endif
