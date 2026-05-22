/*
  This file is part of darktable,
  Copyright (C) 2025 darktable developers.

  darktable is free software: you can redistribute it and/or modify
  it under the terms of the GNU General Public License as published by
  the Free Software Foundation, either version 3 of the License, or
  (at your option) any later version.
*/

#include "common/smart_previews.h"
#include "common/darktable.h"
#include "common/database.h"
#include "common/debug.h"
#include "common/file_location.h"
#include "common/image.h"
#include "common/image_cache.h"
#include "common/mipmap_cache.h"
#include "imageio/imageio_jpeg.h"

#include <glib.h>
#include <stdio.h>
#include <sys/stat.h>

#define DT_SMART_PREVIEW_SUBDIR "smart_previews"
#define DT_SMART_PREVIEW_DEFAULT_SIZE 2560
#define DT_SMART_PREVIEW_QUALITY 90

// Build the path: <cachedir>/smart_previews/sp-<imgid>.jpg
char *dt_smart_preview_path(const dt_imgid_t imgid)
{
  char cachedir[PATH_MAX] = { 0 };
  dt_loc_get_user_cache_dir(cachedir, sizeof(cachedir));
  return g_strdup_printf("%s/" DT_SMART_PREVIEW_SUBDIR "/sp-%d.jpg",
                         cachedir, imgid);
}

gboolean dt_smart_preview_exists(const dt_imgid_t imgid)
{
  char *path = dt_smart_preview_path(imgid);
  const gboolean exists = g_file_test(path, G_FILE_TEST_IS_REGULAR);
  g_free(path);
  return exists;
}

gboolean dt_smart_preview_generate(const dt_imgid_t imgid, int max_size)
{
  if(max_size <= 0) max_size = DT_SMART_PREVIEW_DEFAULT_SIZE;

  // Ensure output directory exists
  char cachedir[PATH_MAX] = { 0 };
  dt_loc_get_user_cache_dir(cachedir, sizeof(cachedir));
  char spdir[PATH_MAX];
  snprintf(spdir, sizeof(spdir), "%s/" DT_SMART_PREVIEW_SUBDIR, cachedir);
  g_mkdir_with_parents(spdir, 0700);

  // Pick the largest available mipmap that fits max_size.
  // DT_MIPMAP_8 is typically the full-resolution pre-scaled thumbnail.
  dt_mipmap_buffer_t buf;
  dt_mipmap_size_t mip = DT_MIPMAP_8;

  // Walk down from the largest mipmap until we have pixel data
  gboolean got_buf = FALSE;
  for(; mip >= DT_MIPMAP_4; mip--)
  {
    dt_mipmap_cache_get(&buf, imgid, mip, DT_MIPMAP_BEST_EFFORT, 'r');
    if(buf.buf && buf.width > 0 && buf.height > 0)
    {
      got_buf = TRUE;
      break;
    }
    dt_mipmap_cache_release(&buf);
  }

  if(!got_buf)
  {
    dt_print(DT_DEBUG_IMAGEIO,
             "[smart_preview] no mipmap available for imgid %d\n", imgid);
    return FALSE;
  }

  // Scale down to max_size if necessary
  int out_w = buf.width;
  int out_h = buf.height;
  if(out_w > max_size || out_h > max_size)
  {
    const float scale = (float)max_size / MAX(out_w, out_h);
    out_w = (int)(out_w * scale);
    out_h = (int)(out_h * scale);
  }

  // The mipmap buffer is RGBA 8-bit; we need RGB for JPEG write.
  // Allocate and convert in-place.
  const size_t n_pixels = (size_t)out_w * out_h;
  uint8_t *rgb = g_malloc(n_pixels * 3);
  if(!rgb)
  {
    dt_mipmap_cache_release(&buf);
    return FALSE;
  }

  // Simple nearest-neighbour downscale + RGBA→RGB strip
  const float sx = (float)buf.width  / out_w;
  const float sy = (float)buf.height / out_h;
  const uint8_t *src = (const uint8_t *)buf.buf;

  for(int y = 0; y < out_h; y++)
  {
    const int sy_i = (int)(y * sy);
    for(int x = 0; x < out_w; x++)
    {
      const int sx_i  = (int)(x * sx);
      const uint8_t *p = src + (sy_i * buf.width + sx_i) * 4;
      uint8_t *d       = rgb  + (y    * out_w    + x)    * 3;
      d[0] = p[0]; d[1] = p[1]; d[2] = p[2];
    }
  }

  dt_mipmap_cache_release(&buf);

  char *path = dt_smart_preview_path(imgid);
  const int rc = dt_imageio_jpeg_write(path, rgb, out_w, out_h,
                                       DT_SMART_PREVIEW_QUALITY, NULL, 0);
  g_free(rgb);

  if(rc != 0)
  {
    dt_print(DT_DEBUG_IMAGEIO,
             "[smart_preview] JPEG write failed for imgid %d: %s\n",
             imgid, path);
    g_free(path);
    return FALSE;
  }

  // Persist path and mtime in the DB
  struct stat st;
  const gint64 mtime = (stat(path, &st) == 0) ? (gint64)st.st_mtime : 0;

  sqlite3_stmt *stmt;
  // clang-format off
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "UPDATE main.images"
    " SET smartpreview_path = ?1, smartpreview_mtime = ?2,"
    "     flags = flags | " G_STRINGIFY(DT_IMAGE_HAS_SMART_PREVIEW)
    " WHERE id = ?3",
    -1, &stmt, NULL);
  // clang-format on
  DT_DEBUG_SQLITE3_BIND_TEXT(stmt, 1, path, -1, SQLITE_TRANSIENT);
  DT_DEBUG_SQLITE3_BIND_INT64(stmt, 2, mtime);
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 3, imgid);
  sqlite3_step(stmt);
  sqlite3_finalize(stmt);

  g_free(path);
  return TRUE;
}

void dt_smart_preview_delete(const dt_imgid_t imgid)
{
  char *path = dt_smart_preview_path(imgid);
  if(g_file_test(path, G_FILE_TEST_EXISTS))
    g_unlink(path);
  g_free(path);

  sqlite3_stmt *stmt;
  // clang-format off
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "UPDATE main.images"
    " SET smartpreview_path = NULL, smartpreview_mtime = 0,"
    "     flags = flags & ~" G_STRINGIFY(DT_IMAGE_HAS_SMART_PREVIEW)
    " WHERE id = ?1",
    -1, &stmt, NULL);
  // clang-format on
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 1, imgid);
  sqlite3_step(stmt);
  sqlite3_finalize(stmt);
}

void dt_smart_preview_generate_collection(int max_size)
{
  if(max_size <= 0) max_size = DT_SMART_PREVIEW_DEFAULT_SIZE;

  sqlite3_stmt *stmt;
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "SELECT id FROM main.images"
    " WHERE (flags & " G_STRINGIFY(DT_IMAGE_HAS_SMART_PREVIEW) ") = 0",
    -1, &stmt, NULL);

  while(sqlite3_step(stmt) == SQLITE_ROW)
  {
    const dt_imgid_t id = sqlite3_column_int(stmt, 0);
    dt_smart_preview_generate(id, max_size);
  }
  sqlite3_finalize(stmt);
}
