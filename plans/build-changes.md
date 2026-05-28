# Fix: Microsoft Store "Default Tile Icon" Rejection

## Problem

Microsoft Store rejected the submission with this note:

> "The available product tile icons include a default image. Tile icons must uniquely represent product, so users associate icons with the appropriate products and do not confuse one product for another."

## Root Cause Analysis

### 1. Simplistic SVG source icon

**File:** `src-tauri/icons/icon.svg`

The current SVG is a minimal placeholder — a single letter "B" on a dark (`#1a1a2e`) background:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect width="512" height="512" rx="64" fill="#1a1a2e"/>
  <text x="256" y="320" font-family="sans-serif" font-size="280" font-weight="bold" fill="#e94560" text-anchor="middle">B</text>
</svg>
```

This is the original Tauri scaffold icon that was never replaced with the actual Bango brand design from `design/logo.png`. While the PNG icons in `src-tauri/icons/` were regenerated from `design/logo.png` (commit `6df998c`, then `3d057a3`), the SVG source was left as the original placeholder.

### 2. Blank/default wide tile in generated MSIX

**File:** `src-tauri/gen/windows/Assets/Wide310x150Logo.png`

This file is only **554 bytes** for a 310×150 image — a nearly-blank default placeholder. The Windows `AppxManifest.xml.template` (line 41) references it:

```xml
<uap:DefaultTile Wide310x150Logo="Assets\Wide310x150Logo.png" />
```

Tauri's bundler generates this file during build, but because there's no explicit wide-tile source in `src-tauri/icons/`, it falls back to a default blank image. Microsoft reviewers see this as a generic/default tile.

### 3. Incomplete icon list in tauri.conf.json

**File:** `src-tauri/tauri.conf.json`

The `bundle.icon` array only lists 5 entries:

```json
"icon": [
  "icons/32x32.png",
  "icons/128x128.png",
  "icons/128x128@2x.png",
  "icons/icon.icns",
  "icons/icon.ico"
]
```

Missing from this list are all the Windows-specific tile sizes that exist in `src-tauri/icons/`:
- `Square30x30Logo.png`
- `Square44x44Logo.png`
- `Square71x71Logo.png`
- `Square89x89Logo.png`
- `Square107x107Logo.png`
- `Square142x142Logo.png`
- `Square150x150Logo.png`
- `Square284x284Logo.png`
- `Square310x310Logo.png`
- `StoreLogo.png`

While Tauri's bundler may auto-discover some of these, explicitly listing them ensures correct packaging and prevents fallback to defaults.

---

## Files to Change

### File 1: `src-tauri/icons/icon.svg` (UPDATE)

**Why:** This is the source-of-truth vector icon. It needs to be updated to match the actual Bango brand from `design/logo.png`. The current placeholder "B" is too generic and doesn't represent the product.

**Change:** Replace the entire SVG content with a proper branded icon that:
- Uses the Bango logo design from `design/logo.png` as reference
- Uses the Bango brand colors (Primary Indigo `#4F46E5`, Surface `#FCF8FF`)
- Maintains the 512×512 viewBox with `rx="64"` rounded corners
- Renders cleanly at all sizes (32px through 512px)

**Approach:** Since SVG can't directly embed raster images cleanly for all renderers, the SVG should be a vector representation of the Bango brand mark — an improved, recognizable icon that matches the logo aesthetic. Use the brand's indigo color scheme and a distinctive shape/monogram.

### File 2: `src-tauri/icons/Wide310x150Logo.png` (CREATE)

**Why:** The Windows manifest requires a wide tile (310×150 px) for the Start Menu. Currently this is auto-generated as a blank default. We need a proper branded wide tile.

**Change:** Create a new 310×150 PNG with:
- Bango logo centered-left on the brand background color (`#4F46E5` or `#1E293B`)
- "Bango" text to the right of the logo
- Consistent with the brand design system (Inter font family, rounded corners)

**Approach:** Use a Python script with Pillow to composite `design/logo-trans.png` (transparent background logo) onto a 310×150 canvas with the app name text. Alternatively, create an SVG source and convert with `rsvg-convert` or Inkscape.

### File 3: `src-tauri/tauri.conf.json` (UPDATE)

