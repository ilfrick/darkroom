# Darkroom — Rust Migration Plan

Incremental rewrite of the Darkroom codebase (C/GTK3) into Rust + GTK4 via the
`gtk4-rs` bindings. A flag-day full rewrite is ruled out: the codebase is
~500 k lines of C, builds on 50+ system libraries, and ships a live product.
The chosen strategy is **incremental FFI-boundary migration**: each subsystem
is replaced one at a time behind a stable C FFI layer, keeping the application
runnable throughout.

---

## Current status — 2026-05-30

**Phase 0 — Infrastructure**: complete. The Cargo workspace is live with
five crates and the cross-stage Docker dev image (`darkroom-rust-dev`) is
used for every `cargo check` / `cargo test` invocation.

**Phase 1 — Image pipeline**: in progress. 56 IOPs registered in Rust
(`crates/darkroom-core/src/iop`), 253 unit tests passing. Shared
infrastructure modules in `darkroom-core` (color, math, raw) have been
added as IOP migrations needed them. Each IOP migration is committed as
its own `Phase 2z+N` patch on top of the workspace.

**Phase 2 — Database**: not yet started. The crate skeleton
(`crates/darkroom-db`) exists with `tags.rs`, `image.rs`, `collection.rs`
placeholder files but no migrated logic.

**Phase 3 — GTK4 UI shell**: bootstrapped. `crates/darkroom-ui` now
depends on real `gtk4` 0.9 + `libadwaita` 0.7 and exposes
`darkroom_ui::run()` which boots an `adw::Application` and presents a
placeholder `ApplicationWindow`. The `darkroom-rs` binary (in
`crates/darkroom`) calls into it and exits cleanly. Production launch
still uses the C binary; the Rust binary is grown alongside it.

**Operations & packaging**: out of the original plan, but real work this
project has shipped — Docker entry-point persistence fixes
(`docker/cont-init-darkroom.sh`, `docker/kasmvnc-autostart.sh`), the
SIGTERM/SIGHUP signal handlers + 30 s periodic `dt_conf_save` in
`src/gui/gtk.c` that survive `docker stop`, the `--no-as-needed`
darkroom_core link + Rust-side SONAME so the loader resolves the
shared library, and the `Dockerfile.rust-dev` build environment.

### IOPs migrated so far (alphabetical)

`agx`, `atrous`, `basecurve`, `basicadj`, `bloom`, `censorize`,
`channelmixer`, `channelmixerrgb`, `clahe`, `colisa`, `colorbalance`,
`colorchecker`, `colorcontrast`, `colorcorrection`, `colorin`,
`colorize`, `colorout`, `colorzones`, `defringe`, `dither`, `exposure`,
`filmic`, `globaltonemap`, `graduatednd`, `grain`, `hazeremoval`,
`highlights`, `highpass`, `hotpixels`, `invert`, `levels`, `lowlight`,
`lowpass`, `lut3d`, `monochrome`, `negadoctor`, `overexposed`,
`overlay`, `primaries`, `profile_gamma`, `rasterfile`, `rawprepare`,
`relight`, `rgbcurve`, `rgblevels`, `shadhi`, `sigmoid`, `soften`,
`splittoning`, `temperature`, `tonecurve`, `velvia`, `vibrance`,
`vignette`, `watermark`, `zonesystem`.

### Shared darkroom-core modules

| Module | Purpose | Consumers |
|--------|---------|-----------|
| `color` | RGB↔HSL, Lab↔XYZ↔ProPhoto, eval_exp, extrapolate_lut, apply_trc, get_rgb_matrix_luminance | overexposed, channelmixer, multi-IOP shared math |
| `math` | fastlog2, fastlog (IEEE-754 bit-twiddled approximations) | colorchecker, future log/exp-heavy IOPs |
| `raw` | fc_bayer, fc_xtrans, fcol (CFA primitives) | highlights, hotpixels X-Trans, future rawoverexposed / demosaic |

---

## Goals

- Memory safety (eliminate the entire class of C buffer-overflow/use-after-free bugs)
- Modern, actively-maintained UI toolkit (GTK4 + libadwaita, `gtk4-rs` 0.9+)
- Cargo-native build, `cargo test`, `cargo bench`, `cargo clippy` in CI
- Keep existing Lua scripting API (via `mlua`)
- Keep OpenCL GPU pipeline (`opencl3` crate)
- End state: `cargo build --release` produces the full binary; CMake deleted

---

## Architecture overview

```
┌───────────────────────────────────────────┐
│              GTK4 UI shell (Rust)         │  Phase 3
│  lighttable · darkroom · panels · dialogs │
├───────────────────────────────────────────┤
│           Core services (Rust)            │  Phase 2
│  collection · tags · history · metadata  │
├───────────────────────────────────────────┤
│          Image pipeline (Rust)            │  Phase 1
│  pixelpipe · IOPs · demosaic · OpenCL    │
├───────────────────────────────────────────┤
│    C FFI shim (darkroom-sys crate)        │  Phase 0
│  bindgen-generated bindings to remaining  │
│  C code; shrinks to zero in Phase 4      │
└───────────────────────────────────────────┘
```

