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
 * Primaries IOP — linear RGB color matrix adjustment.
 *
 * Replaces the OMP loop in src/iop/primaries.c::process().
 * matrix points to dt_colormatrix_t (float[4][4] = 16 floats, row-major).
 * dt_apply_transposed_color_matrix: out[r] = Σ matrix[c][r] * in[c] for c=0..2
 */
void darkroom_primaries_process(const float *in_buf,
                                float *out_buf,
                                size_t npixels,
                                const float *matrix);

/*
 * Profile-gamma IOP — logarithmic or gamma LUT tone mapping.
 *
 * Replaces the OMP loops in src/iop/profile_gamma.c::process().
 * mode: 0=LOG (all ch*npixels elements), 1=GAMMA (channels 0..2 only).
 * grey = data->grey_point / 100.0f (LOG mode only).
 * table: 65536 floats (GAMMA mode); unbounded_coeffs: 3 floats (GAMMA mode).
 */
void darkroom_profile_gamma_process(const float *in_buf,
                                    float *out_buf,
                                    size_t npixels,
                                    int mode,
                                    float grey,
                                    float dynamic_range,
                                    float shadows_range,
                                    const float *table,
                                    const float *unbounded_coeffs);

/*
 * Graduated ND filter IOP — exponential density gradient.
 *
 * Replaces the OMP loops in src/iop/graduatednd.c::process().
 * Geometry scalars must be pre-computed by C caller (see source for formulas).
 * density > 0: divides; density < 0: multiplies (negated length).
 * color / color1 each point to 4 floats (dt_aligned_pixel_t).
 */
void darkroom_graduatednd_process(const float *in_buf,
                                  float *out_buf,
                                  int width,
                                  int height,
                                  float density,
                                  float length_base,
                                  float length_inc,
                                  float cosv_hh_inv,
                                  float filter_hardness,
                                  int iy,
                                  const float *color,
                                  const float *color1);

/*
 * Grain IOP — photographic film grain via simplex noise on L channel.
 *
 * Replaces the OMP loop in src/iop/grain.c::process() (non-filter path only).
 * grain_lut: 128×128 floats from data->grain_lut; if NULL, built from midtones_bias.
 * strength = data->strength / 100.0f
 * zoom = (1.0 + 8*data->scale/100) / 800.0
 * wd = fminf(piece->buf_in.width, piece->buf_in.height)
 * hash = _hash_string(filename) % max(roi->width*0.3, 1)
 */
void darkroom_grain_process(const float *in_buf,
                            float *out_buf,
                            int roi_x,
                            int roi_y,
                            int width,
                            int height,
                            float strength,
                            double zoom,
                            double wd,
                            double scale,
                            int hash,
                            int filter,
                            double filtermul,
                            const float *grain_lut);

/*
 * RGB-curve IOP — per-channel or linked LUT tone mapping.
 *
 * Replaces the OMP loop in src/iop/rgbcurve.c::process().
 * autoscale: 0 = AUTOMATIC_RGB (linked, R curve applied to all channels)
 *            1 = MANUAL_RGB (independent per-channel curves)
 * preserve_colors: 0 = NONE, non-zero = luma-norm mode (see color.rs rgb_norm).
 * table_r/g/b: 65536 floats each; unbounded_r/g/b: 3 floats each.
 * xm_r/g/b = 1.0f / unbounded_coeffs[ch][0], pre-computed by caller.
 */
void darkroom_rgbcurve_process(const float *in_buf,
                               float *out_buf,
                               size_t npixels,
                               const float *table_r,
                               const float *table_g,
                               const float *table_b,
                               const float *unbounded_r,
                               const float *unbounded_g,
                               const float *unbounded_b,
                               float xm_r,
                               float xm_g,
                               float xm_b,
                               int autoscale,
                               int preserve_colors);

/*
 * Color-zones IOP — luminance/chroma/hue equalizer in LCH space.
 *
 * Replaces process_v1/process_v3 in src/iop/colorzones.c.
 * mode: 0 = smooth/v3 (DT_IOP_COLORZONES_MODE_SMOOTH), non-zero = flat/v1.
 * channel: 0 = L, 1 = C, 2 = h (drives LUT selection index).
 * lut_l/a/b: each DT_IOP_COLORZONES_LUT_RES (65536) floats — d->lut[0..2].
 */
void darkroom_colorzones_process(const float *in_buf,
                                 float *out_buf,
                                 size_t npixels,
                                 int mode,
                                 int channel,
                                 const float *lut_l,
                                 const float *lut_a,
                                 const float *lut_b);

/*
 * Vignette IOP — radial brightness/saturation falloff with optional dithering.
 *
 * Replaces the OMP loop in src/iop/vignette.c::process().
 * All geometry scalars must be pre-computed by the C caller.
 * dither_amt: 0.0 = off, 1/256 = 8-bit, 1/65536 = 16-bit.
 * unbound: 0 = clamp output to [0,1], non-zero = no clamp.
 */
void darkroom_vignette_process(const float *in_buf,
                               float *out_buf,
                               int width,
                               int height,
                               float xscale,
                               float yscale,
                               float roi_center_x,
                               float roi_center_y,
                               float dscale,
                               float fscale,
                               float exp1,
                               float exp2,
                               float dither_amt,
                               float brightness,
                               float saturation,
                               int unbound);

