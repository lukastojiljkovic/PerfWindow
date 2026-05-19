# Generates dashboard/assets/PerfWindow.ico — a multi-resolution icon carrying
# the PerfWindow mark: the U+259A block-wordmark motif (two diagonal quadrant
# blocks) in the accent colour on the dark slate background.
#
# ImageMagick is not assumed. Each size (16/32/48/256) is drawn with
# System.Drawing into a 32-bpp Bitmap, then encoded as a classic DIB icon image
# (BITMAPINFOHEADER with doubled height + bottom-up BGRA XOR bitmap + 1-bpp AND
# mask) and packed into a valid ICO container (ICONDIR + one ICONDIRENTRY per
# image + the image data). DIB frames are what the MSVC resource compiler that
# `winresource` invokes consumes reliably — it is the format winresource's own
# test fixture uses for every size.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$assets = Join-Path (Split-Path $PSScriptRoot -Parent) 'dashboard\assets'
$icoPath = Join-Path $assets 'PerfWindow.ico'

# Slate theme palette (dashboard/src/theme/mod.rs, ThemeId::Slate).
$slate  = [System.Drawing.Color]::FromArgb(0xFF, 0x0A, 0x0E, 0x15) # bg
$panel  = [System.Drawing.Color]::FromArgb(0xFF, 0x12, 0x18, 0x23) # panel
$border = [System.Drawing.Color]::FromArgb(0xFF, 0x24, 0x31, 0x44) # border
$accent = [System.Drawing.Color]::FromArgb(0xFF, 0x34, 0xE0, 0xD0) # accent
$soft   = [System.Drawing.Color]::FromArgb(0xFF, 0x7A, 0xF0, 0xE4) # accent_soft

