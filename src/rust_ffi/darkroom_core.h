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
 * Levels IOP — black/white-point + gamma correction via LUT.
 *
 * Replaces the OMP loop in src/iop/levels.c::process().
 * lut points to dt_iop_levels_data_t.lut (65536 floats).
 * level_range = d->levels[2] - d->levels[0], pre-computed by caller.
 */
void darkroom_levels_process(const float *in_buf,
                             float *out_buf,
                             size_t npixels,
                             float level_black,
                             float level_range,
                             float inv_gamma,
                             const float *lut);

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
 * Split-toning IOP — shadow/highlight colorization via HSL.
 *
 * Replaces the OMP loop in src/iop/splittoning.c::process().
 * compress must be pre-scaled: (data->compress / 110.0f) / 2.0f
 */
void darkroom_splittoning_process(const float *in_buf,
                                  float *out_buf,
                                  size_t npixels,
                                  float shadow_hue,
                                  float shadow_saturation,
                                  float highlight_hue,
                                  float highlight_saturation,
                                  float balance,
                                  float compress);

/*
 * Negadoctor IOP — film negative scan inversion.
 *
 * Replaces the OMP loop in src/iop/negadoctor.c::process().
 * dmin, wb_high, offset each point to 4 floats (dt_aligned_pixel_t).
 * black, gamma, soft_clip, soft_clip_comp, exposure are scalar floats.
 */
void darkroom_negadoctor_process(const float *in_buf,
                                 float *out_buf,
                                 size_t npixels,
                                 const float *dmin,
                                 const float *wb_high,
                                 const float *offset,
                                 float black,
                                 float gamma,
                                 float soft_clip,
                                 float soft_clip_comp,
                                 float exposure);

/*
 * Channel-mixer IOP — linear RGB and HSL channel remapping.
 *
 * Replaces the process_rgb/gray/hsl_v1/hsl_v2 loops in channelmixer.c::process().
 * hsl_matrix and rgb_matrix each point to 9 floats (3×3, row-major).
 * operation_mode: 0=RGB, 1=GRAY, 2=HSL_V1, 3=HSL_V2.
 */
void darkroom_channelmixer_process(const float *in_buf,
                                   float *out_buf,
                                   size_t npixels,
                                   const float *hsl_matrix,
                                   const float *rgb_matrix,
                                   int operation_mode);

/*
 * Lowlight IOP — scotopic vision simulation in Lab colorspace.
 *
 * Replaces the OMP loop in src/iop/lowlight.c::process().
 * lut points to d->lut (DT_IOP_LOWLIGHT_LUT_RES = 65536 floats).
 */
void darkroom_lowlight_process(const float *in_buf,
                               float *out_buf,
                               size_t npixels,
                               float blueness,
                               const float *lut);

/*
 * Tone-curve IOP — 3-channel Lab LUT with four autoscale modes.
 *
 * Replaces the OMP loop in src/iop/tonecurve.c::process().
 * table_l/a/b each point to d->table[ch_L/a/b] (65536 floats each).
 * unbounded_coeffs_l: 3 floats (d->unbounded_coeffs_L).
 * unbounded_coeffs_ab: 12 floats (d->unbounded_coeffs_ab).
 * autoscale_ab: 0=MANUAL, 1=AUTOMATIC, 2=AUTOMATIC_XYZ, 3=AUTOMATIC_RGB.
 */
void darkroom_tonecurve_process(const float *in_buf,
                                float *out_buf,
                                size_t npixels,
                                const float *table_l,
                                const float *table_a,
                                const float *table_b,
                                const float *unbounded_coeffs_l,
                                const float *unbounded_coeffs_ab,
                                int autoscale_ab,
                                int unbound_ab,
                                int preserve_colors);

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
