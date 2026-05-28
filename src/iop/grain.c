/*
    This file is part of darktable,
    Copyright (C) 2010-2024 darktable developers.

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

#include <assert.h>
#include <stdlib.h>
#include <string.h>

#include "bauhaus/bauhaus.h"
#include "common/math.h"
#include "control/control.h"
#include "develop/develop.h"
#include "develop/imageop.h"
#include "develop/imageop_gui.h"
#include "gui/accelerators.h"
#include "gui/gtk.h"
#include "iop/iop_api.h"
#include "rust_ffi/darkroom_core.h"
#include <gtk/gtk.h>
#include <inttypes.h>

#define GRAIN_LIGHTNESS_STRENGTH_SCALE 0.15f

// (m_pi/2)/4 = half hue colorspan
#define GRAIN_HUE_COLORRANGE 0.392699082

#define GRAIN_HUE_STRENGTH_SCALE 0.25
#define GRAIN_SATURATION_STRENGTH_SCALE 0.25
#define GRAIN_RGB_STRENGTH_SCALE 0.25

#define GRAIN_SCALE_FACTOR 213.2

#define GRAIN_LUT_SIZE 128
#define GRAIN_LUT_DELTA_MAX 2.0
#define GRAIN_LUT_DELTA_MIN 0.0001
#define GRAIN_LUT_PAPER_GAMMA 1.0

DT_MODULE_INTROSPECTION(2, dt_iop_grain_params_t)


typedef enum _dt_iop_grain_channel_t
{
  DT_GRAIN_CHANNEL_HUE = 0,
  DT_GRAIN_CHANNEL_SATURATION,
  DT_GRAIN_CHANNEL_LIGHTNESS,
  DT_GRAIN_CHANNEL_RGB
} _dt_iop_grain_channel_t;

typedef struct dt_iop_grain_params_t
{
  _dt_iop_grain_channel_t channel; // $DEFAULT: DT_GRAIN_CHANNEL_LIGHTNESS
  float scale;                     /* $MIN: 20.0/GRAIN_SCALE_FACTOR
                                      $MAX: 6400.0/GRAIN_SCALE_FACTOR
                                      $DEFAULT: 1600.0/GRAIN_SCALE_FACTOR
                                      $DESCRIPTION: "coarseness" */
  float strength;      // $MIN: 0.0 $MAX: 100.0 $DEFAULT: 25.0
  float midtones_bias; // $MIN: 0.0 $MAX: 100.0 $DEFAULT: 100.0 $DESCRIPTION: "mid-tones bias"
} dt_iop_grain_params_t;

typedef struct dt_iop_grain_gui_data_t
{
  GtkWidget *scale, *strength, *midtones_bias; // scale, strength, midtones_bias
} dt_iop_grain_gui_data_t;

typedef struct dt_iop_grain_data_t
{
  _dt_iop_grain_channel_t channel;
  float scale;
  float strength;
  float midtones_bias;
  float grain_lut[GRAIN_LUT_SIZE * GRAIN_LUT_SIZE];
} dt_iop_grain_data_t;


int legacy_params(dt_iop_module_t *self,
                  const void *const old_params,
                  const int old_version,
                  void **new_params,
                  int32_t *new_params_size,
                  int *new_version)
{
  typedef struct dt_iop_grain_params_v2_t
  {
    _dt_iop_grain_channel_t channel;
    float scale;
    float strength;
    float midtones_bias;
  } dt_iop_grain_params_v2_t;

  if(old_version == 1)
  {
    typedef struct dt_iop_grain_params_v1_t
    {
      _dt_iop_grain_channel_t channel;
      float scale;
      float strength;
    } dt_iop_grain_params_v1_t;

    const dt_iop_grain_params_v1_t *o = old_params;
    dt_iop_grain_params_v2_t *n = malloc(sizeof(dt_iop_grain_params_v2_t));

    n->channel = o->channel;
    n->scale = o->scale;
    n->strength = o->strength;
    n->midtones_bias = 0.0; // it produces the same results as the old version

    *new_params = n;
    *new_params_size = sizeof(dt_iop_grain_params_v2_t);
    *new_version = 2;
    return 0;
  }
  return 1;
}








static float paper_resp(float exposure, float mb, float gp)
{
  const float delta = GRAIN_LUT_DELTA_MAX * expf((mb / 100.0f) * logf(GRAIN_LUT_DELTA_MIN));
  const float density = (1.0f + 2.0f * delta) / (1.0f + expf( (4.0f * gp * (0.5f - exposure)) / (1.0f + 2.0f * delta) )) - delta;
  return density;
}

static float paper_resp_inverse(float density, float mb, float gp)
{
  const float delta = GRAIN_LUT_DELTA_MAX * expf((mb / 100.0f) * logf(GRAIN_LUT_DELTA_MIN));
  const float exposure = -logf((1.0f + 2.0f * delta) / (density + delta) - 1.0f) * (1.0f + 2.0f * delta) / (4.0f * gp) + 0.5f;
  return exposure;
}

static void evaluate_grain_lut(float *grain_lut, const float mb)
{
  for(int i = 0; i < GRAIN_LUT_SIZE; i++)
  {
    for(int j = 0; j < GRAIN_LUT_SIZE; j++)
    {
      const float gu = (float)i / (GRAIN_LUT_SIZE - 1) - 0.5;
      const float l = (float)j / (GRAIN_LUT_SIZE - 1);
      grain_lut[j * GRAIN_LUT_SIZE + i] = 100.0f * (paper_resp(gu + paper_resp_inverse(l, mb, GRAIN_LUT_PAPER_GAMMA), mb, GRAIN_LUT_PAPER_GAMMA) - l);
    }
  }
}


const char *name()
{
  return _("grain");
}

