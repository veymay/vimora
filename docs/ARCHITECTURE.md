# Vimora architecture

Two programs, one protocol.

```
 teacher (egui, user session)                         student PC
 ┌──────────────────────────┐   TCP 47801 (encrypted)  ┌───────────────────────────────┐
 │ app/  UI (navbar, grid,  │ <──────────────────────> │ agent  (user session)         │
 │       forms, overlay …)  │   UDP 47800 discovery    │   session.rs  handshake+loop  │
 │ net.rs  hub, sessions    │                          │   tasks.rs / supervisors      │
 │ config.rs  settings, key │                          │         │ IPC 127.0.0.1:47802 │
 │ lanupdate.rs  LAN update │                          │         ▼                     │
 └──────────────────────────┘                          │ service (SYSTEM / root)       │
                                                       │   service.rs  IPC server      │
                                                       │   privileged.rs  Engine       │
                                                       │   netcontrol.rs  sites        │
                                                       │   shield/  UWF / overlayroot  │
                                                       │ guard (watches the service)   │
                                                       └───────────────────────────────┘
```

## Crates

| crate | role |
|---|---|
| `vimora-core` | wire protocol (`protocol.rs`), framing (`net.rs`), identity/signatures (`auth.rs`), encrypted channel (`secure.rs`), unlock-code hash (`pin.rs`), screen capture, input injection |
| `vimora-ui` | design system: palette, fonts, icons, buttons, popups (no rounded corners, accent `#7dba28`) |
| `vimora-update` | self-update: signed release lists, package verification, installers |
| `vimora-tray` | tray icon |
| `apps/vimora-teacher` | the teacher program |
| `apps/vimora-student` | the student program: agent, privileged service, setup wizard, tray window |

## Trust model (who may do what)

1. **Teacher** owns an Ed25519 identity (`identity.key`, mode 0600). Student PCs pin its public key
   on first contact (trust on first use) and obey only that key afterwards.
2. **Agent** runs as the *student user*, so it is **not trusted**. It does everything that needs no
   privilege (lock screen, chat, screen capture, opening a URL …).
3. **Service** runs as SYSTEM/root. It executes a privileged command (`PrivCommand`: block sites,
   freeze, update policy, uninstall …) only when it carries a **teacher signature** bound to the
   session nonce and an increasing counter. The agent merely relays signed bytes.
4. **Releases** are verified against `RELEASE_KEYS`: the service installs only a `SHA256SUMS` whose
   signature checks out, copies the installer into its own folder, checks the hash again and only
   then runs it.

## Session handshake

```
T→S Hello{version, teacher_nonce, teacher_key}
S→T Hello{version, student_nonce, info}
S→T SecureOffer{student_eph}                         (new in 0.3.63, skipped by older teachers)
T→S Welcome{sig = Sign(student_nonce, teacher_nonce), stream}
T→S Secure{teacher_eph, sig = Sign(nonces, teacher_eph, student_eph)}
      ── from here every record is ChaCha20-Poly1305, one key per direction (X25519 + HKDF) ──
```

Pre-authentication frames are limited to 64 KiB. A student that has once talked encrypted with its
teacher refuses a plain session with it afterwards (no downgrade). Older builds without the
channel still connect (plain) until their PC has been updated.

## Wire compatibility rules

Messages are positional `bincode`. Never insert, reorder or remove enum variants or struct fields in
`protocol.rs` or the IPC types in `service.rs`; only append. `variant_indices_never_move` pins the
indices. Receivers skip messages they cannot decode.

## State on a student PC

* `ProgramData\Vimora` (Linux `/etc/vimora`): `service.json` (pinned teacher), `net-policy.json`,
  `access-policy.json` — written by the service, excluded from the write filter, re-applied at start.
* Per user: `paired-teacher`, `secure-teacher`, `last-teacher` in the config dir.
* Shadow mode (`shield/`): Windows UWF / Ubuntu overlayroot, with explicit "kept" paths.

## Where to look

* a UI dialog → `apps/vimora-teacher/src/app/forms.rs`, `settings.rs`, `overlay.rs`
* a new teacher command → `protocol.rs` (append!), `net.rs`, `agent/session.rs` or `privileged.rs`
* site/app block lists → `apps/vimora-teacher/src/blockcat.rs`; enforcement → `netcontrol.rs`
* releases → `build.sh`, `installer/windows/`
