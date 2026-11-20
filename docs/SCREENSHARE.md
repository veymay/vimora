# Vimora screen sharing — findings, architecture, status

Written 2026-11-20 after a real audit of the developer's machine and classroom boards. Everything
under "Evidence" was measured, not assumed. What is **not** done is listed plainly at the end.

## Evidence

### This computer (the developer's laptop)

| item | result |
|---|---|
| session | Wayland, GNOME (Ubuntu), kernel 7.3 |
| GPU | Intel Iris Plus (Ice Lake), `iHD` VA-API driver 26.2.4, `/dev/dri/renderD128` |
| VA-API H.264 encode | only the **low-power** entrypoint (`EncSliceLP`): Main, High, Constrained Baseline |
| GStreamer | `pipewiresrc`, `ximagesrc`, `vapostproc`, `vah264lpenc`, `x264enc`, `openh264enc`, `h264parse`, `mpegtsmux`, `rtph264pay` present; `vah264enc` absent |
| Wi-Fi | `wlp0s20f3` managed on 5 GHz channel 52 (a DFS channel) + `p2p-dev` interface; supports P2P-client/GO/device; combination `managed ≤1, P2P ≤1, #channels ≤2` (the P2P group may use a second channel) |
| P2P access | `wpa_supplicant`'s control socket is root-only here; NetworkManager exposes a `wifi-p2p` device (`p2p-dev-wlp0s20f3`) — the supported route for an unprivileged app |
| PipeWire / portals | PipeWire, WirePlumber, `xdg-desktop-portal` (+ GNOME backend) running |

**Encoder measurements** (real encode of a test stream, `crates/vimora-share/examples/capabilities.rs`):

| encoder | 720p30 | 1080p30 | rate control |
|---|---|---|---|
| `vah264lpenc` (hardware) | 112 fps | 51 fps | **CQP only** — cannot hold a bitrate |
| `x264enc` (software) | 74 fps | 32 fps | bitrate |
| `openh264enc` (software) | 124 fps | 65 fps | bitrate |

Decision: on this machine **hardware is not used** (CQP gives an unpredictable bitrate on a Wi-Fi
link; a constant-quantizer control loop is possible later). `x264enc` at 1080p30 has no margin
(32 fps), so `openh264enc` is chosen. The rule is in `probe::choose`: hardware only if its real test
passed, it is fast enough with a ×1.3 margin *and* it can hold a bitrate; else the best software
encoder that is fast enough.

### The classroom boards

* `1-XONA` (192.168.1.43) and `2-XONA` (192.168.1.29) are **Google Chromecast Ultra**-class
  receivers: model `Chromecast Ultra`, manufacturer `Google Inc.`, cast build `1.36.151708`, system
  build `PPR1.180610.011`. (The commercial board model/firmware behind them is unknown and is not
  invented here.)
* Their TLS certificate on 8009 is **X.509 version 1**, `sha1WithRSAEncryption`, RSA-2048,
  **identical on both boards** and valid for only two days. A normal verifier fails with
  `UnsupportedCertVersion`; fingerprint pinning is useless (shared, rotating).
* Launching app `CC1AD845` shows `displayName: "Chrome Mirroring"` on these boards; a Default Media
  Receiver `LOAD` (our picture mode) gets **no answer at all** (17 pictures sent, 0 acknowledged).
  So these boards take pictures only through Cast *streaming* (OFFER/ANSWER on
  `urn:x-cast:com.google.cast.webrtc`), which is what Chrome uses — and why Chrome casting works but
  freezes on this old firmware.
* `192.168.1.19` (the address reported earlier for the Miracast side) did not answer on 8008/8009.
* Wi-Fi Direct scan data supplied by the developer: `wfd_subelems 00000600111c440032` decodes
  (`vimora-share::wfd`) to **primary sink, session available, RTSP port 7236, max 50 Mbit/s,
  no content protection**.

## Architecture

```
capture ─▶ normalise ─▶ bounded newest-wins queue ─▶ encoder ─▶ transport adapter ─▶ receiver
(portal+PipeWire | X11 | Windows GDI)   (vimora-share::queue)   (vimora-share::probe)    ├ Cast
                                                                                         └ Miracast (WFD)
```

