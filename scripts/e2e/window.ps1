# E2E helper: list or close the top-level windows of one process (no mouse/keyboard simulation).
# Usage: powershell -File scripts/e2e/window.ps1 -ProcessId 1234 [-Close pet|chat|settings]
# Output (list): one line per window: <which>|<visible 0/1>
# Keep this file ASCII-only: Windows PowerShell 5.1 misreads UTF-8 without BOM.
param([int]$ProcessId, [string]$Close = "")
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public class E2EWin {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  public static List<KeyValuePair<IntPtr,string>> List(uint pid) {
    var res = new List<KeyValuePair<IntPtr,string>>();
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p);
      if (p == pid) { var sb = new StringBuilder(256); GetWindowText(h, sb, 256); res.Add(new KeyValuePair<IntPtr,string>(h, sb.ToString())); }
      return true; }, IntPtr.Zero);
    return res;
  }
}
"@
$chatTitle = "DeskMate " + [char]0x804A + [char]0x5929
$settingsTitle = "DeskMate " + [char]0x8BBE + [char]0x7F6E
function Which($t) {
  if ($t -eq "DeskMate") { return "pet" }
  if ($t -eq $chatTitle) { return "chat" }
  if ($t -eq $settingsTitle) { return "settings" }
  return ""
}
foreach ($kv in [E2EWin]::List([uint32]$ProcessId)) {
  $w = Which $kv.Value
  if ($w -eq "") { continue }
  if ($Close -ne "" -and $w -eq $Close) {
    # WM_CLOSE = 0x0010, same as clicking the title bar close button
    [E2EWin]::PostMessage($kv.Key, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    "closed|$w"
  } else {
    $v = if ([E2EWin]::IsWindowVisible($kv.Key)) { 1 } else { 0 }
    "$w|$v"
  }
}
