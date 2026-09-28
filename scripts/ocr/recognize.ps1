# Text in a picture (the toolbox's "picture to text"), read by the OCR that
# comes with Windows 10 and later (Windows.Media.Ocr). Microsoft's PowerToys
# Text Extractor documentation loads it the same way, in Windows PowerShell
# 5.1 (PowerShell 7 cannot). Nothing leaves the computer.
# -Path: the picture, a PNG that medkit wrote to its own data folder and
#   deletes right after this script.
# Language: Simplified Chinese when its recognizer is installed, then any
#   Chinese, then the user's own languages, then whatever is installed.
#   Recognizers are the Windows capabilities Language.OCR~~~<tag>~0.0.1.0;
#   AvailableRecognizerLanguages lists the installed ones.
# The recognizer takes pictures up to MaxImageDimension pixels on a side
# (10000 on Windows Server 2025). Wider pictures are scaled down to that
# width. Taller ones (long screenshots) are read in pieces of at most 4000
# pixels that overlap by a strip: in 10000-pixel pieces the recognizer lost
# the first letters of words and whole words (a test on Windows found it). A
# line belongs to the piece where its middle lies outside the overlap, so
# every line is read once and none is cut in half. At most 30 pieces are read
# (truncated).
# Output: result = ok / no-language (no recognizer installed) / unsupported
#   (no Windows OCR here) / bad-image (Windows cannot read the picture);
#   language (the tag used), languages (every installed tag), lines (top to
#   bottom, one object per line: words, as Windows split them; the engine
#   joins them, with no space next to Chinese), pieces, truncated, detail
#   (Windows' own message when the picture or the recognizer failed).

[CmdletBinding()]
param(
    [string]$Path = '',
    # Pieces are at most this tall (and never taller than MaxImageDimension).
    [int]$PieceHeight = 4000,
    # Adds trace: every piece and every line it read, kept or not (tests).
    [switch]$Trace
)

$ErrorActionPreference = 'Stop'

$maxPieces = 30

function New-Result {
    param([string]$Result, [string[]]$Languages = @(), [string]$Detail = '')
    [pscustomobject]@{
        result    = $Result
        language  = ''
        languages = $Languages
        lines     = @()
        pieces    = 0
        truncated = $false
        detail    = $Detail
    }
}

try {
    # AsTask (below) turns the WinRT operations into tasks to wait on.
    Add-Type -AssemblyName System.Runtime.WindowsRuntime
    $null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
    $null = [Windows.Media.Ocr.OcrResult, Windows.Foundation, ContentType = WindowsRuntime]
    $null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
    $null = [Windows.Storage.FileAccessMode, Windows.Storage, ContentType = WindowsRuntime]
    $null = [Windows.Storage.Streams.IRandomAccessStream, Windows.Storage.Streams, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Graphics, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapTransform, Windows.Graphics, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapBounds, Windows.Graphics, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapInterpolationMode, Windows.Graphics, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapPixelFormat, Windows.Graphics, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapAlphaMode, Windows.Graphics, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.ExifOrientationMode, Windows.Graphics, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.ColorManagementMode, Windows.Graphics, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.SoftwareBitmap, Windows.Graphics, ContentType = WindowsRuntime]
}
catch {
    New-Result 'unsupported' @() $_.Exception.Message
    return
}

$asTask = $null
foreach ($method in [System.WindowsRuntimeSystemExtensions].GetMethods()) {
    if (($method.Name -ne 'AsTask') -or (-not $method.IsGenericMethodDefinition)) {
        continue
    }
    $parameters = $method.GetParameters()
    if (($parameters.Count -eq 1) -and ($parameters[0].ParameterType.Name -eq 'IAsyncOperation`1')) {
        $asTask = $method
        break
    }
}
if ($null -eq $asTask) {
    New-Result 'unsupported'
    return
}

# Waits for a WinRT operation and gives its result.
function Wait-Operation {
    param($Operation, [Type]$ResultType)
    $task = $asTask.MakeGenericMethod($ResultType).Invoke($null, @($Operation))
    if (-not $task.Wait(60000)) {
        throw 'Windows did not finish in time'
    }
    return $task.Result
}

$available = @([Windows.Media.Ocr.OcrEngine]::AvailableRecognizerLanguages)
$tags = [string[]]@($available | ForEach-Object { [string]$_.LanguageTag })
if ($available.Count -eq 0) {
    New-Result 'no-language'
    return
}

$chosen = $null
foreach ($pattern in @('^zh-(Hans|CN|SG)', '^zh')) {
    foreach ($candidate in $available) {
        if ([string]$candidate.LanguageTag -match $pattern) {
            $chosen = $candidate
            break
        }
    }
    if ($null -ne $chosen) {
        break
    }
}
$engine = $null
if ($null -ne $chosen) {
    $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage($chosen)
}
if ($null -eq $engine) {
    $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
}
if ($null -eq $engine) {
    $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage($available[0])
}
if ($null -eq $engine) {
    New-Result 'no-language' $tags
    return
}

$stream = $null
try {
    $full = [System.IO.Path]::GetFullPath($Path)
    $file = Wait-Operation ([Windows.Storage.StorageFile]::GetFileFromPathAsync($full)) ([Windows.Storage.StorageFile])
    $stream = Wait-Operation ($file.OpenAsync([Windows.Storage.FileAccessMode]::Read)) ([Windows.Storage.Streams.IRandomAccessStream])
    $decoder = Wait-Operation ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
}
catch {
    if ($null -ne $stream) {
        $stream.Dispose()
    }
    New-Result 'bad-image' $tags $_.Exception.Message
    return
}

