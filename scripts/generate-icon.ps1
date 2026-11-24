#Requires -Version 5.1

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$projectRoot = Split-Path -Parent $PSScriptRoot
$assetsDirectory = Join-Path $projectRoot 'assets'
New-Item -ItemType Directory -Path $assetsDirectory -Force | Out-Null

function New-RoundedRectPath {
    param(
        [float]$X,
        [float]$Y,
        [float]$Width,
        [float]$Height,
        [float]$Radius
    )

    $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $diameter = $Radius * 2
    $path.AddArc($X, $Y, $diameter, $diameter, 180, 90)
    $path.AddArc($X + $Width - $diameter, $Y, $diameter, $diameter, 270, 90)
    $path.AddArc($X + $Width - $diameter, $Y + $Height - $diameter, $diameter, $diameter, 0, 90)
    $path.AddArc($X, $Y + $Height - $diameter, $diameter, $diameter, 90, 90)
    $path.CloseFigure()
    return $path
}

function New-SheetPath {
    $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $path.AddArc(11, 11, 10, 10, 180, 90)
    $path.AddLine(16, 11, 42, 11)
    $path.AddLine(42, 11, 51, 20)
    $path.AddLine(51, 20, 51, 49)
    $path.AddArc(46, 44, 10, 10, 0, 90)
    $path.AddLine(51, 54, 16, 54)
    $path.AddArc(11, 44, 10, 10, 90, 90)
    $path.AddLine(11, 49, 11, 16)
    $path.CloseFigure()
    return $path
}

function New-LinearGradientBrush {
    param(
        [float]$X1,
        [float]$Y1,
        [float]$X2,
        [float]$Y2,
        [string]$From,
        [string]$To
    )

    $start = [System.Drawing.PointF]::new($X1, $Y1)
    $end = [System.Drawing.PointF]::new($X2, $Y2)
    $fromColor = [System.Drawing.ColorTranslator]::FromHtml($From)
    $toColor = [System.Drawing.ColorTranslator]::FromHtml($To)
    return [System.Drawing.Drawing2D.LinearGradientBrush]::new($start, $end, $fromColor, $toColor)
}

function Draw-Line {
    param(
        [System.Drawing.Graphics]$Graphics,
        [float]$X1,
        [float]$Y1,
        [float]$X2,
        [float]$Y2,
        [float]$Width,
        [string]$Color
    )

    $pen = [System.Drawing.Pen]::new([System.Drawing.ColorTranslator]::FromHtml($Color), $Width)
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    try {
        $Graphics.DrawLine($pen, $X1, $Y1, $X2, $Y2)
    } finally {
        $pen.Dispose()
    }
}

function New-LogoBitmap {
    param([int]$Size)

    $bitmap = [System.Drawing.Bitmap]::new($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
        $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
        $graphics.Clear([System.Drawing.Color]::Transparent)
        $scale = $Size / 64.0
        $graphics.ScaleTransform($scale, $scale)

        $tilePath = New-RoundedRectPath 2 2 60 60 14
        $tileBrush = New-LinearGradientBrush 6 4 58 60 '#5B8CFF' '#2F55D4'
        try {
            $graphics.FillPath($tileBrush, $tilePath)
        } finally {
            $tileBrush.Dispose()
            $tilePath.Dispose()
        }

        $sheetPath = New-SheetPath
        $shadowBrush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(56, 30, 58, 138))
        $shadowMatrix = [System.Drawing.Drawing2D.Matrix]::new()
        $shadowMatrix.Translate(0, 2)
        $shadowPath = $sheetPath.Clone()
        $shadowPath.Transform($shadowMatrix)
        try {
            $graphics.FillPath($shadowBrush, $shadowPath)
        } finally {
            $shadowPath.Dispose()
            $shadowMatrix.Dispose()
            $shadowBrush.Dispose()
        }

        $sheetBrush = New-LinearGradientBrush 16 12 48 54 '#FFFFFF' '#E8EFFF'
        try {
            $graphics.FillPath($sheetBrush, $sheetPath)
        } finally {
            $sheetBrush.Dispose()
            $sheetPath.Dispose()
        }

        $foldPoints = @(
            [System.Drawing.PointF]::new(42, 11)
            [System.Drawing.PointF]::new(51, 20)
            [System.Drawing.PointF]::new(42, 20)
        )
        $foldBrush = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#C9D8FF'))
        try {
            $graphics.FillPolygon($foldBrush, $foldPoints)
        } finally {
            $foldBrush.Dispose()
        }

        Draw-Line $graphics 19 28.6 43 28.6 3.2 '#6D8BFF'
        Draw-Line $graphics 19 36.1 38 36.1 3.2 '#A9BCF5'
        Draw-Line $graphics 19 43.6 33 43.6 3.2 '#A9BCF5'

        $state = $graphics.Save()
        try {
            $graphics.TranslateTransform(43, 14)
            $graphics.RotateTransform(38)

            $pinBrush = New-LinearGradientBrush -7 -11 7 10 '#FFD166' '#F59E0B'
            try {
                $graphics.FillEllipse($pinBrush, -7.2, -11.2, 14.4, 10.4)
            } finally {
                $pinBrush.Dispose()
            }

            $highlightBrush = [System.Drawing.SolidBrush]::new([System.Drawing.Color]::FromArgb(217, 255, 244, 199))
            try {
                $graphics.FillEllipse($highlightBrush, -4.8, -8.7, 5.2, 3.0)
            } finally {
                $highlightBrush.Dispose()
            }

            $bodyPoints = @(
                [System.Drawing.PointF]::new(-4.2, -2.2)
                [System.Drawing.PointF]::new(4.2, -2.2)
                [System.Drawing.PointF]::new(2.0, 7.0)
                [System.Drawing.PointF]::new(-2.0, 7.0)
            )
            $bodyBrush = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#F59E0B'))
            try {
                $graphics.FillPolygon($bodyBrush, $bodyPoints)
            } finally {
                $bodyBrush.Dispose()
            }

            $needleBrush = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#D97706'))
            try {
                $graphics.FillRectangle($needleBrush, -1.1, 6.4, 2.2, 9.6)
            } finally {
                $needleBrush.Dispose()
            }

            $tipPoints = @(
                [System.Drawing.PointF]::new(0.0, 15.2)
                [System.Drawing.PointF]::new(1.8, 19.4)
                [System.Drawing.PointF]::new(-1.8, 19.4)
            )
            $tipBrush = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#B45309'))
            try {
                $graphics.FillPolygon($tipBrush, $tipPoints)
            } finally {
                $tipBrush.Dispose()
            }
        } finally {
            $graphics.Restore($state)
        }
    } finally {
        $graphics.Dispose()
    }

    return $bitmap
}