/*
 * Sigmoid IOP — RGB-ratio path: luma-based tone curve + hyperbolic gamut compression.
 *
 * Replaces process_loglogistic_rgb_ratio in src/iop/sigmoid.c.
 * white_target / black_target = module_data->white_target / black_target.
 * paper_exp / film_fog / contrast_power / skew_power = from module_data.
 */
void darkroom_sigmoid_rgb_ratio_process(const float *in_buf,
                                        float *out_buf,
                                        size_t npixels,
                                        float white_target,
                                        float black_target,
                                        float paper_exp,
                                        float film_fog,
                                        float contrast_power,
                                        float skew_power);

/*
 * Sigmoid IOP — per-channel path: per-channel tone curve + hue preservation.
 *
 * Replaces process_loglogistic_per_channel in src/iop/sigmoid.c.
 * pipe_to_base / base_to_rendering / rendering_to_pipe: each 16 floats
 * (dt_colormatrix_t), pre-computed by C caller via _calculate_adjusted_primaries.
 */
void darkroom_sigmoid_per_channel_process(const float *in_buf,
                                          float *out_buf,
                                          size_t npixels,
                                          float white_target,
                                          float paper_exp,
                                          float film_fog,
                                          float contrast_power,
                                          float skew_power,
                                          float hue_preservation,
                                          const float *pipe_to_base,
                                          const float *base_to_rendering,
                                          const float *rendering_to_pipe);

/*
 * RGB-levels IOP — per-channel or luma-linked black/white-point + gamma correction.
 *
 * Replaces the two DT_OMP_FOR loops in src/iop/rgblevels.c::process().
 * mode: 0 = independent channels (INDEPENDENT or preserve_colors==NONE)
 *       1 = linked via rgb_norm luma
 * preserve_colors: dt_rgb_norm mode for linked path.
 * min_levels / max_levels / inv_gamma: 3 floats each (R, G, B).
 * lut_r/g/b: 65536 floats each (d->lut[0..2]).
 */
void darkroom_rgblevels_process(const float *in_buf,
                                float *out_buf,
                                size_t npixels,
                                int mode,
                                int preserve_colors,
                                const float *min_levels,
                                const float *max_levels,
                                const float *inv_gamma,
                                const float *lut_r,
                                const float *lut_g,
                                const float *lut_b);

/*
 * Basic Adjustments IOP pixel loop.
 *
 * Replaces the DT_OMP_FOR loop in src/iop/basicadj.c::process().
 * lut_gamma and lut_contrast are 65536-entry float arrays.
 * plain_contrast and preserve_colors are mutually exclusive (C enforces this).
 */
void darkroom_basicadj_process(const float *in_buf,
                               float *out_buf,
                               size_t npixels,
                               float black_point,
                               float scale,
                               int process_hlcompr,
                               float hlcomp,
                               float hlrange,
                               float lum_r,
                               float lum_g,
                               float lum_b,
                               int process_gamma,
                               float gamma,
                               const float *lut_gamma,
                               int plain_contrast,
                               int preserve_colors,
                               float contrast,
                               float middle_grey,
                               float inv_middle_grey,
                               const float *lut_contrast,
                               int process_saturation_vibrance,
                               float saturation,
                               float vibrance);

/*
 * Zonesystem IOP pixel loop.
 *
 * Replaces the DT_OMP_FOR loop in src/iop/zonesystem.c::process().
 * zonemap_offset and zonemap_scale are arrays of `size` floats.
 */
void darkroom_zonesystem_process(const float *in_buf,
                                 float *out_buf,
                                 size_t npixels,
                                 float rzscale,
                                 const float *zonemap_offset,
                                 const float *zonemap_scale,
                                 size_t size);

/*
 * Overlay IOP pixel loop.
 *
 * Replaces the DT_OMP_FOR(collapse(2)) loop in src/iop/overlay.c::process().
 * image is a Cairo ARGB32 buffer (byte order [B, G, R, A]) with `stride` bytes per row.
 * opacity is pre-divided by 100 (range 0..1).
 */
void darkroom_overlay_process(const float *in_buf,
                              float *out_buf,
                              size_t width,
                              size_t height,
                              const unsigned char *image,
                              size_t stride,
                              float opacity);

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

/*
 * Filmic IOP pixel loop (Lab-space filmic tone-mapping).
 *
 * Replaces the DT_OMP_FOR loop in src/iop/filmic.c::process().
 * All parameters are pre-computed from dt_iop_filmic_data_t by the caller.
 * table and grad_2 are float[0x10000] LUTs from data->table / data->grad_2.
 * output_power is data->output_power (scalar, applied per channel).
 * desaturate = (data->global_saturation != 100.0f).
 * saturation  = data->global_saturation / 100.0f.
 * eps         = powf(2.0f, -16).
 * Output alpha is always 0 (matching Lab copy_pixel_nontemporal behaviour).
 */
void darkroom_filmic_process(const float *in_buf,
                             float *out_buf,
                             size_t npixels,
                             float grey_source,
                             float black_source,
                             float inv_dynamic_range,
                             float output_power,
                             float saturation,
                             float eps,
                             int desaturate,
                             int preserve_color,
                             const float *table,
                             const float *grad_2);

#ifdef __cplusplus
} /* extern "C" */
#endif
