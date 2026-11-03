# Axiom app icon

Axiom's own icon: a white "A" with an inspection probe on its crossbar, on a rounded blue tile
(`#4671ff`). It is covered by the repository's licence: see `LICENSE.txt`.

The interface uses a warm near-black and paper background with ink text (`theme.rs`), a deep
accent `#1659c4` (`theme::BRAND`) for filled controls so white text on them passes WCAG AA, a
bright accent `#2d81ff` for hover and a light accent `#74aaff` for blue text on dark surfaces.
Headings and text are DM Sans.

## Geometry

A 512-unit tile with 110-unit corners (the source's 21.5 %), the raven clipped to it. The macOS
icons pad the tile onto Apple's 824/1024 icon grid; every other render uses the full tile.

## Files

- `axiom.svg`: the master SVG (also the hicolor scalable icon), about 100 KB.
- `axiom-1024.png`: 1024 px render of the tile.
- `axiom.icns`: macOS bundle icon.
- `axiom.ico`: Windows icon (16, 20, 24, 32, 40, 48, 64, 128, 256).
- `hicolor/<size>/apps/ai.storyteller.axiom.png` (16–512) and
  `hicolor/scalable/apps/ai.storyteller.axiom.svg`: Linux icon theme.
- `trace.py`: traces the raven artwork (a 2000 px raster) into the SVG with vtracer.
- `packaging/icons.sh`: makes every other file above from the SVG. The app draws the SVG itself
  at the exact pixel size (title bar, welcome screen); the window icon is the 256 px PNG.

## Regenerate

```sh
# From the 2000 px raven artwork (needs numpy, Pillow and vtracer; svgo 3.3.2 shrinks the paths):
python trace.py raven.webp master.svg 1000 6 0
npx svgo@3.3.2 -p 1 master.svg -o axiom.svg
# Every render (needs resvg on PATH; the .icns needs macOS's iconutil):
packaging/icons.sh
```