**Why:** The `bundle.icon` array must explicitly list all Windows tile icon files so the Tauri bundler includes them in the MSIX package correctly.

**Change:** Update `bundle.icon` to include all platform icons:

```json
"icon": [
  "icons/32x32.png",
  "icons/128x128.png",
  "icons/128x128@2x.png",
  "icons/icon.icns",
  "icons/icon.ico",
  "icons/icon.png",
  "icons/StoreLogo.png",
  "icons/Square30x30Logo.png",
  "icons/Square44x44Logo.png",
  "icons/Square71x71Logo.png",
  "icons/Square89x89Logo.png",
  "icons/Square107x107Logo.png",
  "icons/Square142x142Logo.png",
  "icons/Square150x150Logo.png",
  "icons/Square284x284Logo.png",
  "icons/Square310x310Logo.png",
  "icons/Wide310x150Logo.png"
]
```

### File 4: `src-tauri/icons/icon.png` (REGENERATE)

**Why:** The 512×512 master icon should be regenerated from the updated SVG to ensure consistency across all sizes.

**Change:** Regenerate from updated `icon.svg` using Tauri CLI or manual conversion.

### Files 5–18: All `src-tauri/icons/*.png` (REGENERATE)

**Why:** All platform icon sizes must be regenerated from the updated source to ensure every tile — including the wide tile — is a unique branded image.

**Change:** Regenerate using `npx tauri icon src-tauri/icons/icon.png` (or equivalent). This will produce:
- `32x32.png`
- `64x64.png`
- `128x128.png`
- `128x128@2x.png`
- `icon.icns` (macOS)
- `icon.ico` (Windows)
- `Square30x30Logo.png` through `Square310x310Logo.png`
- `StoreLogo.png`
- Android and iOS variants

Note: `Wide310x150Logo.png` is **not** auto-generated by `tauri icon` — it must be created manually (see File 2 above).

---

## Execution Steps

1. **Update `icon.svg`** with proper Bango brand vector design
2. **Convert SVG to 1024×1024 PNG** for the master icon source (using `rsvg-convert`, Inkscape, or an online tool)
3. **Create `Wide310x150Logo.png`** manually — composite logo + "Bango" text on brand background
4. **Regenerate all icon sizes** using Tauri CLI:
   ```bash
   npx tauri icon src-tauri/icons/icon.png
   ```
   Then manually place the `Wide310x150Logo.png` since `tauri icon` doesn't generate wide tiles.
5. **Update `tauri.conf.json`** `bundle.icon` array to include all icon files including the wide tile
6. **Rebuild the MSIX** and verify:
   ```bash
   npx tauri build --bundles msix
   ```
7. **Verify** all tile assets in the generated package are branded (not blank/default):
   - Check `src-tauri/gen/windows/Assets/` — every PNG should be a proper branded image
   - `Wide310x150Logo.png` should be significantly larger than 554 bytes
   - All `Square*Logo.png` files should be uniquely identifiable as Bango

---

## Verification Checklist

- [ ] `icon.svg` contains the Bango brand design (not a generic "B")
- [ ] `Wide310x150Logo.png` exists in `src-tauri/icons/` and is a proper branded image (logo + "Bango" text)
- [ ] `tauri.conf.json` `bundle.icon` array includes all platform icons including `Wide310x150Logo.png`
- [ ] All `src-tauri/icons/*.png` are regenerated and branded
- [ ] `src-tauri/gen/windows/Assets/Wide310x150Logo.png` is no longer 554 bytes / blank
- [ ] `src-tauri/gen/windows/Assets/` contains only branded images
- [ ] MSIX build succeeds without errors
- [ ] No tile icon in the package is a default/blank image

---

## References

- [Microsoft Tile and Icon Assets](https://docs.microsoft.com/en-us/windows/uwp/controls-and-patterns/tiles-and-notifications-app-assets)
- [Tauri Icon Guide](https://v2.tauri.app/develop/configuration/icons/)
- Design logo source: `design/logo.png`, `design/logo-trans.png`
- Brand colors (from spec §18.2): Primary Indigo `#4F46E5`, Sidebar Slate `#1E293B`
- Font: Inter (from spec §18.2)