# Vimora brand guidelines

## The mark
A hat over a shield-shaped face. It is a single flat colour; there is no gradient, shadow or
outline. Do not redraw, rotate, stretch, recolour parts of it or add text inside it.
The vector master is `branding/source/vimora-master.svg` (traced from the approved artwork);
everything else is generated from it by `python3 branding/tools/build.py`.

## Colours
| Use | Value |
|---|---|
| Brand green (the mark, accents) | `#7DBA28` |
| Wordmark on light backgrounds | `#1D1F1B` |
| Wordmark / mark on dark backgrounds | `#FFFFFF` |
| Monochrome on light | `#000000` |

## Files (SVG and PNG, transparent background)
| Folder | What |
|---|---|
| `source/` | `vimora-master.svg`, the master |
| `logo/primary/` | green mark |
| `logo/monochrome/` | black, white and green single-colour marks |
| `logo/horizontal/` | mark + "VIMORA" (always capitals) on the right: normal (dark text), `-on-dark`, `-black`, `-white` |
| `logo/vertical/` | the same, mark above the word |
| `app-icons/` | square app icon (SVG + PNG 16 … 1024) and `windows/vimora.ico` |
| `system-tray/light/` | **black** icons for light panels (16 … 64 px) |
| `system-tray/dark/` | **white** icons for dark panels (16 … 64 px) |
| `tools/` | the build script and the wordmark outline (Days One, converted to a path; the font file is in `fonts/`) |

`assets/brand/` holds the copies the builds read (window icon, `.ico`, `.deb` icon, tray, and
the picture the programs draw inside their windows). It is overwritten by the build script.

## Theme behaviour
* In the program windows (navbar, lock screens, tray window) the green mark is used on both
  light and dark surfaces.
* The tray / notification-area icon is one colour: **white on a dark taskbar, black on a light
  one**, like a messenger's tray icon. It follows the system while the program runs (checked every
  3 seconds, no restart). Windows: the taskbar's own `SystemUsesLightTheme`. KDE: the colour
  scheme. GNOME / Ubuntu: the top bar is always dark, so the icon is always white.
* Wherever a single colour is required, use black on light and white on dark.

## Clear space and size
* Keep clear space around the mark of at least 15 % of its height (the files already include a
  small margin).
* Minimum size: mark 16 px (tray, favicon); mark with the word 96 px wide.
* Small sizes (16–24 px) are rendered from the vector at that size, never scaled from a larger
  picture.

## Changing the logo
Replace `branding/source/vimora-master.svg`, run `python3 branding/tools/build.py` (needs
`resvg` and Pillow), rebuild. The programs, installer, lock screens and tray all take it from
`assets/brand/`. Not generated: macOS `.icns` (Vimora ships for Windows and Linux only).
