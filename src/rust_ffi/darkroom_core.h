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
 * Color-correction IOP — luminance-dependent Lab a/b scaling with saturation.
 *
 * Replaces the OMP loop in src/iop/colorcorrection.c::process().
 * out.a = saturation * (in.a + in.L * a_scale + a_base)
 * out.b = saturation * (in.b + in.L * b_scale + b_base)
 */
void darkroom_colorcorrection_process(const float *in_buf,
                                      float *out_buf,
                                      size_t npixels,
                                      float a_scale,
                                      float a_base,
                                      float b_scale,
                                      float b_base,
                                      float saturation);

/*
 * Relight IOP — gaussian-weighted L-channel boost in Lab colorspace.
 *
 * Replaces the OMP loop in src/iop/relight.c::process().
 * GAUSS(a=1, b, c, x) = expf(-(x-b)^2 / c^2)  [no 2× in denominator]
 */
void darkroom_relight_process(const float *in_buf,
                              float *out_buf,
                              size_t npixels,
                              float ev,
                              float center,
                              float width);

/*
 * Colorize IOP — replace a/b with fixed Lab color, blend L from input.
 *
 * Replaces the OMP loop in src/iop/colorize.c::process().
 * Alpha is always written as 0 (matching C copy_pixel({0,a,b,0})).
 */
void darkroom_colorize_process(const float *in_buf,
                               float *out_buf,
                               size_t npixels,
                               float color_l,
                               float color_a,
                               float color_b,
                               float mix);

/*
 * Velvia IOP — film-emulation saturation boost in RGB colorspace.
 *
 * Replaces the OMP loop in src/iop/velvia.c::process().
 * strength must be pre-scaled by 0.01 (data->strength / 100.0f).
 * Handles strength <= 0 internally (copies input).
 */
void darkroom_velvia_process(const float *in_buf,
                             float *out_buf,
                             size_t npixels,
                             float strength,
                             float bias);

/*
 * Colisa IOP — contrast/brightness (LUT) + saturation in Lab colorspace.
 *
 * Replaces the OMP loop in src/iop/colisa.c::process().
 * ctable/ltable point to dt_iop_colisa_data_t.ctable/ltable (65536 floats each).
 * cunbounded_coeffs/lunbounded_coeffs each have 3 floats.
 */
void darkroom_colisa_process(const float *in_buf,
                             float *out_buf,
                             size_t npixels,
                             const float *ctable,
                             const float *cunbounded_coeffs,
                             const float *ltable,
                             const float *lunbounded_coeffs,
                             float saturation);

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
