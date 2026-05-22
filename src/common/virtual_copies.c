/*
  This file is part of darktable,
  Copyright (C) 2025 darktable developers.

  darktable is free software: you can redistribute it and/or modify
  it under the terms of the GNU General Public License as published by
  the Free Software Foundation, either version 3 of the License, or
  (at your option) any later version.
*/

#include "common/virtual_copies.h"
#include "common/darktable.h"
#include "common/database.h"
#include "common/debug.h"
#include "common/history.h"
#include "common/image.h"
#include "common/image_cache.h"
#include "common/mipmap_cache.h"
#include "common/undo.h"

dt_imgid_t dt_virtual_copy_create(const dt_imgid_t master_id)
{
  if(!dt_is_valid_imgid(master_id)) return NO_IMGID;

  // Resolve the true master: if master_id is itself a copy, follow the link
  const dt_imgid_t root_id = dt_virtual_copy_master(master_id);
  const dt_imgid_t src_id  = dt_is_valid_imgid(root_id) ? root_id : master_id;

  sqlite3_stmt *stmt;

  // ── 1. Insert new images row copying all metadata from source ────────────
  // clang-format off
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "INSERT INTO main.images"
    "  (id, group_id, film_id, width, height, filename,"
    "   maker_id, model_id, camera_id, lens_id, exposure,"
    "   aperture, iso, focal_length, focus_distance, datetime_taken, flags,"
    "   output_width, output_height, crop, raw_parameters, raw_black, raw_maximum,"
    "   orientation, longitude, latitude, altitude, color_matrix,"
    "   colorspace, version, max_version,"
    "   history_end, position, aspect_ratio, exposure_bias, import_timestamp,"
    "   whitebalance_id, flash_id, exposure_program_id, metering_mode_id,"
    "   flash_tagvalue, virtual_copy_of)"
    " SELECT NULL, group_id, film_id, width, height, filename,"
    "        maker_id, model_id, camera_id, lens_id,"
    "        exposure, aperture, iso, focal_length, focus_distance, datetime_taken,"
    "        flags | " G_STRINGIFY(DT_IMAGE_IS_VIRTUAL_COPY) ","
    "        output_width, output_height, crop, raw_parameters,"
    "        raw_black, raw_maximum, orientation,"
    "        longitude, latitude, altitude, color_matrix, colorspace, NULL, NULL, 0,"
    "        (SELECT IFNULL(MAX(position),0) + 1 FROM main.images),"
    "        aspect_ratio, exposure_bias, import_timestamp,"
    "        whitebalance_id, flash_id, exposure_program_id, metering_mode_id,"
    "        flash_tagvalue, ?1"
    " FROM main.images WHERE id = ?1",
    -1, &stmt, NULL);
  // clang-format on
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 1, src_id);
  sqlite3_step(stmt);
  sqlite3_finalize(stmt);

  const dt_imgid_t newid = (dt_imgid_t)sqlite3_last_insert_rowid(
    dt_database_get(darktable.db));

  if(!dt_is_valid_imgid(newid)) return NO_IMGID;

  // ── 2. Deep-copy history stack ───────────────────────────────────────────
  // clang-format off
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "INSERT INTO main.history"
    "  (imgid, num, module, operation, op_params, enabled,"
    "   blendop_params, blendop_version, multi_priority, multi_name)"
    " SELECT ?1, num, module, operation, op_params, enabled,"
    "        blendop_params, blendop_version, multi_priority, multi_name"
    " FROM main.history WHERE imgid = ?2 ORDER BY num",
    -1, &stmt, NULL);
  // clang-format on
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 1, newid);
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 2, src_id);
  sqlite3_step(stmt);
  sqlite3_finalize(stmt);

  // ── 3. Deep-copy masks history ───────────────────────────────────────────
  // clang-format off
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "INSERT INTO main.masks_history"
    "  (imgid, num, formid, form, name, version, points, points_count, source)"
    " SELECT ?1, num, formid, form, name, version, points, points_count, source"
    " FROM main.masks_history WHERE imgid = ?2",
    -1, &stmt, NULL);
  // clang-format on
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 1, newid);
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 2, src_id);
  sqlite3_step(stmt);
  sqlite3_finalize(stmt);

  // ── 4. Copy IOP order list ───────────────────────────────────────────────
  // clang-format off
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "INSERT INTO main.module_order (imgid, iop_list, version)"
    " SELECT ?1, iop_list, version FROM main.module_order WHERE imgid = ?2",
    -1, &stmt, NULL);
  // clang-format on
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 1, newid);
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 2, src_id);
  sqlite3_step(stmt);
  sqlite3_finalize(stmt);

  // ── 5. Copy tags ─────────────────────────────────────────────────────────
  // clang-format off
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "INSERT INTO main.tagged_images (imgid, tagid, position)"
    " SELECT ?1, tagid,"
    "        (SELECT IFNULL(MAX(position),0) & 0xFFFFFFFF00000000 FROM main.tagged_images)"
    "        + (ROW_NUMBER() OVER (ORDER BY imgid) << 32)"
    " FROM main.tagged_images WHERE imgid = ?2",
    -1, &stmt, NULL);
  // clang-format on
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 1, newid);
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 2, src_id);
  sqlite3_step(stmt);
  sqlite3_finalize(stmt);

  // ── 6. Copy color labels ─────────────────────────────────────────────────
  // clang-format off
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "INSERT INTO main.color_labels (imgid, color)"
    " SELECT ?1, color FROM main.color_labels WHERE imgid = ?2",
    -1, &stmt, NULL);
  // clang-format on
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 1, newid);
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 2, src_id);
  sqlite3_step(stmt);
  sqlite3_finalize(stmt);

  // Invalidate mipmap thumbnails so the new copy gets its own
  dt_mipmap_cache_remove(newid);

  dt_print(DT_DEBUG_IMAGEIO,
           "[virtual_copy] created copy %d from master %d\n", newid, src_id);
  return newid;
}

