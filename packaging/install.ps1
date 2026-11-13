# Installs or updates scope, the IDE for Claude Code sessions, on Windows from its latest
# release. In PowerShell:
#
#   irm https://github.com/GuintherKovalski/sessman/releases/latest/download/install.ps1 | iex
#
# Puts scope.exe in %LOCALAPPDATA%\Programs\scope, adds that folder to your PATH (so `scope list`
# works in a terminal), adds scope to the Start menu and opens it. Run it again any time to update.
# Environment, all optional:
#   SCOPE_REPO=owner/name   repository to install from
#   SCOPE_DIR=C:\path       where to put scope.exe
#   SCOPE_FROM=URL|folder   where scope-windows-x64.zip and SHA256SUMS are (default: latest release)
#   SCOPE_NO_OPEN=1         don't open it afterwards
#   SCOPE_NO_MODEL=1        don't download the speech model for dictation (about 150 MB)
#   SCOPE_NO_CLAUDE=1       don't install Claude Code when it is missing
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'  # Invoke-WebRequest is many times slower with it on.

function Fail($message) {
    Write-Host "scope: $message" -ForegroundColor Red
    exit 1
}

if ($env:OS -ne 'Windows_NT') { Fail 'this installer is for Windows; on macOS use install.sh.' }
if (-not [Environment]::Is64BitOperatingSystem) { Fail 'scope needs 64-bit Windows.' }
if ([Environment]::OSVersion.Version.Major -lt 10) { Fail 'scope needs Windows 10 or later.' }

$repo = if ($env:SCOPE_REPO) { $env:SCOPE_REPO } else { 'GuintherKovalski/sessman' }
$from = if ($env:SCOPE_FROM) { $env:SCOPE_FROM.TrimEnd('/', '\') } else { "https://github.com/$repo/releases/latest/download" }
$dest = if ($env:SCOPE_DIR) { $env:SCOPE_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\scope' }
$zipName = 'scope-windows-x64.zip'

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("scope-install-" + [Guid]::NewGuid())
New-Item -ItemType Directory -Force $tmp | Out-Null

# A folder on this computer (a build), or a web address.
function Fetch($name, $to) {
    if (Test-Path -LiteralPath $from) {
        Copy-Item -LiteralPath (Join-Path $from $name) $to
    } else {
        Invoke-WebRequest -UseBasicParsing -Uri "$from/$name" -OutFile $to
    }
}

try {
    Write-Host 'Downloading scope...'
    $zip = Join-Path $tmp $zipName
    try { Fetch $zipName $zip } catch { Fail "couldn't download $from/$zipName ($($_.Exception.Message))" }
    $sums = Join-Path $tmp 'SHA256SUMS'
    try { Fetch 'SHA256SUMS' $sums } catch { Fail "couldn't download the checksums ($($_.Exception.Message))" }
    $want = Get-Content $sums | ForEach-Object {
        $parts = $_ -split '\s+', 2
        if ($parts.Count -eq 2 -and $parts[1].TrimStart('*') -eq $zipName) { $parts[0].ToLower() }
    } | Select-Object -First 1
    $have = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
    if (-not $want -or $want -ne $have) { Fail "the download doesn't match its checksum; nothing was installed." }

    $unpacked = Join-Path $tmp 'unpacked'
    Expand-Archive -LiteralPath $zip -DestinationPath $unpacked -Force
    $new = Join-Path $unpacked 'scope.exe'
    if (-not (Test-Path $new)) { Fail 'the download has no scope.exe in it.' }

    New-Item -ItemType Directory -Force $dest | Out-Null
    $exe = Join-Path $dest 'scope.exe'
    $running = @(Get-Process -Name scope -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $exe }).Count -gt 0
    # A running scope.exe can't be overwritten, but it can be renamed: it goes on from there, and
    # scope removes the old file the next time it starts.
    $old = "$exe.old"
    Remove-Item -LiteralPath $old -Force -ErrorAction SilentlyContinue
    if (Test-Path $exe) { Move-Item -LiteralPath $exe $old -Force }
    try {
        Copy-Item -LiteralPath $new $exe -Force
    } catch {
        if (Test-Path $old) { Move-Item -LiteralPath $old $exe -Force }
        Fail "couldn't write $exe ($($_.Exception.Message))"
    }
    if (-not $running) { Remove-Item -LiteralPath $old -Force -ErrorAction SilentlyContinue }
    Write-Host "Installed scope in $exe"

    # `scope list|rename|move|new|ask` from a terminal, and from Claude sessions.
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $entries = @($userPath -split ';' | Where-Object { $_ })
    if (-not ($entries | Where-Object { $_.TrimEnd('\') -ieq $dest.TrimEnd('\') })) {
        [Environment]::SetEnvironmentVariable('Path', (($entries + $dest) -join ';'), 'User')
        Write-Host "Added $dest to your PATH (new terminals see it)."
    }

    # Start menu.
    $programs = [Environment]::GetFolderPath('Programs')
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut((Join-Path $programs 'scope.lnk'))
    $link.TargetPath = $exe
    $link.WorkingDirectory = $env:USERPROFILE
    $link.Description = 'scope: your Claude Code sessions, organized'
    $link.Save()

    # Dictation's speech model, so the mic works at once (scope would otherwise ask the first time).
    $config = if ($env:SCOPE_CONFIG) { $env:SCOPE_CONFIG } else { Join-Path $env:USERPROFILE '.config\scope' }
    $modelDir = Join-Path $config 'models'
    $model = Join-Path $modelDir 'ggml-base.bin'
    $modelBytes = 147951465
    $haveModel = (Test-Path $model) -and ((Get-Item $model).Length -eq $modelBytes)
    if (-not $env:SCOPE_NO_MODEL -and -not $haveModel) {
        Write-Host 'Downloading the speech model for dictation (about 150 MB)...'
        New-Item -ItemType Directory -Force $modelDir | Out-Null
        $part = "$model.part"
        $url = if ($env:SCOPE_MODEL_URL) { $env:SCOPE_MODEL_URL } else { 'https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin' }
        try {
            Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $part
            if ((Get-Item $part).Length -ne $modelBytes) { throw 'wrong size' }
            Move-Item -LiteralPath $part $model -Force
        } catch {
            Remove-Item -LiteralPath $part -Force -ErrorAction SilentlyContinue
            Write-Host "Couldn't download the speech model; scope offers it again the first time you dictate."
        }
    }

    # Claude Code, which scope runs the sessions with.
    function Test-Claude {
        (Get-Command claude -ErrorAction SilentlyContinue) -or (Test-Path (Join-Path $env:USERPROFILE '.local\bin\claude.exe'))
    }
    if (-not (Test-Claude)) {
        if (-not $env:SCOPE_NO_CLAUDE) {
            Write-Host 'Installing Claude Code (scope runs your sessions with it)...'
            try { Invoke-RestMethod https://claude.ai/install.ps1 | Invoke-Expression } catch { }
        }
        if (-not (Test-Claude)) {
            Write-Host ''
            Write-Host 'scope needs Claude Code. Install it with:  irm https://claude.ai/install.ps1 | iex'
        }
    }
    if (Test-Claude) { Write-Host "Claude Code is ready. If you haven't yet, sign in once by running: claude" }

    if ($running) {
        Write-Host 'scope is open: close it and open it again to use the new version. Your sessions keep running.'
    } elseif (-not $env:SCOPE_NO_OPEN) {
        Start-Process -FilePath $exe
    }
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
