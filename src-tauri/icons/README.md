# Application Icons

This directory contains the application icons for PETool.

## Icon Formats Needed

- `32x32.png` - Small icon
- `128x128.png` - Medium icon
- `128x128@2x.png` - High DPI medium icon
- `icon.icns` - macOS icon
- `icon.ico` - Windows icon

## Generating Icons

You can use the following tools to generate icons from the SVG:

### Option 1: Using ImageMagick

```bash
# Convert SVG to PNG files
convert -background none -resize 32x32 icon.svg 32x32.png
convert -background none -resize 128x128 icon.svg 128x128.png
convert -background none -resize 256x256 icon.svg 128x128@2x.png

# Convert to ICO (Windows)
convert -background none -resize 256x256 icon.svg icon.ico

# Convert to ICNS (macOS) - requires iconutil
# First create iconset
mkdir icon.iconset
sips -z 16 16     icon.svg --out icon.iconset/icon_16x16.png
sips -z 32 32     icon.svg --out icon.iconset/icon_16x16@2x.png
sips -z 32 32     icon.svg --out icon.iconset/icon_32x32.png
sips -z 64 64     icon.svg --out icon.iconset/icon_32x32@2x.png
sips -z 128 128   icon.svg --out icon.iconset/icon_128x128.png
sips -z 256 256   icon.svg --out icon.iconset/icon_128x128@2x.png
sips -z 256 256   icon.svg --out icon.iconset/icon_256x256.png
sips -z 512 512   icon.svg --out icon.iconset/icon_256x256@2x.png
sips -z 512 512   icon.svg --out icon.iconset/icon_512x512.png
sips -z 1024 1024 icon.svg --out icon.iconset/icon_512x512@2x.png
# Create ICNS
iconutil -c icns icon.iconset
```

### Option 2: Using Online Tools

Visit https://favicon.io/ or https://realfavicongenerator.net/ to generate icons from SVG.

### Option 3: Using Tauri Icon (Default)

For now, the Tauri default icon is being used. You can replace it with custom icons using the methods above.
