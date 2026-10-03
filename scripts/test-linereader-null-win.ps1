$ErrorActionPreference = 'Stop'

function Start-LineReader($Reader, $Writer) {
    $hostPs = [powershell]::Create()
    if ($null -ne $Writer) {
        $null = $hostPs.AddScript({
            param($reader, $writer)
            try {
                while ($null -ne ($line = $reader.ReadLine())) {
                    $writer.WriteLine($line)
                }
            } catch {}
        }).AddArgument($Reader).AddArgument($Writer)
    } else {
        $null = $hostPs.AddScript({
            param($reader)
            try {
                while ($null -ne ($line = $reader.ReadLine())) {}
            } catch {}
        }).AddArgument($Reader)
    }
    return @{ Host = $hostPs; Async = $hostPs.BeginInvoke() }
}

function Stop-LineReader($Job) {
    if ($null -eq $Job) { return }
    try { $null = $Job.Host.EndInvoke($Job.Async) } catch {}
    try { $Job.Host.Dispose() } catch {}
}

Write-Host "Testing Start-LineReader with null writer on Windows PowerShell $($PSVersionTable.PSVersion)..."

$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = "powershell.exe"
$psi.Arguments = '-NoProfile -Command "Write-Output line1; Write-Output line2"'
$psi.UseShellExecute = $false
$psi.RedirectStandardOutput = $true
$psi.CreateNoWindow = $true

$proc = [System.Diagnostics.Process]::Start($psi)
$job = $null
try {
    $job = Start-LineReader $proc.StandardOutput $null
    [void]$proc.WaitForExit(5000)
    Write-Host "PASS: Start-LineReader with null writer did not throw" -ForegroundColor Green
    exit 0
} catch {
    Write-Host "FAIL: Start-LineReader with null writer threw: $_" -ForegroundColor Red
    exit 1
} finally {
    Stop-LineReader $job
    try { $proc.Dispose() } catch {}
}