try {
    $max = [int][Windows.Media.Ocr.OcrEngine]::MaxImageDimension
    $width = [int]$decoder.PixelWidth
    $height = [int]$decoder.PixelHeight
    if (($width -lt 1) -or ($height -lt 1) -or ($max -lt 100)) {
        New-Result 'bad-image' $tags
        return
    }
    $scale = 1.0
    if ($width -gt $max) {
        $scale = $max / $width
    }
    $scaledWidth = [int][Math]::Max(1, [Math]::Floor($width * $scale))
    $scaledHeight = [int][Math]::Max(1, [Math]::Floor($height * $scale))
    $pieceMax = [int][Math]::Max(1000, [Math]::Min($max, $PieceHeight))
    $overlap = [int][Math]::Min(400, [Math]::Floor($pieceMax / 4))

    $lines = New-Object System.Collections.Generic.List[object]
    $traced = New-Object System.Collections.Generic.List[object]
    $top = 0
    $pieces = 0
    $truncated = $false
    while ($true) {
        $pieceHeight = [int][Math]::Min($pieceMax, $scaledHeight - $top)
        $last = (($top + $pieceHeight) -ge $scaledHeight)
        $transform = New-Object Windows.Graphics.Imaging.BitmapTransform
        if ($scale -lt 1.0) {
            $transform.ScaledWidth = [uint32]$scaledWidth
            $transform.ScaledHeight = [uint32]$scaledHeight
            $transform.InterpolationMode = [Windows.Graphics.Imaging.BitmapInterpolationMode]::Fant
        }
        # The crop is taken from the scaled picture (Microsoft, BitmapTransform:
        # scale, flip, rotation, then crop).
        $bounds = New-Object Windows.Graphics.Imaging.BitmapBounds
        $bounds.X = [uint32]0
        $bounds.Y = [uint32]$top
        $bounds.Width = [uint32]$scaledWidth
        $bounds.Height = [uint32]$pieceHeight
        $transform.Bounds = $bounds
        $operation = $decoder.GetSoftwareBitmapAsync(
            [Windows.Graphics.Imaging.BitmapPixelFormat]::Bgra8,
            [Windows.Graphics.Imaging.BitmapAlphaMode]::Premultiplied,
            $transform,
            [Windows.Graphics.Imaging.ExifOrientationMode]::IgnoreExifOrientation,
            [Windows.Graphics.Imaging.ColorManagementMode]::DoNotColorManage)
        $bitmap = Wait-Operation $operation ([Windows.Graphics.Imaging.SoftwareBitmap])
        $pieceTrace = $null
        if ($Trace) {
            $pieceTrace = [pscustomobject]@{
                top    = $top
                width  = [int]$bitmap.PixelWidth
                height = [int]$bitmap.PixelHeight
                lines  = New-Object System.Collections.Generic.List[object]
            }
            $traced.Add($pieceTrace)
        }
        try {
            $recognized = Wait-Operation ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
        }
        finally {
            $bitmap.Dispose()
        }
        $pieces++

        # Lines whose middle is in the strip shared with the piece before or
        # after belong to that piece.
        $keepFrom = [double]::MinValue
        if ($top -gt 0) {
            $keepFrom = $overlap / 2
        }
        $keepTo = [double]::MaxValue
        if (-not $last) {
            $keepTo = $pieceHeight - ($overlap / 2)
        }
        foreach ($line in $recognized.Lines) {
            $words = New-Object System.Collections.Generic.List[string]
            $lineTop = [double]::MaxValue
            $lineBottom = [double]::MinValue
            $lineLeft = [double]::MaxValue
            foreach ($word in $line.Words) {
                $text = [string]$word.Text
                if ($text.Length -eq 0) {
                    continue
                }
                $words.Add($text)
                $rect = $word.BoundingRect
                $lineTop = [Math]::Min($lineTop, [double]$rect.Y)
                $lineBottom = [Math]::Max($lineBottom, [double]$rect.Y + [double]$rect.Height)
                $lineLeft = [Math]::Min($lineLeft, [double]$rect.X)
            }
            if ($words.Count -eq 0) {
                continue
            }
            $middle = ($lineTop + $lineBottom) / 2
            $keep = (($middle -ge $keepFrom) -and ($middle -lt $keepTo))
            if ($null -ne $pieceTrace) {
                $pieceTrace.lines.Add([pscustomobject]@{
                        text   = ($words.ToArray() -join ' ')
                        left   = [int]$lineLeft
                        top    = [int]$lineTop
                        bottom = [int]$lineBottom
                        kept   = $keep
                    })
            }
            if (-not $keep) {
                continue
            }
            $lines.Add([pscustomobject]@{ words = [string[]]$words.ToArray() })
        }

        if ($last) {
            break
        }
        if ($pieces -ge $maxPieces) {
            $truncated = $true
            break
        }
        $top = $top + $pieceHeight - $overlap
    }
}
catch {
    New-Result 'bad-image' $tags $_.Exception.Message
    return
}
finally {
    $stream.Dispose()
}

[pscustomobject]@{
    result    = 'ok'
    language  = [string]$engine.RecognizerLanguage.LanguageTag
    languages = $tags
    lines     = $lines.ToArray()
    pieces    = $pieces
    truncated = $truncated
    detail    = ''
    trace     = $traced.ToArray()
}
