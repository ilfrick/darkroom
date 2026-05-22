/*
  This file is part of darktable,
  Copyright (C) 2025 darktable developers.

  darktable is free software: you can redistribute it and/or modify
  it under the terms of the GNU General Public License as published by
  the Free Software Foundation, either version 3 of the License, or
  (at your option) any later version.
*/

#pragma once

#include "common/image.h"
#include <glib.h>

// Smart Previews: JPEG proxies stored in cache so images can be edited
// without the original RAW file present (e.g. offline external drives).
// Storage: <cachedir>/smart_previews/sp-<imgid>.jpg

/** Returns TRUE if a smart preview JPEG exists on disk for this image. */
gboolean dt_smart_preview_exists(const dt_imgid_t imgid);

/** Returns the full path to the smart preview JPEG. Caller must g_free().
    The file may not exist yet; call dt_smart_preview_exists() first. */
char *dt_smart_preview_path(const dt_imgid_t imgid);

/** Generate (or refresh) a smart preview for one image.
    max_size: longest edge in pixels (default 2560 if <= 0).
    Returns TRUE on success. */
gboolean dt_smart_preview_generate(const dt_imgid_t imgid, int max_size);

/** Delete the smart preview file and clear DB columns for this image. */
void dt_smart_preview_delete(const dt_imgid_t imgid);

/** Generate smart previews for all images in the current collection.
    Runs synchronously; intended for CLI use. */
void dt_smart_preview_generate_collection(int max_size);
