/*
    This file is part of darktable,
    Copyright (C) 2021-2024 darktable developers.

    darktable is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    darktable is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with darktable.  If not, see <http://www.gnu.org/licenses/>.
*/

#include "common/extra_optimizations.h"

#include "bauhaus/bauhaus.h"
#include "develop/imageop.h"
#include "rust_ffi/darkroom_core.h"
#include "develop/imageop_gui.h"
#include "gui/color_picker_proxy.h"
#include "gui/gtk.h"
#include "iop/iop_api.h"
#include "common/gaussian.h"
#include "common/fast_guided_filter.h"

#include <gtk/gtk.h>
#include <stdlib.h>

DT_MODULE_INTROSPECTION(1, dt_iop_cacorrectrgb_params_t)

/**
 * Description of the approach
 *
 ** The problem
 * chromatic aberration appear when:
 * (1) channels are misaligned
 * (2) or if some channel is more blurry than another.
 *
 * example case (1):
 *           _________
 * _________|               first channel
 *             _______
 * ___________|             second channel
 *          ^^ chromatic aberration
 *
 * other example case (1):
 *           _________
 * _________|               first channel
 * ___________
 *            |_______      second channel
 *          ^^ chromatic aberration
 *
 * example case (2):
 *           _________
 *          |               first channel
 * _________|
 *            ________
 *           /              second channel
 *          /
 * ________/
 *         ^^^ chromatic aberration
 *
 * note that case (1) can already be partially corrected using the lens
 * correction module.
 *
 ** Requirements for the solution
 * - handle both cases
 * - preserve borders as much as possible
 * - be fast to compute
 *
 ** The solution
 * The main idea is to represent 2 channels as a function of the third one.
 *
 * a very simple function is: guided = a * guide
 * where a = blur(guided) / blur(guide)
 * But this function is too simple to cope with borders.
 *
 * We stick with the idea of having guided channel as a factor of
 * the guide channel, but instead of having a locally constant factor
 * a, we use a factor that depends on the value of the guide pixel:
 * guided = a(guide) * guide
 *
 * Our function a(guide) is pretty simple, it is a weighted average
 * between 2 values (one high and one low), where the weights are
 * dependent on the guide pixel value.
 *
 * Now, how do we determine these high and low value.
 *
 * We compute 2 manifolds.
 * manifolds are partial local averages:
 * some pixels are not used in the averages.
 *
 * for the lower manifold, we average only pixels whose guide value are below
 * a local average of the guide.
 * for the higher manifold, we average only pixels whose guide value are above
 * a local average of the guide.
 *
 * for example here:
 *           _________
 * _ _ _ _ _| _ _ _ _ _ _ _ average
 * _________|
 *
 * ^^^^^^^^^ pixels below average (will be used to compute lower manifold)
 *
 *           ^^^^^^^^^ pixels above average (will be used to compute higher manifold)
 *
 * As we want to write the guided channel as a ratio of the guide channel,
 * we compute the manifolds on:
 * - the guide channel
 * - log difference between guide and guided
 *
 * using the log difference gives much better result than using directly the
 * guided channel in the manifolds computation and computing the ratio after
 * that, because averaging in linear makes lower manifolds harder to estimate
 * accurately.
 * Note that the repartition of pixels into higher and lower manifold
 * computation is done by taking into account ONLY the guide channel.
 *
 * Once we have our 2 manifolds, with an average log difference for each of them
 * (i.e. an average ratio), we can do a weighted mean to get the result.
 * We weight more one ratio or the other depending to how close the guide pixel
 * is from one manifold or another.
 *
 **/

typedef enum dt_iop_cacorrectrgb_guide_channel_t
{
  DT_CACORRECT_RGB_R = 0,    // $DESCRIPTION: "red"
  DT_CACORRECT_RGB_G = 1,    // $DESCRIPTION: "green"
  DT_CACORRECT_RGB_B = 2     // $DESCRIPTION: "blue"
} dt_iop_cacorrectrgb_guide_channel_t;

