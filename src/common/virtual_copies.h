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

// Virtual Copies: multiple independent edit histories for the same physical
// file. Each copy has its own history stack. The master image row has
// virtual_copy_of = NULL; all copies point back to the master's id.

/** Create a virtual copy of master_id. Returns the new image id on success,
    NO_IMGID on failure. The copy starts with a deep clone of the master's
    full history and masks stack. */
dt_imgid_t dt_virtual_copy_create(const dt_imgid_t master_id);

/** Delete a virtual copy. Only the copy row is removed; the physical file and
    the master image row are untouched. No-op if copy_id is not a virtual copy. */
void dt_virtual_copy_delete(const dt_imgid_t copy_id);

/** Return the master image id for copy_id, or NO_IMGID if copy_id is already
    a master (virtual_copy_of IS NULL). */
dt_imgid_t dt_virtual_copy_master(const dt_imgid_t copy_id);

/** Promote copy_id to master: clears virtual_copy_of so it becomes
    an independent image. The original master is unaffected. */
void dt_virtual_copy_promote(const dt_imgid_t copy_id);

/** Return TRUE if imgid is a virtual copy (virtual_copy_of IS NOT NULL). */
gboolean dt_virtual_copy_is_copy(const dt_imgid_t imgid);
