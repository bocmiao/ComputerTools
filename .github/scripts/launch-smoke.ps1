# CI only (not bundled into the app): launch the built app on the Windows runner and check
# that it really comes up.
#   - the main window appears and no error dialog is shown
#   - WebView2 started for this app
#   - the main window fits in the screen's work area (the runner's screen is only 1024x768,
#     smaller than the default window size, like many budget laptops)
#   - a second instance is refused instead of opening its own window
# Also saves a screenshot for humans to look at.
# Run with pwsh: the window title is Chinese and Windows PowerShell 5.1 would read this file
# with the ANSI code page.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Exe,
    [Parameter(Mandatory)][string]$Screenshot
)

$ErrorActionPreference = 'Stop'
$title = '电脑小药箱'

Add-Type @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class SmokeWin {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("dwmapi.dll")] static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
    // The visible frame (DWMWA_EXTENDED_FRAME_BOUNDS), without the invisible resize borders.
    public static int[] Bounds(IntPtr h) {
        RECT r;
        if (DwmGetWindowAttribute(h, 9, out r, Marshal.SizeOf(typeof(RECT))) != 0) { GetWindowRect(h, out r); }
        return new[] { r.Left, r.Top, r.Right, r.Bottom };
    }
    public static List<string[]> Of(uint pid) {
        var list = new List<string[]>();
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            if (p == pid) {
                var c = new StringBuilder(256); GetClassName(h, c, 256);
                var t = new StringBuilder(256); GetWindowText(h, t, 256);
                list.Add(new[] { c.ToString(), t.ToString(), IsWindowVisible(h) ? "1" : "0", h.ToInt64().ToString() });
            }
            return true;
        }, IntPtr.Zero);
        return list;
    }
}
'@

function Get-AppWindow {
    param([System.Diagnostics.Process]$Process)
    foreach ($w in [SmokeWin]::Of([uint32]$Process.Id)) {
        [pscustomobject]@{ Class = $w[0]; Title = $w[1]; Visible = ($w[2] -eq '1'); Handle = [IntPtr][long]$w[3] }
    }
}

function Save-Screenshot {
    param([string]$Path)
    try {
        Add-Type -AssemblyName System.Windows.Forms, System.Drawing
        $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
        $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
        $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
        Write-Output "screenshot saved: $Path"
    }
    catch {
        Write-Output "::warning::could not take a screenshot: $($_.Exception.Message)"
    }
}

$app = Start-Process -FilePath $Exe -PassThru
$second = $null
try {
    # 1. The main window appears, and no error dialog (#32770 is the dialog window class).
    $deadline = (Get-Date).AddSeconds(90)
    $main = $null
    while ($null -eq $main) {
        if ((Get-Date) -gt $deadline) { throw 'the main window did not appear within 90 seconds' }
        Start-Sleep -Seconds 2
        if ($app.HasExited) { throw "the app exited early with code $($app.ExitCode)" }
        $windows = @(Get-AppWindow $app)
        $dialog = $windows | Where-Object { $_.Visible -and $_.Class -eq '#32770' } | Select-Object -First 1
        if ($dialog) { throw "the app showed an error dialog: $($dialog.Title)" }
        $main = $windows | Where-Object { $_.Visible -and $_.Class -ne '#32770' -and $_.Title -eq $title } | Select-Object -First 1
    }
    Write-Output "main window: class '$($main.Class)', title '$($main.Title)'"

    # 2. WebView2 started for this app (its browser process carries the host exe name).
    $exeName = [System.IO.Path]::GetFileName($Exe)
    $webview = @(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" |
        Where-Object { ($_.ParentProcessId -eq $app.Id) -or ($_.CommandLine -like "*--webview-exe-name=$exeName*") })
    if ($webview.Count -eq 0) { throw 'no WebView2 process was started for the app' }
    Write-Output "WebView2 processes: $($webview.Count)"

    # 3. The main window fits in the work area (not under the taskbar, not off the screen).
    Add-Type -AssemblyName System.Windows.Forms
    $area = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
    $b = [SmokeWin]::Bounds($main.Handle)
    Write-Output ('main window {0},{1} - {2},{3}; work area {4},{5} - {6},{7}' -f $b[0], $b[1], $b[2], $b[3], $area.Left, $area.Top, $area.Right, $area.Bottom)
    $slack = 2
    if (($b[0] -lt $area.Left - $slack) -or ($b[1] -lt $area.Top - $slack) -or ($b[2] -gt $area.Right + $slack) -or ($b[3] -gt $area.Bottom + $slack)) {
        throw 'the main window does not fit in the work area: part of it is off the screen or under the taskbar'
    }

    # Give the page a moment to load and call the backend, then take a screenshot.
    Start-Sleep -Seconds 10
    if ($app.HasExited) { throw "the app exited after start-up with code $($app.ExitCode)" }
    Save-Screenshot -Path $Screenshot

    # 4. A second instance is refused: it shows a dialog (or exits) and never opens its own window.
    $second = Start-Process -FilePath $Exe -PassThru
    Start-Sleep -Seconds 8
    if (-not $second.HasExited) {
        $own = @(Get-AppWindow $second | Where-Object { $_.Visible -and $_.Class -ne '#32770' -and $_.Title -eq $title })
        if ($own.Count -gt 0) { throw 'a second instance opened its own window' }
        $refusal = @(Get-AppWindow $second | Where-Object { $_.Visible -and $_.Class -eq '#32770' })
        if ($refusal.Count -eq 0) { throw 'a second instance neither exited nor showed the "already running" message' }
    }
    Write-Output 'second instance was refused'
}
finally {
    foreach ($p in @($second, $app)) {
        if (($null -ne $p) -and (-not $p.HasExited)) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    }
}