void dt_virtual_copy_delete(const dt_imgid_t copy_id)
{
  if(!dt_is_valid_imgid(copy_id)) return;
  if(!dt_virtual_copy_is_copy(copy_id)) return;

  // dt_image_remove handles cascade deletion of history, masks, tags etc.
  dt_image_remove(copy_id);
}

dt_imgid_t dt_virtual_copy_master(const dt_imgid_t copy_id)
{
  if(!dt_is_valid_imgid(copy_id)) return NO_IMGID;

  sqlite3_stmt *stmt;
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "SELECT virtual_copy_of FROM main.images WHERE id = ?1",
    -1, &stmt, NULL);
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 1, copy_id);

  dt_imgid_t master = NO_IMGID;
  if(sqlite3_step(stmt) == SQLITE_ROW
     && sqlite3_column_type(stmt, 0) != SQLITE_NULL)
    master = sqlite3_column_int(stmt, 0);

  sqlite3_finalize(stmt);
  return master;
}

void dt_virtual_copy_promote(const dt_imgid_t copy_id)
{
  if(!dt_is_valid_imgid(copy_id)) return;

  sqlite3_stmt *stmt;
  // clang-format off
  DT_DEBUG_SQLITE3_PREPARE_V2(
    dt_database_get(darktable.db),
    "UPDATE main.images"
    " SET virtual_copy_of = NULL,"
    "     flags = flags & ~" G_STRINGIFY(DT_IMAGE_IS_VIRTUAL_COPY)
    " WHERE id = ?1",
    -1, &stmt, NULL);
  // clang-format on
  DT_DEBUG_SQLITE3_BIND_INT(stmt, 1, copy_id);
  sqlite3_step(stmt);
  sqlite3_finalize(stmt);

  // Flush from cache so flags are re-read
  dt_image_cache_remove(copy_id);
}

gboolean dt_virtual_copy_is_copy(const dt_imgid_t imgid)
{
  return dt_is_valid_imgid(dt_virtual_copy_master(imgid));
}
