# SimRig

A Linux app, in Rust with a Slint GUI, that configures sim-racing rigs of any brand through
plugins (each an adapter plus a driver).

## Read first

- `docs/architecture.md`: the architecture to follow. Principles (§1), the domain model and
  standard interfaces (§3), plugins (§5), drivers and their recipes (§6), how a brand is added
  (§11), and the decisions taken (§12).
- `PRODUCT.md` and `DESIGN.md`: who the app is for, and the design system (tokens, components,
  rules). Every UI change follows them.
- `docs/development.md`: building, running on real hardware, repository layout, checks, releases.
- `docs/requirements.md`: what SimRig must do.
- `docs/design/flows.md`: how people move through the app.
- `THIRD-PARTY.md`: what is bundled or derived, and from whom.

## Rules

- **Responsibilities stay apart.**
  - The core (model, host, GUI) knows no brand: brand logic goes in its plugin.
  - What several plugins share goes in `crates/simrig-plugin-kit`.
  - The GUI only presents.
  - A feature lives in its own module, wired in like the others.
- **No root at runtime.** The password is asked only when something touches the system. Prefer an
  app's own setting to custom daemons or scripts added to the system.
- **Unsupported items stay visible**, disabled with their reason. Items that work only in part
  carry a caveat (a yellow sign with a tooltip).
- **Third-party drivers are pinned git submodules**, used unmodified and credited in
  `THIRD-PARTY.md`. A driver the user installed by other means is kept as it is.
- **Changing a driver** (`plugins/*/driver`): record the fingerprints, and raise `revision` when
  installed copies need the change (see `docs/development.md`). The recipe copies in each driver
  stay identical to `crates/simrig-driver/recipes`.
- **Language.**
  - Code, comments and docs are in English.
  - UI copy is impersonal and technical ("4 devices found", never "we found"), without em dashes.
- **README** is for users first.
- **No mockups in the repository**: mockups made while designing stay out of it.
- **Checks before saying it is done**: `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace`. Hooks and plugins are tested end to end
  with `simrig_driver::testing` and `simrig_host::testing`; screens can be checked with
  `simrig --screenshot` (see `docs/development.md`).
