/*
    This file is part of darktable,
    Copyright (C) 2024 darktable developers.

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

#pragma once

#include <glib.h>
#include "common/printing.h"

#define DT_PRINT_LAYOUT_NAME_LEN 128
#define DT_PRINT_LAYOUT_MAX_CELLS 20

typedef struct dt_print_layout_cell_t
{
  float x, y, w, h; // relative positions [0..1] as fraction of page
} dt_print_layout_cell_t;

typedef struct dt_print_layout_t
{
  char name[DT_PRINT_LAYOUT_NAME_LEN];
  int  num_cells;
  dt_print_layout_cell_t cells[DT_PRINT_LAYOUT_MAX_CELLS];
} dt_print_layout_t;

// Load all templates from the system data dir and user config dir.
// Returns a GList of dt_print_layout_t* (caller owns and must free with dt_print_layouts_free).
GList *dt_print_layouts_load(void);

void dt_print_layouts_free(GList *layouts);

// Apply a layout template: clears existing boxes and sets up new cells.
// Needs imgs->screen.page to be valid (i.e. print view must be visible).
void dt_print_layout_apply(dt_images_box *imgs, const dt_print_layout_t *layout);

// clang-format off
// modelines: These editor modelines have been set for all relevant files by tools/update_modelines.py
// vim: shiftwidth=2 expandtab tabstop=2 cindent
// kate: tab-indents: off; indent-width 2; replace-tabs on; indent-mode cstyle; remove-trailing-spaces modified;
// clang-format on
