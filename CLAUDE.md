# Vimora: project notes for Claude Code

Open-source classroom management (NetSupport School / Veyon alternative), Rust + egui.
The owner writes in Uzbek; answer in Uzbek.

## Layout
Only two programs exist: **teacher** and **student** (never "console", "client", "server",
"agent" in texts or names; "o'qituvchi" / "o'quvchi" in Uzbek).
- `apps/vimora-teacher` – the teacher program (egui). UI in `src/app/`: `navbar.rs` (navbar, tabs row,
  status bar), `grid.rs` (cards + list view, filters, context menu), `settings.rs`, `forms.rs`
  (dialogs), `overlay.rs`, `viewer.rs`. Translations: `src/i18n_table.rs` (Uzbek text is the key;
  every `tr()`/`tf!()` string needs a row, a test checks it).
- `apps/vimora-student` – the student program (agent + privileged service, setup window
  `wizard.rs`, tray window `about.rs`, small-window frame `smallwin.rs`, folders `layout.rs`). `crates/vimora-core` (protocol), `crates/vimora-ui`
  (design system: palette, fonts, icons, arrow popups), `crates/vimora-update` (self-update).
- Brand: master `branding/source/vimora-master.svg`; `python3 branding/tools/build.py` makes every
  variant (needs `resvg`, Pillow) and refreshes `assets/brand/` (what builds read). Name next to the
  logo: Days One, always capitals (`VIMORA`); UI text: Mona Sans (+ Geist Mono labels). Tray icon:
  white on dark panels, black on light, follows the system (`vimora-tray`).
- UI rules: no border radius anywhere, accent #7dba28, uppercase button/tab labels, constant font
  weight in every state, tooltips without delay (`.tip()` with an arrow).

## Dev aids (console)
`VIMORA_DEMO=14` fills the console with fake computers (nothing is saved);
`VIMORA_OPEN=settings|settings:appearance|power|net|room|...` opens a dialog;
`VIMORA_MENU=1` opens a context menu; `VIMORA_SCREENSHOT=out.jpg` (+ `VIMORA_SCREENSHOT_DELAY`) saves a
screenshot and exits. Use `XDG_CONFIG_HOME=<dir>` to run with a throw-away config.
`VIMORA_AUTOADD=1` adds new computers at once (no clicking). Test console + agent together without
touching the real console: run both inside `unshare -rn` with a veth pair (`ip addr add ... brd +`).

## CI / release
- CI needs: `cargo fmt --all --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`.
- Releases come from tag `v<version>` (version in root `Cargo.toml`) via `.github/workflows/release.yml`.
- GitHub Actions has been blocked by a billing issue, so releases are built locally.
- Build system (two entry points, output in `dist/` with `SHA256SUMS`, log `dist/build.log`):
  - Ubuntu: `./build.sh [all|linux|windows]`. Linux binaries are built with `cargo zigbuild` for
    glibc 2.35 (so the `.deb`s install on Ubuntu 22.04+; a plain local build would require this
    machine's glibc), packaged with `cargo deb`. Windows is cross-compiled (`x86_64-pc-windows-gnu`,
    static CRT) and packed with NSIS (`installer/windows/vimora.nsi`). Missing tools are installed
    on first run (`apt`: zig, mingw-w64, nsis; `cargo install`: cargo-zigbuild, cargo-deb).
  - Windows: `build.bat` (double click) → `installer/windows/build.ps1` (MSVC + NSIS, installs
    missing tools with winget). `build.bat upload` / `windows-release.bat` also upload the `.exe`
    to the existing `v<ver>` release and update its `SHA256SUMS` (needs `gh`).
  - The NSIS installer takes the silent switches the self-updater passes
    (`/VERYSILENT /TYPE=student ...`, plus `/LANG=uz|ru|en`, `/LAUNCH`). Interactive runs open
    `vimora-student setup-wizard <installer>` (language with flags → teacher/student → silent
    install); exit code 2 = no window possible, then the plain NSIS page is shown.
  - Windows folder: `C:\Program Files (x86)\Vimora` (fixed). Up to 0.3.27: `C:\Program Files\Vimora`
    with `vimora-client.exe` / `vimora-server.exe`. The installer moves such PCs, except a frozen
    student PC (`vimora-student is-frozen`): updated in place under the old name; the service
    moves it once the freeze is off (`layout::migrate`). Keep both names in process checks.
  - Student registrations (service tasks, Run `VimoraStudent`, firewall `Vimora Student`,
    updater tasks, `Files` folder, hidden uninstall entry) are done by `vimora-student setup`,
    not by NSIS, so the move can redo them. Files the teacher sends: `<install dir>\Files`
    (Ubuntu `/opt/vimora/files`), no folder in Downloads.
  - Only the teacher removes Vimora from a student PC (`PrivCommand::UninstallVimora` →
    `uninstall.exe /S /BYTEACHER`); the uninstaller refuses without `/BYTEACHER`.
  - Saved names that must not change: `service.json` field `console_key` (serde alias),
    per-user files `paired-console`, `last-console`, `client.json`.
- Release 0.3.8's `.deb` files were built before this and require glibc 2.44 (do not install on
  Ubuntu 22.04/24.04); rebuild with `./build.sh linux` before publishing.
- Do not move or recreate existing tags; do not force push; ask before publishing releases.
- Updates are signed: student PCs (0.3.27+) install only a `SHA256SUMS` whose `SHA256SUMS.sig`
  verifies against `RELEASE_KEYS` in `crates/vimora-update/src/lib.rs`. `build.sh` signs with
  `~/.config/vimora-release/release.key` (never in the repo; backup `../vimora-release.key.BACKUP`).
  Losing the key = PCs never update again; to rotate, add the new public key to `RELEASE_KEYS`
  first and keep the old one for a long time. Every release must upload `SHA256SUMS.sig`.
- Update paths on a PC: the service (every 30 min), the "Vimora Updater" tasks
  (`vimora-student update-now`, boot + every 4 h) and the console's LAN push (`lanupdate.rs`,
  `Command::InstallUpdate`). Future installers must keep accepting
  `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TYPE=student /COMPONENTS=agent`, and asset names
  must keep matching `*windows*.exe` / `vimora-student*.deb`.

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
  the agent runs `vimora-student ui-probe` with `VIMORA_RENDERER=glow`, then `wgpu`, and passes the
  winner to its children. A lock screen that dies at once switches backend and reports an alert.
- Dev aids (agent): `VIMORA_OVERLAY_DEMO=<code>` shows the lock screen without a console
  (`VIMORA_OVERLAY_TYPE`, `VIMORA_OVERLAY_SCREENSHOT`).
- Console sounds are synthesised in `sound.rs` (no audio files). Block lists keep switched-off
  items with a leading `#` (`config::active_list`). Backups are the config as JSON (`.vbk`).
