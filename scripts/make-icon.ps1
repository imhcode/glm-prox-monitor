# Generate ikon sumber 1024x1024 untuk glm-overflow (gauge + persen)
Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAlias
$g.Clear([System.Drawing.Color]::Transparent)

# Rounded square background (gradient gelap)
$rect = New-Object System.Drawing.Rectangle(32, 32, 960, 960)
$path = New-Object System.Drawing.Drawing2D.GraphicsPath
$r = 200
$path.AddArc($rect.X, $rect.Y, $r, $r, 180, 90)
$path.AddArc($rect.Right - $r, $rect.Y, $r, $r, 270, 90)
$path.AddArc($rect.Right - $r, $rect.Bottom - $r, $r, $r, 0, 90)
$path.AddArc($rect.X, $rect.Bottom - $r, $r, $r, 90, 90)
$path.CloseFigure()
$bg = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    $rect,
    [System.Drawing.Color]::FromArgb(255, 17, 21, 32),
    [System.Drawing.Color]::FromArgb(255, 40, 56, 84), 55)
$g.FillPath($bg, $path)

# Arc gauge: sisa hijau (kiri atas) + track abu
$track = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 62, 70, 92), 78)
$track.StartCap = 'Round'; $track.EndCap = 'Round'
$g.DrawArc($track, 202, 202, 620, 620, 125, 290)
$green = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 74, 222, 128), 78)
$green.StartCap = 'Round'; $green.EndCap = 'Round'
$g.DrawArc($green, 202, 202, 620, 620, 125, 175)

# Teks %
$font = New-Object System.Drawing.Font('Segoe UI', 230, [System.Drawing.FontStyle]::Bold)
$white = [System.Drawing.Brushes]::White
$sz = $g.MeasureString('%', $font)
$g.DrawString('%', $font, $white, (1024 - $sz.Width) / 2, (1024 - $sz.Height) / 2 - 20)

$out = Join-Path $PSScriptRoot 'app-icon.png'
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Host "ICON_OK $out"
