/*
    This file is part of darktable,
    Copyright (C) 2010-2026 darktable developers.

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

#include "bauhaus/bauhaus.h"
#include "common/colorspaces.h"
#include "common/darktable.h"
#include "common/math.h"
#include "control/control.h"
#include "common/dttypes.h"
#include "develop/develop.h"
#include "develop/imageop.h"
#include "dtgtk/resetlabel.h"
#include "gui/gtk.h"
#include "iop/iop_api.h"
#include "rust_ffi/darkroom_core.h"

#include <assert.h>
#include <gtk/gtk.h>
#include <inttypes.h>
#include <stdlib.h>
#include <string.h>

#define ROUND_POSISTIVE(f) ((unsigned int)((f)+0.5))

DT_MODULE(1)

typedef struct dt_iop_rlce_params_t
{
  double radius;
  double slope;
} dt_iop_rlce_params_t;

typedef struct dt_iop_rlce_gui_data_t
{
  GtkWidget *scale1, *scale2; // radie pixels, slope
} dt_iop_rlce_gui_data_t;

typedef struct dt_iop_rlce_data_t
{
  double radius;
  double slope;
} dt_iop_rlce_data_t;


const char *name()
{
  return _("old local contrast");
}

int default_group()
{
  return IOP_GROUP_EFFECT | IOP_GROUP_EFFECTS;
}

const char *deprecated_msg()
{
  return _("this module is deprecated. better use new local contrast module instead.");
}

int flags()
{
  return IOP_FLAGS_INCLUDE_IN_STYLES | IOP_FLAGS_DEPRECATED;
}

dt_iop_colorspace_type_t default_colorspace(dt_iop_module_t *self,
                                            dt_dev_pixelpipe_t *pipe,
                                            dt_dev_pixelpipe_iop_t *piece)
{
  return IOP_CS_RGB;
}

void process(dt_iop_module_t *self,
             dt_dev_pixelpipe_iop_t *piece,
             const void *const ivoid,
             void *const ovoid,
             const dt_iop_roi_t *const roi_in,
             const dt_iop_roi_t *const roi_out)
{
  dt_iop_rlce_data_t *data = piece->data;

  const int rad = data->radius * roi_in->scale / piece->iscale;
  darkroom_clahe_process((const float *)ivoid, (float *)ovoid,
                         (size_t)roi_out->width, (size_t)roi_out->height,
                         rad, data->slope);
}

static void radius_callback(GtkWidget *slider,
                            dt_iop_module_t *self)
{
  DT_GUARD_GUI_UPDATE();
  dt_iop_rlce_params_t *p = self->params;
  p->radius = dt_bauhaus_slider_get(slider);
  dt_dev_add_history_item(darktable.develop, self, TRUE);
}

static void slope_callback(GtkWidget *slider,
                           dt_iop_module_t *self)
{
  DT_GUARD_GUI_UPDATE();
  dt_iop_rlce_params_t *p = self->params;
  p->slope = dt_bauhaus_slider_get(slider);
  dt_dev_add_history_item(darktable.develop, self, TRUE);
}



void commit_params(dt_iop_module_t *self,
                   dt_iop_params_t *p1,
                   dt_dev_pixelpipe_t *pipe,
                   dt_dev_pixelpipe_iop_t *piece)
{
  dt_iop_rlce_params_t *p = (dt_iop_rlce_params_t *)p1;
  dt_iop_rlce_data_t *d = piece->data;

  d->radius = p->radius;
  d->slope = p->slope;
}

void init_pipe(dt_iop_module_t *self,
               dt_dev_pixelpipe_t *pipe,
               dt_dev_pixelpipe_iop_t *piece)
{
  piece->data = calloc(1, sizeof(dt_iop_rlce_data_t));
}

void cleanup_pipe(dt_iop_module_t *self,
                  dt_dev_pixelpipe_t *pipe,
                  dt_dev_pixelpipe_iop_t *piece)
{
  free(piece->data);
  piece->data = NULL;
}

void gui_update(dt_iop_module_t *self)
{
  dt_iop_rlce_gui_data_t *g = self->gui_data;
  dt_iop_rlce_params_t *p = self->params;
  dt_bauhaus_slider_set(g->scale1, p->radius);
  dt_bauhaus_slider_set(g->scale2, p->slope);
}

void init(dt_iop_module_t *self)
{
  self->params = calloc(1, sizeof(dt_iop_rlce_params_t));
  self->default_params = calloc(1, sizeof(dt_iop_rlce_params_t));
  self->default_enabled = FALSE;
  self->params_size = sizeof(dt_iop_rlce_params_t);
  self->gui_data = NULL;
  *((dt_iop_rlce_params_t *)self->default_params) = (dt_iop_rlce_params_t){ 64, 1.25 };
}

void gui_init(dt_iop_module_t *self)
{
  dt_iop_rlce_gui_data_t *g = IOP_GUI_ALLOC(rlce);
  dt_iop_rlce_params_t *p = self->default_params;

  g->scale1 = dt_bauhaus_slider_new_with_range(NULL, 0.0, 256.0, 0, p->radius, 0);
  g->scale2 = dt_bauhaus_slider_new_with_range(NULL, 1.0, 3.0, 0, p->slope, 2);
  dt_bauhaus_widget_set_label(g->scale1, NULL, _("radius"));
  dt_bauhaus_widget_set_label(g->scale2, NULL, _("amount"));
  gtk_widget_set_tooltip_text(GTK_WIDGET(g->scale1), _("size of features to preserve"));
  gtk_widget_set_tooltip_text(GTK_WIDGET(g->scale2), _("strength of the effect"));

  g_signal_connect(G_OBJECT(g->scale1), "value-changed",
                   G_CALLBACK(radius_callback), self);
  g_signal_connect(G_OBJECT(g->scale2), "value-changed",
                   G_CALLBACK(slope_callback), self);

  self->widget = dt_gui_vbox(g->scale1, g->scale2);
}

// clang-format off
// modelines: These editor modelines have been set for all relevant files by tools/update_modelines.py
// vim: shiftwidth=2 expandtab tabstop=2 cindent
// kate: tab-indents: off; indent-width 2; replace-tabs on; indent-mode cstyle; remove-trailing-spaces modified;
// clang-format on