| piece | where | state |
|---|---|---|
| Cast discovery (mDNS) | `vimora-cast::Discovery` | done, used by the drawer |
| Cast control channel (CASTV2 over TLS) | `vimora-cast::session`, `::proto` | done, **verified on the real board** |
| Cast TLS for legacy certificates | `vimora-cast::tls` | done (below) |
| Receiver diagnostics (read-only) | `vimora-cast::diag` + `examples/probe.rs` | done, verified on the real board |
| Picture mode (Default Media Receiver) | `vimora-cast::session` | works against the mock; **rejected by the real boards**; reported as failed, never as "connected" |
| Cast streaming (OFFER/ANSWER, Cast RTP, AES-CTR, RTCP SR, VP8 via GStreamer) | `vimora-cast::stream`, `::encoder`, `::session` | **implemented 2026-11-20**: the real board answers the OFFER (`ANSWER ok`, 1920×1080@30) and sends RTCP back, state becomes Casting in ~1 s; **picture on the board not yet confirmed by a person**; no retransmission/PLI handling; Linux teacher only (needs GStreamer: `gstreamer1.0-tools`, `-plugins-good`, `-plugins-bad`); a Windows teacher needs an in-process encoder (not done) |
| Miracast / Wi-Fi Direct sender | — | **not implemented** |
| Capability probe + real encode test | `vimora-share::probe` | done, verified on this machine |
| Bounded queue with drop/age statistics | `vimora-share::queue` | done, unit-tested |
| Session state machine (timeouts, retries, cleanup) | `vimora-share::machine` | done, unit-tested; not yet driven by a transport |
| Backoff | `vimora-share::backoff` | done |
| Receiver profiles (data) | `vimora-share/profiles.json` | done: generic Cast, generic Miracast, observed Chromecast Ultra |
| Capture backends | `vimora-core::capture` (X11, Windows), `wayland_cast` (portal) | existing; pointer is drawn into frames |
| UI | the "Wireless displays" drawer | list, connect/stop, honest states; labelled experimental |
| Audio | — | not implemented |
| Adaptive bitrate / benchmark mode / long-run tests | — | not implemented |

### TLS compatibility decision (Cast, X.509 v1)

`vimora-cast::tls::CastVerifier` is used **only** for the Cast control channel to a display chosen from
the discovery list. It (1) parses the certificate structurally (versions 1–3; truncated, trailing,
alien-version and garbage certificates are refused — tests), and (2) verifies the TLS handshake
signature against *that certificate's own key* (RSA PKCS1/PSS, ECDSA, Ed25519 — tests), so the peer
must hold the key it presented. Its identity is **not** established: a hostile device on the same
network could pose as the display and receive what is cast. Cast closes that with *device
authentication* (a challenge answered with a certificate chain to Google's Cast CAs); implementing it
needs the Cast root certificates and is listed below. Every other TLS connection in Vimora keeps full
verification; nothing global was relaxed.

### Certificates and signing (what is actually needed)

* **Release signing**: updates are already signed with Vimora's own Ed25519 key (`RELEASE_KEYS`).
  Authenticode (Windows) or distro signing are optional extras for user-facing trust prompts.
* **Driver signing**: not needed — no kernel driver is used or planned.
* **TLS**: see above; no certificate has to be bought.
* **Miracast / Wi-Fi Alliance certification**: a separate commercial programme for branded products;
  not needed to implement the source role.

## Next steps (in order)

1. **Cast streaming**: confirm the picture on the board; handle receiver feedback (NACK/PLI,
   keyframe on demand), adapt bitrate from receiver reports, in-process encoder for Windows,
   audio.
2. Device authentication for Cast (needs the Cast root CA list).
3. Miracast source: NetworkManager `wifi-p2p` (D-Bus) discovery and group, then RTSP source
   (M1–M7, WFD capability exchange), H.264 in MPEG-TS over RTP, teardown and cleanup. P2P must not
   touch the managed Wi-Fi link without the user's confirmation.
4. Wayland capture through the portal straight into the encoder (`pipewiresrc` → `appsink`).
5. Audio, adaptive bitrate, benchmark mode, 30/60/120-minute runs, compatibility matrix.

## Compatibility matrix

| device | protocol | result | tested |
|---|---|---|---|
| Chromecast Ultra-class board "1-XONA" (fw 1.36) | Cast control channel | TLS + status OK | 2026-11-20 |
| same | Cast picture mode | not accepted by the display | 2026-11-20 |
| same | Cast streaming (VP8, 720p15 start) | OFFER accepted, RTCP received, ~15 fps sent for 13 s; visual result to be confirmed on the board | 2026-11-20 |
| Hisense board (model unknown) | Miracast | not implemented; WFD data decoded | — |

Nothing else is marked as tested.
