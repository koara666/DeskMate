# Measure DeskMate release build: startup time, idle CPU, memory (all WebView2 child processes).
# Usage: powershell -File scripts/measure.ps1 -Exe src-tauri/target/release/deskmate.exe -DataDir <temp dir> [-IdleSeconds 30]
eleasedeskmate.exe -DataDir <temp dir> [-IdleSeconds 30]
# Keep this file ASCII-only: Windows PowerShell 5.1 misreads UTF-8 without BOM.
param([string]$Exe, [string]$DataDir, [int]$IdleSeconds = 30)
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public class MW {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  public static bool HasVisible(uint pid, string title) {
    bool f = false;
    EnumWindows((h,l)=>{ uint p; GetWindowThreadProcessId(h, out p); var sb=new StringBuilder(256); GetWindowText(h,sb,256);
      if (p==pid && IsWindowVisible(h) && sb.ToString()==title) { f=true; return false; } return true; }, IntPtr.Zero);
    return f;
  }
}
"@
function Get-Tree($rootId) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
  $ids = @($rootId); $added = $true
  while ($added) { $added = $false
    foreach ($p in $all) { if (($ids -contains $p.ParentProcessId) -and -not ($ids -contains $p.ProcessId)) { $ids += $p.ProcessId; $added = $true } } }
  return $ids
}
New-Item -ItemType Directory -Force $DataDir | Out-Null
$env:DESKMATE_DATA_DIR = $DataDir
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $Exe -PassThru
while (-not [MW]::HasVisible([uint32]$p.Id, "DeskMate")) { Start-Sleep -Milliseconds 20; if ($sw.ElapsedMilliseconds -gt 15000) { break } }
"window_visible_ms=" + $sw.ElapsedMilliseconds
# wait for UI render and chat window pre-creation before measuring idle
Start-Sleep -Seconds 8
$ids = Get-Tree $p.Id
$procs = $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
$t0 = Get-Date
$cpuStart = 0; $per = @{}; foreach ($x in $procs) { $cpuStart += $x.TotalProcessorTime.TotalMilliseconds; $per[$x.Id] = $x.TotalProcessorTime.TotalMilliseconds }
Start-Sleep -Seconds $IdleSeconds
$procs2 = $ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue }
$cpuEnd = 0; foreach ($x in $procs2) { $cpuEnd += $x.TotalProcessorTime.TotalMilliseconds }
$elapsed = ((Get-Date) - $t0).TotalMilliseconds
$cores = [Environment]::ProcessorCount
"idle_cpu_percent_of_total=" + [math]::Round(($cpuEnd - $cpuStart) / $elapsed / $cores * 100, 3)
"idle_cpu_percent_of_one_core=" + [math]::Round(($cpuEnd - $cpuStart) / $elapsed * 100, 3)
$ws = 0; $priv = 0
foreach ($x in $procs2) { $ws += $x.WorkingSet64; $priv += $x.PrivateMemorySize64; $cmd = (Get-CimInstance Win32_Process -Filter "ProcessId=$($x.Id)").CommandLine; $type = if ($cmd -match '--type=([a-z-]+)') { $Matches[1] } else { 'main' }; "  {0,-16} {1,-14} ws={2,6:N1}MB private={3,6:N1}MB cpu_ms={4,6:N0}" -f $x.ProcessName, $type, ($x.WorkingSet64/1MB), ($x.PrivateMemorySize64/1MB), ($x.TotalProcessorTime.TotalMilliseconds - $per[$x.Id]) }
"total_working_set_mb=" + [math]::Round($ws/1MB,1)
"total_private_mb=" + [math]::Round($priv/1MB,1)
"process_count=" + $procs2.Count
Stop-Process -Id $p.Id -Force
