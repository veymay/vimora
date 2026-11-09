# Vimora: project notes for Claude Code

Open-source classroom management (NetSupport School / Veyon alternative), Rust + egui.
The owner writes in Uzbek; answer in Uzbek.

## Layout
- `apps/vimora-server` – teacher console (egui). UI in `src/app/`: `navbar.rs` (navbar, tabs row,
  status bar), `grid.rs` (cards + list view, filters, context menu), `settings.rs`, `forms.rs`
  (dialogs), `overlay.rs`, `viewer.rs`. Translations: `src/i18n_table.rs` (Uzbek text is the key;
  every `tr()`/`tf!()` string needs a row, a test checks it).
- `apps/vimora-client` – student agent. `crates/vimora-core` (protocol), `crates/vimora-ui`
  (design system: palette, fonts, icons, arrow popups), `crates/vimora-update` (self-update).
- UI rules: no border radius anywhere, accent #7dba28, uppercase button/tab labels, constant font
  weight in every state, tooltips without delay (`.tip()` with an arrow).

## Dev aids (console)
`VIMORA_DEMO=14` fills the console with fake computers (nothing is saved);
`VIMORA_OPEN=settings|settings:appearance|power|net|room|...` opens a dialog;
`VIMORA_MENU=1` opens a context menu; `VIMORA_SCREENSHOT=out.jpg` (+ `VIMORA_SCREENSHOT_DELAY`) saves a
screenshot and exits. Use `XDG_CONFIG_HOME=<dir>` to run with a throw-away config.

## CI / release
- CI needs: `cargo fmt --all --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`.
- Releases come from tag `v<version>` (version in root `Cargo.toml`) via `.github/workflows/release.yml`.
- GitHub Actions has been blocked by a billing issue. v0.3.0 was published by hand: the Linux
  `.deb` files were built locally. The Windows installer is built on a Windows PC with
  `windows-release.bat` (double click): it builds, creates `dist\vimora-<ver>-windows-x64-setup.exe`
  with Inno Setup, adds its hash to the release's `SHA256SUMS` and uploads both (needs `gh`).
  Log: `dist\windows-release.log`.
- Windows installer can also be built on Linux, with no Windows PC and no Actions:
  `sudo apt install mingw-w64 nsis`, then `installer/windows/build-from-linux.sh` (cross-compiles
  with the `x86_64-pc-windows-gnu` target and builds `installer/windows/vimora.nsi` with NSIS).
  It takes the same silent switches the self-updater passes (`/VERYSILENT /TYPE=student ...`).
  From 0.3.7 on, Windows releases are made this way; the Inno Setup script is the fallback.
- Do not move or recreate existing tags; do not force push; ask before publishing releases.

## Compatibility rules (old and new builds must keep working together)
- The wire format (bincode) is positional. Never insert, reorder or remove enum variants or
  struct fields in `vimora-core/src/protocol.rs` or in the IPC types of `service.rs`
  (`IpcReq`, `IpcResp`, `PrivStatus`): only **append** at the end. A test
  (`variant_indices_never_move`) pins the indices.
- Do not bump `PROTOCOL_VERSION` for additions; peers refuse each other only below
  `MIN_PROTOCOL_VERSION`. Receivers skip messages they cannot decode (`net::is_undecodable`).
- Releases 0.3.1–0.3.3 shipped with shifted variants (wire-incompatible with 0.3.0 and later
  builds); update all PCs to 0.3.4 or newer.
- The agent is single-instance per user (`single.rs`): a new copy ends the previous one.
- Student windows (lock screen, chat, popup) pick a graphics backend that works on the PC:
  the agent runs `vimora-client ui-probe` with `VIMORA_RENDERER=glow`, then `wgpu`, and passes the
  winner to its children. A lock screen that dies at once switches backend and reports an alert.
- Dev aids (agent): `VIMORA_OVERLAY_DEMO=<code>` shows the lock screen without a console
  (`VIMORA_OVERLAY_TYPE`, `VIMORA_OVERLAY_SCREENSHOT`).
- Console sounds are synthesised in `sound.rs` (no audio files). Block lists keep switched-off
  items with a leading `#` (`config::active_list`). Backups are the config as JSON (`.vbk`).