typedef enum dt_iop_cacorrectrgb_mode_t
{
  DT_CACORRECT_MODE_STANDARD = 0,  // $DESCRIPTION: "standard"
  DT_CACORRECT_MODE_DARKEN = 1,    // $DESCRIPTION: "darken only"
  DT_CACORRECT_MODE_BRIGHTEN = 2   // $DESCRIPTION: "brighten only"
} dt_iop_cacorrectrgb_mode_t;

typedef struct dt_iop_cacorrectrgb_params_t
{
  dt_iop_cacorrectrgb_guide_channel_t guide_channel; // $DEFAULT: DT_CACORRECT_RGB_G $DESCRIPTION: "guide"
  float radius; // $MIN: 1 $MAX: 500 $DEFAULT: 5 $DESCRIPTION: "radius"
  float strength; // $MIN: 0 $MAX: 4 $DEFAULT: 0.5 $DESCRIPTION: "strength"
  dt_iop_cacorrectrgb_mode_t mode; // $DEFAULT: DT_CACORRECT_MODE_STANDARD $DESCRIPTION: "correction mode"
  gboolean refine_manifolds; // $MIN: FALSE $MAX: TRUE $DEFAULT: FALSE $DESCRIPTION: "very large chromatic aberration"
} dt_iop_cacorrectrgb_params_t;

typedef struct dt_iop_cacorrectrgb_gui_data_t
{
  GtkWidget *guide_channel, *radius, *strength, *mode, *refine_manifolds;
} dt_iop_cacorrectrgb_gui_data_t;

const char *name()
{
  return _("chromatic aberrations");
}

const char **description(dt_iop_module_t *self)
{
  return dt_iop_set_description(self, _("correct chromatic aberrations"),
                                      _("corrective"),
                                      _("linear, RGB, scene-referred"),
                                      _("linear, RGB"),
                                      _("linear, RGB, scene-referred"));
}

int flags()
{
  return IOP_FLAGS_INCLUDE_IN_STYLES | IOP_FLAGS_SUPPORTS_BLENDING;
}

int default_group()
{
  return IOP_GROUP_CORRECT | IOP_GROUP_TECHNICAL;
}

dt_iop_colorspace_type_t default_colorspace(dt_iop_module_t *self,
                                            dt_dev_pixelpipe_t *pipe,
                                            dt_dev_pixelpipe_iop_t *piece)
{
  return IOP_CS_RGB;
}

void commit_params(dt_iop_module_t *self, dt_iop_params_t *p1, dt_dev_pixelpipe_t *pipe, dt_dev_pixelpipe_iop_t *piece)
{
  memcpy(piece->data, p1, self->params_size);
}

static void normalize_manifolds(const float *const restrict blurred_in,
                                float *const restrict blurred_manifold_lower,
                                float *const restrict blurred_manifold_higher,
                                const size_t width,
                                const size_t height,
                                const dt_iop_cacorrectrgb_guide_channel_t guide)
{
  darkroom_cacorrectrgb_normalize_manifolds(blurred_in,
                                            blurred_manifold_lower,
                                            blurred_manifold_higher,
                                            width, height,
                                            (unsigned int)guide);
}

