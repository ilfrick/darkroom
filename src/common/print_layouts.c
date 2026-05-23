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

#include "common/print_layouts.h"
#include "common/darktable.h"
#include "common/file_location.h"

#include <json-glib/json-glib.h>
#include <string.h>

static dt_print_layout_t *_load_layout_file(const char *path)
{
  GError *error = NULL;
  JsonParser *parser = json_parser_new();

  if(!json_parser_load_from_file(parser, path, &error))
  {
    dt_print(DT_DEBUG_ALWAYS, "[print_layouts] failed to parse '%s': %s\n",
             path, error ? error->message : "unknown error");
    if(error) g_error_free(error);
    g_object_unref(parser);
    return NULL;
  }

  JsonNode *root = json_parser_get_root(parser);
  if(!root || !JSON_NODE_HOLDS_OBJECT(root))
  {
    g_object_unref(parser);
    return NULL;
  }

  JsonObject *obj = json_node_get_object(root);

  const char *name = json_object_has_member(obj, "name")
    ? json_object_get_string_member(obj, "name")
    : "Unnamed";

  JsonArray *cells_arr = json_object_has_member(obj, "cells")
    ? json_object_get_array_member(obj, "cells")
    : NULL;

  if(!cells_arr)
  {
    dt_print(DT_DEBUG_ALWAYS, "[print_layouts] '%s': missing 'cells' array\n", path);
    g_object_unref(parser);
    return NULL;
  }

  const guint ncells = json_array_get_length(cells_arr);
  if(ncells == 0 || ncells > DT_PRINT_LAYOUT_MAX_CELLS)
  {
    dt_print(DT_DEBUG_ALWAYS, "[print_layouts] '%s': invalid cell count %u\n", path, ncells);
    g_object_unref(parser);
    return NULL;
  }

  dt_print_layout_t *layout = calloc(1, sizeof(dt_print_layout_t));
  if(!layout)
  {
    g_object_unref(parser);
    return NULL;
  }

  g_strlcpy(layout->name, name, DT_PRINT_LAYOUT_NAME_LEN);
  layout->num_cells = (int)ncells;

  for(guint i = 0; i < ncells; i++)
  {
    JsonNode *cell_node = json_array_get_element(cells_arr, i);
    if(!JSON_NODE_HOLDS_OBJECT(cell_node)) continue;

    JsonObject *c = json_node_get_object(cell_node);
    layout->cells[i].x = json_object_has_member(c, "x") ? (float)json_object_get_double_member(c, "x") : 0.0f;
    layout->cells[i].y = json_object_has_member(c, "y") ? (float)json_object_get_double_member(c, "y") : 0.0f;
    layout->cells[i].w = json_object_has_member(c, "w") ? (float)json_object_get_double_member(c, "w") : 1.0f;
    layout->cells[i].h = json_object_has_member(c, "h") ? (float)json_object_get_double_member(c, "h") : 1.0f;
  }

  g_object_unref(parser);
  return layout;
}

static void _load_layouts_from_dir(const char *dirpath, GList **list)
{
  GDir *dir = g_dir_open(dirpath, 0, NULL);
  if(!dir) return;

  const char *fname;
  while((fname = g_dir_read_name(dir)))
  {
    if(!g_str_has_suffix(fname, ".json")) continue;

    char *path = g_build_filename(dirpath, fname, NULL);
    dt_print_layout_t *layout = _load_layout_file(path);
    if(layout)
      *list = g_list_append(*list, layout);
    g_free(path);
  }
  g_dir_close(dir);
}

GList *dt_print_layouts_load(void)
{
  GList *layouts = NULL;

  // system templates
  char *sysdir = g_build_filename(darktable.datadir, "print_layouts", NULL);
  _load_layouts_from_dir(sysdir, &layouts);
  g_free(sysdir);

  // user templates (~/.config/darkroom/print_layouts/)
  char *userdir = g_build_filename(darktable.configdir, "print_layouts", NULL);
  _load_layouts_from_dir(userdir, &layouts);
  g_free(userdir);

  return layouts;
}

void dt_print_layouts_free(GList *layouts)
{
  for(GList *l = layouts; l; l = g_list_next(l))
    free(l->data);
  g_list_free(layouts);
}

void dt_print_layout_apply(dt_images_box *imgs, const dt_print_layout_t *layout)
{
  if(imgs->screen.page.width < 1.0f || imgs->screen.page.height < 1.0f) return;

  dt_printing_clear_boxes(imgs);

  const float px = imgs->screen.page.x;
  const float py = imgs->screen.page.y;
  const float pw = imgs->screen.page.width;
  const float ph = imgs->screen.page.height;

  for(int i = 0; i < layout->num_cells && i < DT_PRINT_LAYOUT_MAX_CELLS; i++)
  {
    const dt_print_layout_cell_t *c = &layout->cells[i];
    const float sx = px + c->x * pw;
    const float sy = py + c->y * ph;
    const float sw = c->w * pw;
    const float sh = c->h * ph;
    dt_printing_setup_box(imgs, i, sx, sy, sw, sh);
  }
}

// clang-format off
// modelines: These editor modelines have been set for all relevant files by tools/update_modelines.py
// vim: shiftwidth=2 expandtab tabstop=2 cindent
// kate: tab-indents: off; indent-width 2; replace-tabs on; indent-mode cstyle; remove-trailing-spaces modified;
// clang-format on