# Draws the PerfWindow mark for one square size and returns a 32-bpp Bitmap.
function New-MarkBitmap([int]$s) {
    $bmp = New-Object System.Drawing.Bitmap($s, $s, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.Clear([System.Drawing.Color]::Transparent)

    # Rounded-rect slate field with a thin border, leaving a small margin.
    $margin = [math]::Max(1, [int][math]::Round($s * 0.06))
    $rad    = [math]::Max(2, [int][math]::Round($s * 0.16))
    $fw     = $s - 2 * $margin
    $d      = 2 * $rad
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc($margin, $margin, $d, $d, 180, 90)
    $path.AddArc($margin + $fw - $d, $margin, $d, $d, 270, 90)
    $path.AddArc($margin + $fw - $d, $margin + $fw - $d, $d, $d, 0, 90)
    $path.AddArc($margin, $margin + $fw - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    $bgBrush = New-Object System.Drawing.SolidBrush($slate)
    $g.FillPath($bgBrush, $path)
    if ($s -ge 32) {
        $borderPen = New-Object System.Drawing.Pen($border, [math]::Max(1.0, $s * 0.035))
        $g.DrawPath($borderPen, $path)
        $borderPen.Dispose()
    }

    # The U+259A mark: a 2x2 grid of quadrant cells inside an inset square; the
    # two cells on the anti-diagonal (top-left, bottom-right) are lit — the
    # block-wordmark motif. All four cells get a faint panel fill so the glyph
    # frame reads; the lit pair is painted in the accent colours.
    $inset = $s * 0.22
    $cell  = ($s - 2 * $inset) / 2.0
    $gap   = [math]::Max(0.0, $s * 0.022)
    $cellBrush = New-Object System.Drawing.SolidBrush($panel)
    for ($r = 0; $r -lt 2; $r++) {
        for ($c = 0; $c -lt 2; $c++) {
            $x = $inset + $c * $cell + $gap / 2
            $y = $inset + $r * $cell + $gap / 2
            $w = $cell - $gap
            $g.FillRectangle($cellBrush, [float]$x, [float]$y, [float]$w, [float]$w)
        }
    }
    $accBrush  = New-Object System.Drawing.SolidBrush($accent)
    $softBrush = New-Object System.Drawing.SolidBrush($soft)
    foreach ($q in @(@(0, 0), @(1, 1))) {
        $c = $q[0]; $r = $q[1]
        $x = $inset + $c * $cell + $gap / 2
        $y = $inset + $r * $cell + $gap / 2
        $w = $cell - $gap
        # Top-left lit cell uses accent_soft, bottom-right uses accent — a
        # subtle two-tone so the mark has depth at large sizes.
        $b = if ($c -eq 0) { $softBrush } else { $accBrush }
        $g.FillRectangle($b, [float]$x, [float]$y, [float]$w, [float]$w)
    }

    $g.Dispose()
    $bgBrush.Dispose(); $cellBrush.Dispose(); $accBrush.Dispose(); $softBrush.Dispose()
    $path.Dispose()
    return $bmp
}

# Encodes a 32-bpp Bitmap as a classic DIB icon image: BITMAPINFOHEADER (with
# doubled biHeight) + bottom-up 32-bpp BGRA XOR bitmap + bottom-up 1-bpp AND
# mask (rows padded to 4 bytes). Returns the raw image bytes.
function ConvertTo-DibIcon([System.Drawing.Bitmap]$bmp) {
    $w = $bmp.Width
    $h = $bmp.Height
    $ms = New-Object System.IO.MemoryStream
    $bw = New-Object System.IO.BinaryWriter($ms)

    # BITMAPINFOHEADER (40 bytes). biHeight is doubled (XOR + AND stacked).
    $bw.Write([uint32]40)        # biSize
    $bw.Write([int32]$w)         # biWidth
    $bw.Write([int32]($h * 2))   # biHeight (doubled)
    $bw.Write([uint16]1)         # biPlanes
    $bw.Write([uint16]32)        # biBitCount
    $bw.Write([uint32]0)         # biCompression = BI_RGB
    $bw.Write([uint32]0)         # biSizeImage (0 ok for BI_RGB)
    $bw.Write([int32]0)          # biXPelsPerMeter
    $bw.Write([int32]0)          # biYPelsPerMeter
    $bw.Write([uint32]0)         # biClrUsed
    $bw.Write([uint32]0)         # biClrImportant

    # XOR bitmap: 32-bpp BGRA, bottom-up.
    for ($y = $h - 1; $y -ge 0; $y--) {
        for ($x = 0; $x -lt $w; $x++) {
            $p = $bmp.GetPixel($x, $y)
            $bw.Write([byte]$p.B)
            $bw.Write([byte]$p.G)
            $bw.Write([byte]$p.R)
            $bw.Write([byte]$p.A)
        }
    }

    # AND mask: 1 bpp, bottom-up, each row padded to a 4-byte boundary. A bit
    # is 1 where the pixel is fully transparent (mask out), 0 where opaque.
    $rowBytes = [int][math]::Floor(($w + 31) / 32) * 4
    for ($y = $h - 1; $y -ge 0; $y--) {
        $row = New-Object byte[] $rowBytes
        for ($x = 0; $x -lt $w; $x++) {
            if ($bmp.GetPixel($x, $y).A -eq 0) {
                $row[[int][math]::Floor($x / 8)] = $row[[int][math]::Floor($x / 8)] -bor (0x80 -shr ($x % 8))
            }
        }
        $bw.Write($row)
    }

    $bw.Flush()
    $bytes = $ms.ToArray()
    $bw.Dispose(); $ms.Dispose()
    return , $bytes
}

$sizes = 16, 32, 48, 256
$images = @()
foreach ($s in $sizes) {
    $bmp = New-MarkBitmap $s
    $data = ConvertTo-DibIcon $bmp
    $bmp.Dispose()
    $images += , @{ Size = $s; Bytes = $data }
}

# Assemble the ICO container.
#   ICONDIR      : 6 bytes  (reserved=0, type=1, count)
#   ICONDIRENTRY : 16 bytes each
#   image data   : DIB icon images, appended after all entries
$out = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter($out)
$bw.Write([uint16]0)               # reserved
$bw.Write([uint16]1)               # type: 1 = icon
$bw.Write([uint16]$images.Count)   # image count

$offset = 6 + 16 * $images.Count
foreach ($img in $images) {
    $s = $img.Size
    # 256 px is stored as 0 in the byte-wide width/height fields.
    $dim = if ($s -ge 256) { 0 } else { $s }
    $bw.Write([byte]$dim)               # width
    $bw.Write([byte]$dim)               # height
    $bw.Write([byte]0)                  # palette count (0 = no palette)
    $bw.Write([byte]0)                  # reserved
    $bw.Write([uint16]1)                # colour planes
    $bw.Write([uint16]32)               # bits per pixel
    $bw.Write([uint32]$img.Bytes.Length) # image data size
    $bw.Write([uint32]$offset)          # image data offset
    $offset += $img.Bytes.Length
}
foreach ($img in $images) {
    $bw.Write($img.Bytes)
}
$bw.Flush()
[System.IO.File]::WriteAllBytes($icoPath, $out.ToArray())
$bw.Dispose()
$out.Dispose()

Write-Host "Wrote $icoPath ($((Get-Item $icoPath).Length) bytes, $($images.Count) images)"