#define DT_CACORRECTRGB_MAX_EV_DIFF 2.0f
static void get_manifolds(const float* const restrict in, const size_t width, const size_t height,
                          const float sigma, const float sigma2,
                          const dt_iop_cacorrectrgb_guide_channel_t guide,
                          float* const restrict manifolds, gboolean refine_manifolds)
{
  float *const restrict blurred_in = dt_alloc_align_float(width * height * 4);
  float *const restrict manifold_higher = dt_alloc_align_float(width * height * 4);
  float *const restrict manifold_lower = dt_alloc_align_float(width * height * 4);
  float *const restrict blurred_manifold_higher = dt_alloc_align_float(width * height * 4);
  float *const restrict blurred_manifold_lower = dt_alloc_align_float(width * height * 4);

  dt_aligned_pixel_t max = {FLT_MAX, FLT_MAX, FLT_MAX, FLT_MAX};
  dt_aligned_pixel_t min = {-FLT_MAX, -FLT_MAX, -FLT_MAX, 0.0f};
  // start with a larger blur to estimate the manifolds if we refine them
  // later on
  const float blur_size = refine_manifolds ? sigma2 : sigma;
  dt_gaussian_t *g = dt_gaussian_init(width, height, 4, max, min, blur_size, 0);
  if(!g) return;
  dt_gaussian_blur_4c(g, in, blurred_in);

  // construct the manifolds (Rust FFI)
  darkroom_cacorrectrgb_build_manifolds(in, blurred_in, manifold_lower, manifold_higher,
                                        width, height, (unsigned int)guide);

  dt_gaussian_blur_4c(g, manifold_higher, blurred_manifold_higher);
  dt_gaussian_blur_4c(g, manifold_lower, blurred_manifold_lower);
  dt_gaussian_free(g);

  normalize_manifolds(blurred_in, blurred_manifold_lower, blurred_manifold_higher, width, height, guide);

  // note that manifolds were constructed based on the value and average
  // of the guide channel ONLY.
  // this implies that the "higher" manifold in the channel c may be
  // actually lower than the "lower" manifold of that channel.
  // This happens in the following example:
  // guide:  1_____
  //               |_____0
  // guided:        _____1
  //         0_____|
  // here the higher manifold of guide is equal to 1, its lower manifold is
  // equal to 0. The higher manifold of the guided channel is equal to 0
  // as it is the average of the values where the guide is higher than its
  // average, and the lower manifold of the guided channel is equal to 1.

  if(refine_manifolds)
  {
    g = dt_gaussian_init(width, height, 4, max, min, sigma, 0);
    if(!g) return;
    dt_gaussian_blur_4c(g, in, blurred_in);

    // refine the manifolds
    // improve result especially on very degraded images
    // we use a blur of normal size for this step
    // refinement pass (Rust FFI)
    darkroom_cacorrectrgb_refine_manifolds(in, blurred_in,
                                           blurred_manifold_lower, blurred_manifold_higher,
                                           manifold_lower, manifold_higher,
                                           width, height, (unsigned int)guide);

    dt_gaussian_blur_4c(g, manifold_higher, blurred_manifold_higher);
    dt_gaussian_blur_4c(g, manifold_lower, blurred_manifold_lower);
    normalize_manifolds(blurred_in, blurred_manifold_lower, blurred_manifold_higher, width, height, guide);
    dt_gaussian_free(g);
  }

  dt_free_align(manifold_lower);
  dt_free_align(manifold_higher);

  // store all manifolds in the same structure to make upscaling faster
  darkroom_cacorrectrgb_pack_manifolds(blurred_manifold_lower, blurred_manifold_higher,
                                       manifolds, width * height);
  dt_free_align(blurred_in);
  dt_free_align(blurred_manifold_lower);
  dt_free_align(blurred_manifold_higher);
}
#undef DT_CACORRECTRGB_MAX_EV_DIFF

static void apply_correction(const float* const restrict in,
                          const float* const restrict manifolds,
                          const size_t width, const size_t height, const float sigma,
                          const dt_iop_cacorrectrgb_guide_channel_t guide,
                          const dt_iop_cacorrectrgb_mode_t mode,
                          float* const restrict out)

{
  darkroom_cacorrectrgb_apply_correction(in, manifolds, width, height,
                                         (unsigned int)guide, (unsigned int)mode, out);
}

static void reduce_artifacts(const float* const restrict in,
                          const size_t width, const size_t height, const float sigma,
                          const dt_iop_cacorrectrgb_guide_channel_t guide,
                          const float safety,
                          float* const restrict out)

