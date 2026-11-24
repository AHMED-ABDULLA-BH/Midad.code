param([Parameter(Mandatory=$true)][string]$Installer)
$ErrorActionPreference = 'Stop'
$root = Join-Path ([IO.Path]::GetTempPath()) ("midad-test-" + [guid]::NewGuid().ToString('N'))
$install = Join-Path $root 'app'
New-Item -ItemType Directory -Path $root | Out-Null
$process = $null
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class NativeStudio {
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr w, int id);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr SendMessageW(IntPtr w, uint m, IntPtr a, string text);
    [DllImport("user32.dll", EntryPoint="SendMessageW")] public static extern IntPtr Command(IntPtr w, uint m, IntPtr a, IntPtr b);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageW")] public static extern IntPtr Read(IntPtr w, uint m, IntPtr a, StringBuilder s);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr w, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr w, out Rect r);
    public struct Rect { public int Left, Top, Right, Bottom; }
}
'@
function Launch([string]$file) {
    $p = Start-Process (Join-Path $install 'midad.exe') -ArgumentList ('"' + $file + '"') -PassThru
    for ($i=0; $i -lt 100; $i++) {
        $p.Refresh()
        if ($p.HasExited) { throw 'The editor exited during startup' }
        if ($p.MainWindowHandle -ne 0 -and $p.MainWindowTitle -like '*Smoke*') { return $p }
        Start-Sleep -Milliseconds 150
    }
    throw 'Document window did not open'
}
function Read-Line([IntPtr]$window) {
    $s = [Text.StringBuilder]::new(1024)
    [void][NativeStudio]::Read([NativeStudio]::GetDlgItem($window,101), 13, [IntPtr]1024, $s)
    return $s.ToString()
}
try {
    $setup=Start-Process (Resolve-Path $Installer) -ArgumentList '/S',("/D="+$install) -Wait -PassThru
    if ($setup.ExitCode) { throw "Installer exit $($setup.ExitCode)" }
    $file = Join-Path $root 'Smoke.midad'
    Copy-Item (Get-ChildItem "$PSScriptRoot/../examples/*.midad" | Select-Object -First 1) $file
    $sample=(Get-Content $file -Raw -Encoding UTF8 | ConvertFrom-Json).pages[0].lines[0].content.text
    $process=Launch $file
    $w=$process.MainWindowHandle
    if ((Read-Line $w) -ne $sample) { throw 'Editor did not load the example text' }
    $bounds=New-Object 'NativeStudio+Rect'
    [void][NativeStudio]::GetWindowRect($w,[ref]$bounds)
    $bmp=[Drawing.Bitmap]::new($bounds.Right-$bounds.Left,$bounds.Bottom-$bounds.Top)
    $g=[Drawing.Graphics]::FromImage($bmp)
    $dc=$g.GetHdc()
    try { $captured=[NativeStudio]::PrintWindow($w,$dc,2) } finally { $g.ReleaseHdc($dc);$g.Dispose() }
    if($captured){ $bmp.Save((Join-Path (Resolve-Path "$PSScriptRoot/../target").Path 'studio-windows.png'),[Drawing.Imaging.ImageFormat]::Png) }
    $bmp.Dispose()
    $edited='حبر مداد'
    [void][NativeStudio]::SendMessageW([NativeStudio]::GetDlgItem($w,101),12,[IntPtr]::Zero,$edited)
    [void][NativeStudio]::Command($w,273,[IntPtr]203,[IntPtr]::Zero)
    if ((Get-Content $file -Raw -Encoding UTF8 | ConvertFrom-Json).pages[0].lines[0].content.text -ne $edited) { throw 'Edited project was not saved' }
    [void]$process.CloseMainWindow()
    if (!$process.WaitForExit(10000)) { throw 'Saved window did not close' }
    $process=Launch $file
    if ((Read-Line $process.MainWindowHandle) -ne $edited) { throw 'Edited text was not restored' }
    [void]$process.CloseMainWindow()
    if (!$process.WaitForExit(10000)) { throw 'Reopened window did not close' }
    $un=Start-Process "$install/Uninstall.exe" -ArgumentList '/S',("_?="+$install) -Wait -PassThru
    if ($un.ExitCode -or (Test-Path "$install/midad.exe")) { throw 'Uninstall failed' }
    Write-Host 'Install, open, edit, save, reopen, and uninstall passed.'
} finally {
    if ($process -and !$process.HasExited) {$process.Kill()}
    Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
}
