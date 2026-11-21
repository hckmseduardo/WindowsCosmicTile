param([Parameter(Mandatory)] [string] $Executable)

$ErrorActionPreference = 'Stop'
$source = (Resolve-Path -LiteralPath $Executable).Path
$originalAppData = $env:APPDATA
foreach ($recovery in @($false, $true)) {
    $name = "CosmicTile update $([Guid]::NewGuid().ToString('N'))"
    $folder = Join-Path $env:RUNNER_TEMP $name
    $stage = Join-Path $folder '.cosmictile-update-012345ABCDEF'
    $installed = Join-Path $folder "$name.exe"
    $previous = Join-Path $stage 'previous.exe'
    $replacement = Join-Path $stage 'replacement.exe'
    $helper = $null
    $mutex = $null
    $ownsMutex = $false
    try {
        $env:APPDATA = Join-Path $folder 'appdata'
        $settings = Join-Path $env:APPDATA 'WindowsCosmicTile'
        New-Item -ItemType Directory -Path $stage, $settings -Force | Out-Null
        Set-Content (Join-Path $settings 'settings.conf') -Value @('auto_updates=false', 'gap=16')
        Copy-Item $source $installed
        Copy-Item $source $previous
        $originalHash = (Get-FileHash $installed).Hash
        if ($recovery) {
            Set-Content $replacement -Value 'Invalid executable'
            $expectedHash = $originalHash
        } else {
            Copy-Item $source $replacement
            $stream = [IO.File]::Open($replacement, [IO.FileMode]::Append)
            try { $stream.WriteByte(42) } finally { $stream.Dispose() }
            $expectedHash = (Get-FileHash $replacement).Hash
        }

        $created = $false
        $mutex = [Threading.Mutex]::new($true, 'Local\WindowsCosmicTile.SingleInstance', [ref]$created)
        if (!$created) { throw 'The runner already has a CosmicTile instance' }
        $ownsMutex = $true
        $helper = Start-Process -FilePath $previous -ArgumentList @('--apply-update', ('"{0}.exe"' -f $name), 'en') -PassThru
        if ($helper.WaitForExit(500) -or (Get-FileHash $installed).Hash -ne $originalHash) {
            throw 'The updater did not wait for the original instance to exit'
        }
        $mutex.ReleaseMutex()
        $ownsMutex = $false
        $mutex.Dispose()
        $mutex = $null

        if (!$helper.WaitForExit(40000) -or $helper.ExitCode -ne 0) {
            throw 'The updater could not restart CosmicTile'
        }
        if ((Get-FileHash $installed).Hash -ne $expectedHash) {
            throw "Unexpected bytes at the original executable path (recovery: $recovery)"
        }
        if (!(Get-Process -Name $name -ErrorAction SilentlyContinue)) {
            throw 'CosmicTile did not remain running at the original path'
        }
        foreach ($attempt in 1..40) {
            if (!(Test-Path -LiteralPath $stage)) { break }
            Start-Sleep -Milliseconds 100
        }
        if (Test-Path -LiteralPath $stage) { throw 'The staging folder was left behind' }
        Write-Host "In-place restart passed (recovery: $recovery)"
    } finally {
        if ($ownsMutex) { $mutex.ReleaseMutex() }
        if ($mutex) { $mutex.Dispose() }
        if ($helper -and !$helper.HasExited) { Stop-Process -Id $helper.Id -Force }
        $running = Get-Process -Name $name -ErrorAction SilentlyContinue
        if ($running) {
            $running | Stop-Process -Force
            $running | Wait-Process -Timeout 10
        }
        $env:APPDATA = $originalAppData
        Remove-Item -LiteralPath $folder -Recurse -Force
    }
}