{
  // in_out contains the 2 guided channels of in, and the 2 guided channels of out
  // it allows to blur all channels in one 4-channel gaussian blur instead of 2
  float *const restrict in_out = dt_alloc_align_float(width * height * 4);
  darkroom_cacorrectrgb_pack_inout(in, out, in_out, width * height, (unsigned int)guide);

  float *const restrict blurred_in_out = dt_alloc_align_float(width * height * 4);
  const dt_aligned_pixel_t max = {FLT_MAX, FLT_MAX, FLT_MAX, FLT_MAX};
  const dt_aligned_pixel_t min = {0.0f, 0.0f, 0.0f, 0.0f};
  dt_gaussian_t *g = dt_gaussian_init(width, height, 4, max, min, sigma, 0);
  if(!g) return;
  dt_gaussian_blur_4c(g, in_out, blurred_in_out);
  dt_gaussian_free(g);
  dt_free_align(in_out);

  // we consider that even with chromatic aberration, local average should
  // be close to be accurate.
  // thus, the local average of output should be similar to the one of the input
  // if they are not, the algorithm probably washed out colors too much or
  // may have produced artifacts.
  // we do a weighted average between input and output, keeping more input if
  // the local averages are very different.
  // we use the same weight for all channels, as using different weights
  // introduces artifacts in practice.
  darkroom_cacorrectrgb_blend_artifacts(in, blurred_in_out, out,
                                        width * height, (unsigned int)guide, safety);
  dt_free_align(blurred_in_out);
}

static void reduce_chromatic_aberrations(const float* const restrict in,
                          const size_t width, const size_t height,
                          const size_t ch, const float sigma, const float sigma2,
                          const dt_iop_cacorrectrgb_guide_channel_t guide,
                          const dt_iop_cacorrectrgb_mode_t mode,
                          const gboolean refine_manifolds,
                          const float safety,
                          float* const restrict out)

{
  const float downsize = fminf(3.0f, sigma);
  const size_t ds_width = width / downsize;
  const size_t ds_height = height / downsize;
  float *const restrict ds_in = dt_alloc_align_float(ds_width * ds_height * 4);
  // we use only one variable for both higher and lower manifolds in order
  // to save time by doing only one bilinear interpolation instead of 2.
  float *const restrict ds_manifolds = dt_alloc_align_float(ds_width * ds_height * 6);
  // Downsample the image for speed-up
  interpolate_bilinear(in, width, height, ds_in, ds_width, ds_height, 4);

  // Compute manifolds
  get_manifolds(ds_in, ds_width, ds_height, sigma / downsize, sigma2 / downsize, guide, ds_manifolds, refine_manifolds);
  dt_free_align(ds_in);

  // upscale manifolds
  float *const restrict manifolds = dt_alloc_align_float(width * height * 6);
  interpolate_bilinear(ds_manifolds, ds_width, ds_height, manifolds, width, height, 6);
  dt_free_align(ds_manifolds);

  apply_correction(in, manifolds, width, height, sigma, guide, mode, out);
  dt_free_align(manifolds);

  reduce_artifacts(in, width, height, sigma, guide, safety, out);
}

void process(dt_iop_module_t *self, dt_dev_pixelpipe_iop_t *piece, const void *const ivoid, void *const ovoid,
             const dt_iop_roi_t *const roi_in, const dt_iop_roi_t *const roi_out)
{
  if(!dt_iop_have_required_input_format(4 /*we need full-color pixels*/, self, piece->colors,
                                         ivoid, ovoid, roi_in, roi_out))
    return; // ivoid has been copied to ovoid and the module's trouble flag has been set

  dt_iop_cacorrectrgb_params_t *d = piece->data;
  // used to adjuste blur level depending on size. Don't amplify noise if magnified > 100%
  const float scale = fmaxf(piece->iscale / roi_in->scale, 1.f);
  const int ch = piece->colors;
  const size_t width = roi_out->width;
  const size_t height = roi_out->height;
  const float* in = (float*)ivoid;
  float* out = (float*)ovoid;
  const float sigma = fmaxf(d->radius / scale, 1.0f);
  const float sigma2 = fmaxf(d->radius * d->radius / scale, 1.0f);

  // whether to be very conservative in preserving the original image, or to
  // keep algorithm result even if it overshoots
  const float safety = powf(20.0f, 1.0f - d->strength);
  reduce_chromatic_aberrations(in, width, height, ch, sigma, sigma2, d->guide_channel, d->mode, d->refine_manifolds, safety, out);
}