---

## Phase 0 — Infrastructure ✅ done

### What landed

- Cargo workspace at the repo root with five members:
  ```
  crates/darkroom-sys   # auto-generated C bindings
  crates/darkroom-core  # image pipeline + shared math/colour/raw helpers
  crates/darkroom-db    # collections / SQLite (skeleton only)
  crates/darkroom-ui    # GTK4 UI shell (boots a window today)
  crates/darkroom       # binary crate (darkroom-rs)
  ```
- `darkroom-sys/build.rs` generates bindings for the public C symbols the
  Rust side currently needs (`dt_imgid_t`, `NO_IMGID`, the `dt_iop_*`
  parameter accessors used by FFI shims).
- The build is driven from `src/CMakeLists.txt` via `find_program(CARGO …)`;
  Cargo is invoked with `CARGO_TARGET_DIR=build/cargo-target` so all build
  artefacts live under `build/`. RUSTFLAGS sets `-soname=libdarkroom_core.so`
  and `lib_darktable` carries `-Wl,--no-as-needed` so the dynamic loader
  resolves Rust symbols from the C plugins.
- `docker/Dockerfile.rust-dev` provides a persistent Rust + GTK4 +
  libadwaita build environment. `cargo check --workspace` and
  `cargo test --workspace` are run inside it from CI and from local
  development.

### Verification

```bash
docker build -t darkroom-rust-dev -f docker/Dockerfile.rust-dev .
docker run --rm -v "$PWD:/src" -w /src darkroom-rust-dev cargo check --workspace
docker run --rm -v "$PWD:/src" -w /src darkroom-rust-dev cargo test --workspace --release
```

---

## Phase 1 — Image pipeline (in progress)

**Goal:** Every IOP `process()` runs as safe Rust; the C pixelpipe calls
into Rust through a stable `extern "C"` FFI surface (`src/rust_ffi/darkroom_core.h`).

### Working model

Each migration is shipped as one self-contained patch named
`Phase 2<letter>` or `Phase 2z+<N>`. The contract is:

1. Write `crates/darkroom-core/src/iop/<name>.rs` with a `#[no_mangle]`
   `extern "C"` entry point and inline unit tests.
2. Register the module in `crates/darkroom-core/src/iop/mod.rs`.
3. Declare the function in `src/rust_ffi/darkroom_core.h`.
4. Replace the corresponding `DT_OMP_FOR` body in `src/iop/<name>.c`
   with a call into the Rust function.
5. Add the `.rs` file to the `DEPENDS` of the `darkroom_core_rust`
   custom target in `src/CMakeLists.txt`.
6. Commit + push to both `origin` (GitHub) and `gitea`.

### Progress summary

| | Count |
|---|---|
| IOPs migrated | 56 / 93 |
| Rust unit tests | 253 |
| Shared support modules | `color`, `math`, `raw` |

The C plugins that currently survive untouched fall into three buckets:

| Bucket | Examples | Reason for not migrating yet |
|--------|----------|------------------------------|
| Cold-path geometric distort_* | `borders`, `crop`, `flip`, `enlargecanvas`, `scalepixels`, `rotatepixels` | Each only has `distort_transform`/`distort_backtransform` loops that shift a coord buffer by a constant; migrating moves a memcpy from C to Rust with negligible benefit. |
| Multi-pass algorithm with heavy infrastructure | `sharpen` (per-thread Gaussian kernel), `demosaic` (Bayer/X-Trans interpolation kernels), `colormapping` (k-means + bilateral), `cacorrect` (tiled CA correction), `colorbalancergb` (Filmlight Yrg/Ych + JzAzBz), `colorequal`, `ashift` | Bodies depend on substantial shared infrastructure (Gaussian, bilateral grid, k-means, cluster lookup, JzAzBz space, …) that must be ported first. |
| Heavy colour-profile dependencies | `colorharmonizer`, `colortransfer`, parts of `rawoverexposed` | Need `dt_dev_distort_backtransform_plus`, `dt_ioppr_rgb_matrix_to_dt_UCS_JCH`, or LCMS-bound transforms; future work once `colorspaces.c` is in Rust. |

### Verification per IOP

- Per-function Rust unit tests with the same exact arithmetic as the C
  side (constants and bit patterns copied verbatim).
- The Rust crate is linked into `libdarktable.so` via `-Wl,--no-as-needed`
  so every IOP plugin resolves its `darkroom_*` symbols at startup; the
  Docker container regression-test on real photos is the integration
  signal.

### Anti-patterns

