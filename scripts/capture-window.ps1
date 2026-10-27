# Capture one window by exact title into a PNG, without focusing it or touching mouse/keyboard.
# Restores the window (without activating) if it is minimized. Uses PrintWindow(PW_RENDERFULLCONTENT).
# Usage: powershell -File scripts/capture-window.ps1 -Title "DeskMate <title>" -Out shot.png
# Keep this file ASCII-only: Windows PowerShell 5.1 misreads UTF-8 without BOM.
param([string]$Title, [string]$Out)
[void][Reflection.Assembly]::LoadWithPartialName("System.Drawing")
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public class CapWin {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public static IntPtr Find(string title) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => { var sb = new StringBuilder(256); GetWindowText(h, sb, 256);
      if (IsWindowVisible(h) && sb.ToString() == title) { found = h; return false; } return true; }, IntPtr.Zero);
    return found;
  }
}
"@
[CapWin]::SetProcessDPIAware() | Out-Null
$h = [CapWin]::Find($Title)
if ($h -eq [IntPtr]::Zero) { "window not found: $Title"; exit 1 }
if ([CapWin]::IsIconic($h)) { [CapWin]::ShowWindow($h, 4) | Out-Null; Start-Sleep -Milliseconds 800 }
$r = New-Object CapWin+RECT
[CapWin]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.R - $r.L; $hh = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
[CapWin]::PrintWindow($h, $hdc, 2) | Out-Null
$g.ReleaseHdc($hdc)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
"captured {0}x{1}" -f $w, $hh