void gui_update(dt_iop_module_t *self)
{
  dt_iop_cacorrectrgb_gui_data_t *g = self->gui_data;
  dt_iop_cacorrectrgb_params_t *p = self->params;

  gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(g->refine_manifolds), p->refine_manifolds);
}

void reload_defaults(dt_iop_module_t *self)
{
  dt_iop_cacorrectrgb_params_t *d = self->default_params;

  d->guide_channel = DT_CACORRECT_RGB_G;
  d->radius = 5.0f;
  d->strength = 0.5f;
  d->mode = DT_CACORRECT_MODE_STANDARD;
  d->refine_manifolds = FALSE;

  dt_iop_cacorrectrgb_gui_data_t *g = self->gui_data;
  if(g)
  {
    dt_bauhaus_combobox_set_default(g->guide_channel, d->guide_channel);
    dt_bauhaus_slider_set_default(g->radius, d->radius);
    dt_bauhaus_slider_set_soft_range(g->radius, 1.0, 20.0);
    dt_bauhaus_slider_set_default(g->strength, d->strength);
    dt_bauhaus_combobox_set_default(g->mode, d->mode);
    gtk_toggle_button_set_active(GTK_TOGGLE_BUTTON(g->refine_manifolds), d->refine_manifolds);
  }
}

void gui_init(dt_iop_module_t *self)
{
  dt_iop_cacorrectrgb_gui_data_t *g = IOP_GUI_ALLOC(cacorrectrgb);

  g->guide_channel = dt_bauhaus_combobox_from_params(self, "guide_channel");
  gtk_widget_set_tooltip_text(g->guide_channel, _("channel used as a reference to\n"
                                           "correct the other channels.\n"
                                           "use sharpest channel if some\n"
                                           "channels are blurry.\n"
                                           "try changing guide channel if you\n"
                                           "have artifacts."));
  g->radius = dt_bauhaus_slider_from_params(self, "radius");
  gtk_widget_set_tooltip_text(g->radius, _("increase for stronger correction"));
  g->strength = dt_bauhaus_slider_from_params(self, "strength");
  gtk_widget_set_tooltip_text(g->strength, _("balance between smoothing colors\n"
                                             "and preserving them.\n"
                                             "high values can lead to overshooting\n"
                                             "and edge bleeding."));

  dt_gui_box_add(self->widget, dt_ui_section_label_new(C_("section", "advanced parameters")));
  g->mode = dt_bauhaus_combobox_from_params(self, "mode");
  gtk_widget_set_tooltip_text(g->mode, _("correction mode to use.\n"
                                         "can help with multiple\n"
                                         "instances for very damaged\n"
                                         "images.\n"
                                         "darken only is particularly\n"
                                         "efficient to correct blue\n"
                                         "chromatic aberration."));
  g->refine_manifolds = dt_bauhaus_toggle_from_params(self, "refine_manifolds");
  gtk_widget_set_tooltip_text(g->refine_manifolds, _("runs an iterative approach\n"
                                                    "with several radii.\n"
                                                    "improves result on images\n"
                                                    "with very large chromatic\n"
                                                    "aberrations, but can smooth\n"
                                                    "colors too much on other\n"
                                                    "images."));
}
// clang-format off
// modelines: These editor modelines have been set for all relevant files by tools/update_modelines.py
// vim: shiftwidth=2 expandtab tabstop=2 cindent
// kate: tab-indents: off; indent-width 2; replace-tabs on; indent-mode cstyle; remove-trailing-spaces modified;
// clang-format on