- No `unsafe` calls outside the FFI shim. The FFI layer reconstructs
  borrows from raw pointers, then the rest of the function is safe Rust.
- No silent numeric drift. When a C function uses a bit-twiddled
  approximation (`fastlog2`, `dt_fast_expf`), the Rust port copies the
  same magic constants and bit masks.

---

## Phase 2 — Database and collections (not yet started)

**Goal:** All SQLite queries go through `rusqlite`-based Rust; C uses the
same structs through `#[repr(C)]` FFI.

### Files to replace

| C file | Rust replacement |
|--------|-----------------|
| `src/common/collection.c` | `darkroom-db/src/collection.rs` |
| `src/common/image.c` | `darkroom-db/src/image.rs` |
| `src/common/tags.c` | `darkroom-db/src/tags.rs` |
| `src/common/history.c` | `darkroom-db/src/history.rs` |
| `src/common/metadata.c` | `darkroom-db/src/metadata.rs` |
| `src/common/film.c` | `darkroom-db/src/film.rs` |

### Key Rust types

```rust
#[repr(C)]
pub struct DtImage {
    pub id: i32,
    pub film_id: i32,
    pub width: i32,
    pub height: i32,
    pub flags: u32,
    // ...
}

pub struct Collection {
    conn: Arc<Mutex<Connection>>,
}

impl Collection {
    pub fn query(&self, rules: &[CollectRule]) -> Result<Vec<DtImage>> { ... }
}
```

### Order of attack

1. `tags` (smallest, well-isolated SQL surface) — proves the FFI pattern.
2. `metadata` (key/value writes by image ID).
3. `film` (top-level container; thin DAO).
4. `collection` (collect rules query builder — biggest jump in complexity).
5. `image` (image rows; deepest coupling, do last).
6. `history` (per-image edit history; touches the IOP pipeline too).

Each lands as a `Phase 2-db-N` patch following the same contract as the
IOP migrations: Rust struct + extern "C" trampoline + thin C wrapper.

---

## Phase 3 — UI shell: GTK4 + gtk4-rs (bootstrapped)

**Goal:** The entire GTK3 UI is replaced with GTK4 + `gtk4-rs`. Highest-risk
phase, longest expected duration.

### Current state

- `crates/darkroom-ui` depends on `gtk4 0.9` (`features = ["v4_12"]`)
  and `libadwaita 0.7` (`features = ["v1_5"]`).
- `darkroom_ui::run()` boots an `adw::Application` with
  `application_id = "org.darkroom.Darkroom"` and presents a
  1280×800 `ApplicationWindow` carrying a placeholder label.
- `crates/darkroom/src/main.rs` (`darkroom-rs`) calls into
  `darkroom_ui::run()` and forwards the exit code.
- The production runtime still launches the C binary
  (`/usr/local/bin/darkroom`). When the Rust UI reaches feature parity
  with a given panel/view, the relevant C code is removed and the
  autostart script switches to `darkroom-rs`.

### GTK3 → GTK4 migration notes

| GTK3 pattern | GTK4 equivalent |
|---|---|
| `GtkBox pack_start/end` | `GtkBox append` |
| `GtkContainer::add` | widget-specific `.set_child()` |
| `gtk_widget_show_all` | widgets shown by default |
| `GdkEventButton` callbacks | `GtkGestureClick` controllers |
| `GdkEventKey` callbacks | `GtkEventControllerKey` |
| `gtk_dialog_run` (blocking) | async `.present()` + response signals |
| `GtkFileChooserDialog` | `GtkFileDialog` (GTK 4.10+) |
| Cairo `draw` signal | `GtkDrawingArea::set_draw_func` |
| `GtkCellRenderer` in trees | `GtkListView` + `GtkColumnView` |
| Custom `GtkWidget` subclass | `glib::wrapper!` macro + `impl ObjectSubclass` |

### Migration order

| Priority | View / panel | Notes |
|----------|-------------|-------|
| 1 | Application shell (`AdwApplicationWindow`) | done (placeholder) |
| 2 | Lighttable thumbnail grid | `GtkGridView` + `GtkListItemFactory` |
| 3 | Darkroom editing view | `GtkDrawingArea` + overlays |
| 4 | Collections panel | `GtkListView` + `GtkTreeExpander` |
| 5 | History panel | `GtkListView` |
| 6 | Develop modules (IOPs) | one panel per IOP |
| 7 | Export dialog | `GtkDialog` → async `GtkFileDialog` |
| 8 | Preferences | `AdwPreferencesWindow` |
| 9 | Import / geotagging / map | last, most complex |

### Crate layout for the UI