const char **description(dt_iop_module_t *self)
{
  return dt_iop_set_description(self, _("simulate silver grains from film"),
                                      _("creative"),
                                      _("non-linear, Lab, display-referred"),
                                      _("non-linear, Lab"),
                                      _("non-linear, Lab, display-referred"));
}

int flags()
{
  return IOP_FLAGS_INCLUDE_IN_STYLES | IOP_FLAGS_SUPPORTS_BLENDING;
}

int default_group()
{
  return IOP_GROUP_EFFECT | IOP_GROUP_EFFECTS;
}

dt_iop_colorspace_type_t default_colorspace(dt_iop_module_t *self,
                                            dt_dev_pixelpipe_t *pipe,
                                            dt_dev_pixelpipe_iop_t *piece)
{
  return IOP_CS_LAB;
}

// This is a modified Bernstein hash known as DJBX33X (hash x 33 with bitwise XOR).
// However, we calculate the hash from the end of the string to the beginning. Why?
// We hash the image filename. This allows us to get rid of static grain when
// creating a video from a sequence of images, the names of which will usually
// differ by the last characters. Therefore, we start hashing from the changed
// characters so that these changes have a greater impact on the resulting hash.
static unsigned int _hash_string(char *str)
{
  unsigned int hash = 5381;

  for(int i = strlen(str) - 1; i >= 0; i--)
    hash = ((hash << 5) + hash) ^ str[i];

  return hash;
}

void process(dt_iop_module_t *self,
             dt_dev_pixelpipe_iop_t *piece,
             const void *const ivoid,
             void *const ovoid,
             const dt_iop_roi_t *const roi_in,
             const dt_iop_roi_t *const roi_out)
{
  if(!dt_iop_have_required_input_format(4 /*we need full-color pixels*/, self, piece->colors,
                                        ivoid, ovoid, roi_in, roi_out))
    return;

  dt_iop_grain_data_t *data = piece->data;

  unsigned int hash = _hash_string(piece->pipe->image.filename) % (int)fmax(roi_out->width * 0.3, 1.0);

  const gboolean fastmode = dt_pipe_is_fast(piece->pipe);
  // Apply grain to image
  const float strength = (data->strength / 100.0f);
  // double zoom=1.0+(8*(data->scale/100.0));
  const double wd = fminf(piece->buf_in.width, piece->buf_in.height);
  const double zoom = (1.0 + 8 * data->scale / 100) / 800.0;
  // in fastpipe mode, skip the downsampling for zoomed-out views
  const int filter = !fastmode && fabsf(roi_out->scale - 1.0f) > 0.01f;
  // filter width depends on world space (i.e. reverse wd norm and roi->scale, as well as buffer input to
  // pixelpipe iscale)
  const double filtermul = piece->iscale / (roi_out->scale * wd);
  const double scale = roi_out->scale;	// is only used in double expressions, so avoid conversion

  darkroom_grain_process(
      (const float *)ivoid, (float *)ovoid,
      roi_out->x, roi_out->y, roi_out->width, roi_out->height,
      strength, zoom, wd, scale,
      hash, filter, filtermul,
      data->grain_lut);
}

void commit_params(dt_iop_module_t *self, dt_iop_params_t *p1, dt_dev_pixelpipe_t *pipe,
                   dt_dev_pixelpipe_iop_t *piece)
{
  dt_iop_grain_params_t *p = (dt_iop_grain_params_t *)p1;
  dt_iop_grain_data_t *d = piece->data;

  d->channel = p->channel;
  d->scale = p->scale;
  d->strength = p->strength;
  d->midtones_bias = p->midtones_bias;

  evaluate_grain_lut(d->grain_lut, d->midtones_bias);
}

void init_pipe(dt_iop_module_t *self, dt_dev_pixelpipe_t *pipe, dt_dev_pixelpipe_iop_t *piece)
{
  piece->data = calloc(1, sizeof(dt_iop_grain_data_t));
}

void cleanup_pipe(dt_iop_module_t *self, dt_dev_pixelpipe_t *pipe, dt_dev_pixelpipe_iop_t *piece)
{
  free(piece->data);
  piece->data = NULL;
}

void init_global(dt_iop_module_so_t *self)
{
}

void gui_init(dt_iop_module_t *self)
{
  dt_iop_grain_gui_data_t *g = IOP_GUI_ALLOC(grain);

  /* courseness */
  g->scale = dt_bauhaus_slider_from_params(self, "scale");
  dt_bauhaus_slider_set_factor(g->scale, GRAIN_SCALE_FACTOR);
  dt_bauhaus_slider_set_digits(g->scale, 0);
  dt_bauhaus_slider_set_format(g->scale, " ISO");
  gtk_widget_set_tooltip_text(g->scale, _("the grain size (~ISO of the film)"));

  g->strength = dt_bauhaus_slider_from_params(self, N_("strength"));
  dt_bauhaus_slider_set_format(g->strength, "%");
  gtk_widget_set_tooltip_text(g->strength, _("the strength of applied grain"));

  g->midtones_bias = dt_bauhaus_slider_from_params(self, "midtones_bias");
  dt_bauhaus_slider_set_format(g->midtones_bias, "%");
  gtk_widget_set_tooltip_text(g->midtones_bias, _("amount of mid-tones bias from the photographic paper response modeling. the greater the bias, the more pronounced the fall off of the grain in shadows and highlights"));
}

// clang-format off
// modelines: These editor modelines have been set for all relevant files by tools/update_modelines.py
// vim: shiftwidth=2 expandtab tabstop=2 cindent
// kate: tab-indents: off; indent-width 2; replace-tabs on; indent-mode cstyle; remove-trailing-spaces modified;
// clang-format on
