# Terminus Windows reload watcher (Windows PowerShell 5.1 compatible).
#
# Watches reload.stamp in the deploy directory. On each change: stop the
# running app, swap rio-dev.exe.new -> rio-dev.exe, restart.
# Writes watcher.alive so the WSL side can tell whether this process is up.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File win-reload.ps1 `
#     -DeployDir "$env:LOCALAPPDATA\terminus-dev" `
#     -ConfigHome "\\wsl$\NixOS\home\...\terminus\.dev\config"

param(
    [Parameter(Mandatory = $true)]
    [string]$DeployDir,

    [Parameter(Mandatory = $false)]
    [string]$ConfigHome = "",

    [Parameter(Mandatory = $false)]
    [string]$LogLevel = "info",

    [Parameter(Mandatory = $false)]
    [string]$ExeName = "rio-dev.exe",

    [Parameter(Mandatory = $false)]
    [int]$PollMs = 400
)

$ErrorActionPreference = "Stop"

# Normalize: "%~dp0." from the .cmd can leave a trailing "\."
$DeployDir = [System.IO.Path]::GetFullPath($DeployDir.TrimEnd('\', '/'))

$mutexName = "Global\TerminusWinReload"
$mutex = New-Object System.Threading.Mutex($false, $mutexName)
if (-not $mutex.WaitOne(0)) {
    Write-Host "win-reload: another watcher is already running"
    exit 0
}

$exePath = Join-Path $DeployDir $ExeName
$newPath = "$exePath.new"
$stampPath = Join-Path $DeployDir "reload.stamp"
$alivePath = Join-Path $DeployDir "watcher.alive"
$runLog = Join-Path $DeployDir "run-win.log"
$watcherLog = Join-Path $DeployDir "watcher.log"

function WriteLog([string]$msg) {
    $line = "[{0}] {1}" -f (Get-Date -Format "HH:mm:ss"), $msg
    try { Add-Content -LiteralPath $watcherLog -Value $line -ErrorAction SilentlyContinue } catch {}
    Write-Host $line
}

function TouchAlive {
    [System.IO.File]::WriteAllText($alivePath, (Get-Date).ToString("o"))
}

function StopApp {
    $base = [System.IO.Path]::GetFileNameWithoutExtension($ExeName)
    foreach ($name in @($base, "rio")) {
        Get-Process -Name $name -ErrorAction SilentlyContinue | ForEach-Object {
            try { $_.CloseMainWindow() | Out-Null } catch {}
            Start-Sleep -Milliseconds 200
            if (-not $_.HasExited) {
                Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
            }
        }
    }
    for ($i = 0; $i -lt 50; $i++) {
        if (-not (Test-Path -LiteralPath $exePath)) { return }
        try {
            $fs = [System.IO.File]::Open($exePath, "Open", "ReadWrite", "None")
            $fs.Close()
            return
        } catch {
            Start-Sleep -Milliseconds 100
        }
    }
}

function StartApp {
    if (-not (Test-Path -LiteralPath $exePath)) {
        WriteLog "no $ExeName yet - waiting for first deploy"
        return
    }

    $argList = @()
    $argsFile = Join-Path $DeployDir "app-args.txt"
    if (Test-Path -LiteralPath $argsFile) {
        $argList = @(Get-Content -LiteralPath $argsFile | Where-Object { $_ -ne "" })
    }

    $prevConfig = $env:RIO_CONFIG_HOME
    $prevLog = $env:RIO_LOG_LEVEL
    $env:RIO_LOG_LEVEL = $LogLevel
    if ($ConfigHome -ne "") {
        $env:RIO_CONFIG_HOME = $ConfigHome
    } else {
        Remove-Item Env:RIO_CONFIG_HOME -ErrorAction SilentlyContinue
    }

    try {
        $startParams = @{
            FilePath         = $exePath
            WorkingDirectory = $DeployDir
            PassThru         = $true
        }
        if ($argList.Count -gt 0) {
            $startParams["ArgumentList"] = $argList
        }
        $proc = Start-Process @startParams
        WriteLog ("started {0} (pid {1})" -f $ExeName, $proc.Id)
        try {
            Add-Content -LiteralPath $runLog -Value ("[{0}] started pid {1}" -f (Get-Date -Format "o"), $proc.Id)
        } catch {}
    } finally {
        if ($null -ne $prevConfig) { $env:RIO_CONFIG_HOME = $prevConfig }
        else { Remove-Item Env:RIO_CONFIG_HOME -ErrorAction SilentlyContinue }
        if ($null -ne $prevLog) { $env:RIO_LOG_LEVEL = $prevLog }
        else { Remove-Item Env:RIO_LOG_LEVEL -ErrorAction SilentlyContinue }
    }
}

function ApplyReload {
    if (-not (Test-Path -LiteralPath $newPath)) {
        $base = [System.IO.Path]::GetFileNameWithoutExtension($ExeName)
        $running = Get-Process -Name $base -ErrorAction SilentlyContinue
        if (-not $running) { StartApp }
        return
    }
    WriteLog "reload: swapping $ExeName"
    StopApp
    try {
        Move-Item -LiteralPath $newPath -Destination $exePath -Force
    } catch {
        WriteLog ("move failed: {0}" -f $_.Exception.Message)
        return
    }
    StartApp
}

try {
    if (-not (Test-Path -LiteralPath $DeployDir)) {
        New-Item -ItemType Directory -Path $DeployDir -Force | Out-Null
    }

    WriteLog ("watching {0}" -f $DeployDir)
    TouchAlive

    $lastStamp = $null
    if (Test-Path -LiteralPath $stampPath) {
        $lastStamp = (Get-Item -LiteralPath $stampPath).LastWriteTimeUtc
        ApplyReload
    }

    $aliveCounter = 0
    while ($true) {
        Start-Sleep -Milliseconds $PollMs
        $aliveCounter++
        if ($aliveCounter -ge 5) {
            TouchAlive
            $aliveCounter = 0
        }
        if (-not (Test-Path -LiteralPath $stampPath)) { continue }
        $stamp = (Get-Item -LiteralPath $stampPath).LastWriteTimeUtc
        if ($null -eq $lastStamp -or $stamp -gt $lastStamp) {
            $lastStamp = $stamp
            ApplyReload
        }
    }
} finally {
    try { $mutex.ReleaseMutex() | Out-Null } catch {}
    try { $mutex.Dispose() } catch {}
}