```
crates/darkroom-ui/
  src/
    lib.rs           # adw::Application boot (done)
    app.rs           # global state, action map
    lighttable/
      mod.rs
      thumbnail.rs   # GtkListItemFactory impl
      culling.rs
    darkroom/
      mod.rs
      pixbuf_display.rs
      iop_panel.rs
    panels/
      collect.rs
      history.rs
      tagging.rs
      export.rs
    dialogs/
      preferences.rs
      about.rs
```

### Production Dockerfile changes (deferred)

The production Dockerfile (`docker/Dockerfile`) currently only installs
GTK3 libraries — it builds and runs the C binary plus the Rust
`libdarkroom_core.so`. Once the Rust UI is past the proof-of-concept
stage, the production image gets a parallel update:

1. Add `libgtk-4-dev libadwaita-1-dev` (+ a few transitive deps) to the
   builder stage.
2. Add `cargo build --release --workspace` after the CMake build and
   install `target/release/darkroom-rs` into `/opt/darkroom/bin/`.
3. Add `libgtk-4-1 libadwaita-1-0` to the runtime stage.
4. Initially `docker exec darkroom darkroom-rs` lets users opt into the
   Rust UI alongside the working C app.
5. When the Rust UI reaches feature parity, flip the autostart script.

---

## Phase 4 — Remove C entirely (future)

**Goal:** Zero C source files remain; build is pure Cargo.

### Tasks

1. Delete `CMakeLists.txt` and all `CMakeLists.txt` sub-files.
2. Move asset installation (`share/darktable/`) to `build.rs` or a custom
   Cargo xtask (`cargo xtask install`).
3. Delete `build.sh`.
4. Update Docker builder stage:
   ```dockerfile
   RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
   RUN cargo build --release && cargo install --path crates/darkroom
   ```
5. Update CI: remove CMake job, keep Rust job.

---

## Operations & packaging (cross-cutting)

Work outside the original phase model that has nonetheless shipped:

- `docker/Dockerfile`: builder stage links Rust crate; runtime stage
  install path documented; container entry-point hardened.
- `docker/cont-init-darkroom.sh`: `/config/{darkroom,cache}`
  created with `PUID:PGID` ownership and 0750 perms before the desktop
  session runs.
- `docker/kasmvnc-autostart.sh`: traps SIGTERM/SIGINT/SIGHUP and
  forwards them to the running darkroom child, then waits up to ~15 s
  for clean shutdown so `dt_conf_save` runs.
- `src/gui/gtk.c`: `g_unix_signal_add` handler that calls
  `dt_control_quit()` on signal delivery, plus a 30 s periodic
  `g_timeout_add_seconds` that flushes `dt_conf` to disk as a
  belt-and-braces measure against SIGKILL'd shutdowns.

These fixes were prerequisites for the IOP migration cadence: without
them every `docker stop` was wiping user settings, which would have
made the per-phase verification cycle untenable.

---

## Effort and risk summary

| Phase | Status | Risk | Key dependencies |
|-------|--------|------|-----------------|
| 0 — Infrastructure | ✅ done | Low | `bindgen`, `cc` crate |
| 1 — IOP pipeline | 56 / 93 IOPs migrated | Medium | `lcms2`, `opencl3`, `rawspeed` C lib |
| 2 — Database | not started | Low | `rusqlite`, `r2d2` |
| 3 — UI (GTK4) | bootstrapped | High | `gtk4-rs` ≥ 0.9, `libadwaita-rs` |
| 4 — Remove C | future | Medium | all prior phases complete |

The remaining IOP migrations follow the same cadence as the recent
`Phase 2z+N` patches. Phases 2 and 3 can proceed in parallel with the
tail of Phase 1 since they touch disjoint subsystems.

---

## Key crate dependencies

```toml
[dependencies]
gtk4       = { version = "0.9", features = ["v4_12"] }       # in use today
libadwaita = { version = "0.7", features = ["v1_5"] }        # in use today
glib       = "0.20"                                          # in use today
rusqlite   = { version = "0.31", features = ["bundled"] }    # workspace dep, Phase 2 consumer
rayon      = "1"
anyhow     = "1"
tracing    = "0.1"
cairo-rs   = "0.20"   # arrives with Phase 3 (drawing area work)
gdk4       = "0.9"    # arrives with Phase 3 (input controllers)
lcms2      = "6"      # arrives when colorin/colorout migrate the LCMS calls
opencl3    = "0.9"    # arrives when the OpenCL kernels move out of C
mlua       = { version = "0.10", features = ["lua54", "vendored"] } # arrives when Lua scripting moves
cbindgen   = "0.27"   # build-dep for generating C headers from Rust
bindgen    = "0.70"   # build-dep for darkroom-sys
```

---

## What is NOT in scope

- Replacing the `rawspeed` C++ library (keep as a vendored submodule via `cc` crate)
- Replacing `lensfun` (keep as system library)
- Replacing `gmic` (keep as system library)
- Rewriting the Lua plugin API surface (keep `mlua` as a thin wrapper)