function Get-IconBmpData {
    param([System.Drawing.Bitmap]$Bitmap)

    $width = $Bitmap.Width
    $height = $Bitmap.Height
    $rect = [System.Drawing.Rectangle]::new(0, 0, $width, $height)
    $locked = $Bitmap.LockBits(
        $rect,
        [System.Drawing.Imaging.ImageLockMode]::ReadOnly,
        [System.Drawing.Imaging.PixelFormat]::Format32bppArgb
    )
    try {
        $stride = $locked.Stride
        $pixels = [byte[]]::new($stride * $height)
        [System.Runtime.InteropServices.Marshal]::Copy($locked.Scan0, $pixels, 0, $pixels.Length)
    } finally {
        $Bitmap.UnlockBits($locked)
    }

    $maskStride = [int]([math]::Floor(($width + 31) / 32) * 4)
    $mask = [byte[]]::new($maskStride * $height)
    for ($y = 0; $y -lt $height; $y++) {
        $sourceRow = ($height - 1 - $y) * $stride
        $maskRow = $y * $maskStride
        for ($x = 0; $x -lt $width; $x++) {
            if ($pixels[$sourceRow + $x * 4 + 3] -eq 0) {
                $mask[$maskRow + [math]::Floor($x / 8)] = $mask[$maskRow + [math]::Floor($x / 8)] -bor (0x80 -shr ($x % 8))
            }
        }
    }

    $stream = [System.IO.MemoryStream]::new()
    $writer = [System.IO.BinaryWriter]::new($stream)
    try {
        $maskSize = $mask.Length
        $writer.Write([uint32]40)
        $writer.Write([int32]$width)
        $writer.Write([int32]($height * 2))
        $writer.Write([uint16]1)
        $writer.Write([uint16]32)
        $writer.Write([uint32]0)
        $writer.Write([uint32]($width * $height * 4 + $maskSize))
        $writer.Write([int32]0)
        $writer.Write([int32]0)
        $writer.Write([uint32]0)
        $writer.Write([uint32]0)
        for ($y = $height - 1; $y -ge 0; $y--) {
            $sourceRow = $y * $stride
            for ($x = 0; $x -lt $width; $x++) {
                $offset = $sourceRow + $x * 4
                $writer.Write($pixels[$offset + 2])
                $writer.Write($pixels[$offset + 1])
                $writer.Write($pixels[$offset + 0])
                $writer.Write($pixels[$offset + 3])
            }
        }
        $writer.Write($mask)
        $writer.Flush()
        return [byte[]]$stream.ToArray()
    } finally {
        $writer.Dispose()
        $stream.Dispose()
    }
}

$logo = New-LogoBitmap -Size 512
try {
    $logo.Save((Join-Path $assetsDirectory 'logo.png'), [System.Drawing.Imaging.ImageFormat]::Png)
} finally {
    $logo.Dispose()
}

$sizes = @(16, 24, 32, 48, 64, 128, 256)
$images = @()
foreach ($size in $sizes) {
    $bitmap = New-LogoBitmap -Size $size
    try {
        $images += [pscustomobject]@{
            Size = $size
            Data = [byte[]](Get-IconBmpData -Bitmap $bitmap)
        }
    } finally {
        $bitmap.Dispose()
    }
}

$iconStream = [System.IO.MemoryStream]::new()
$iconWriter = [System.IO.BinaryWriter]::new($iconStream)
try {
    $iconWriter.Write([uint16]0)
    $iconWriter.Write([uint16]1)
    $iconWriter.Write([uint16]$images.Count)
    $offset = 6 + 16 * $images.Count
    foreach ($image in $images) {
        $dimension = if ($image.Size -ge 256) { 0 } else { $image.Size }
        $iconWriter.Write([byte]$dimension)
        $iconWriter.Write([byte]$dimension)
        $iconWriter.Write([byte]0)
        $iconWriter.Write([byte]0)
        $iconWriter.Write([uint16]1)
        $iconWriter.Write([uint16]32)
        $iconWriter.Write([uint32]$image.Data.Length)
        $iconWriter.Write([uint32]$offset)
        $offset += $image.Data.Length
    }
    foreach ($image in $images) {
        $iconWriter.Write([byte[]]$image.Data)
    }
    $iconWriter.Flush()
    [System.IO.File]::WriteAllBytes((Join-Path $assetsDirectory 'icon.ico'), $iconStream.ToArray())
} finally {
    $iconWriter.Dispose()
    $iconStream.Dispose()
}

Write-Host "已生成 assets/logo.png 与 assets/icon.ico"
