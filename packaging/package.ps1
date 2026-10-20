<#
.SYNOPSIS
    Builds the release archive for the version in Cargo.toml.

.DESCRIPTION
    One archive, one set of files, one set of hashes. The archive holds the executable,
    the documents that ship beside it (README, privacy page, both licences) and the channel
    file the in-app update check reads.

    The archive is byte-for-byte reproducible from the same executable: entries are sorted
    by name and every entry carries the same fixed timestamp, so two runs over one build
    produce one hash. -Verify proves it by packing twice and comparing bytes, and by
    unpacking the archive and comparing every file against what went in.

.PARAMETER Exe
    The executable to package. Default: target\<Target>\release\rubrica-app.exe, built first
    unless -SkipBuild is passed.

.PARAMETER OutDir
    Where the archive and SHA256SUMS are written. Default: dist\ beside this script's root.

.PARAMETER Target
    The Rust target triple to build for. Default: x86_64-pc-windows-msvc.

.PARAMETER SkipBuild
    Package the executable that is already on disk instead of building it.

.PARAMETER Verify
    Unpack the archive, compare every file byte for byte, pack it a second time from the
    same staged files, and compare the two archives' hashes.

.EXAMPLE
    pwsh -File packaging\package.ps1 -Verify

.EXAMPLE
    pwsh -File packaging\package.ps1 -Exe target\x86_64-pc-windows-msvc\release\rubrica-app.exe -SkipBuild
#>
[CmdletBinding()]
param(
    [string]$Exe,
    [string]$OutDir,
    [string]$Target = 'x86_64-pc-windows-msvc',
    [switch]$SkipBuild,
    [switch]$Verify
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$global:LASTEXITCODE = 0
$root = Split-Path -Parent $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $root 'dist' }
$OutDir = [System.IO.Path]::GetFullPath($OutDir)

# The version has one home, and this reads it from there rather than from a second copy.
function Get-WorkspaceVersion {
    $text = Get-Content -Raw (Join-Path $root 'Cargo.toml')
    $parts = $text -split '\[workspace\.package\]', 2
    if ($parts.Count -lt 2) { throw 'Cargo.toml has no [workspace.package] section' }
    if ($parts[1] -match 'version\s*=\s*"([^"]+)"') { return $Matches[1] }
    throw 'Cargo.toml has no [workspace.package] version'
}

# git writes its "no tag here" to stderr, and this script stops on errors, so a failed
# question has to be a value rather than an exception.
function Invoke-Git([string[]]$arguments) {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $out = @(& git @arguments 2>$null)
        return [pscustomobject]@{ Code = $LASTEXITCODE; Lines = @($out | Where-Object { $null -ne $_ }) }
    } finally {
        $ErrorActionPreference = $previous
    }
}

# What the release page will say, read from the commits that made this version. A machine
# without git still packages; it just ships a channel file with no notes.
function Get-ReleaseNotes {
    $notes = @()
    if (Get-Command git -ErrorAction SilentlyContinue) {
        Push-Location $root
        try {
            # On a tag being released, the commits are the ones since the tag before it;
            # anywhere else they are the ones since the last tag.
            $head = Invoke-Git @('describe', '--tags', '--exact-match', 'HEAD')
            if ($head.Code -eq 0 -and $head.Lines.Count -gt 0) {
                $last = Invoke-Git @('describe', '--tags', '--abbrev=0', 'HEAD^')
            } else {
                $last = Invoke-Git @('describe', '--tags', '--abbrev=0')
            }
            $haveTag = ($last.Code -eq 0 -and $last.Lines.Count -gt 0)
            $range = if ($haveTag) { "$($last.Lines[0])..HEAD" } else { 'HEAD' }
            $log = Invoke-Git @('log', '--format=%s', '--no-merges', '-n', '12', $range)
            if ($log.Code -eq 0) {
                $notes = @($log.Lines | Where-Object { $_ -and $_.Trim().Length -gt 0 } | ForEach-Object { $_.Trim() })
            }
        } finally {
            Pop-Location
        }
    }
    if ($notes.Count -eq 0) { $notes = @('See the release page for what changed.') }
    return $notes
}

# A zip with no clock in it: sorted entries, one fixed timestamp, so the same staged files
# always produce the same bytes. Nothing else here is allowed to leak the time of day.
function New-Archive([string]$stage, [string]$zipPath) {
    if (Test-Path $zipPath) { Remove-Item -LiteralPath $zipPath -Force }
    $fixed = [System.DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [System.TimeSpan]::Zero)
    $archive = [System.IO.Compression.ZipFile]::Open($zipPath, [System.IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($file in (Get-ChildItem -LiteralPath $stage -File | Sort-Object Name)) {
            $entry = $archive.CreateEntry($file.Name, [System.IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = $fixed
            $out = $entry.Open()
            try {
                $in = [System.IO.File]::OpenRead($file.FullName)
                try { $in.CopyTo($out) } finally { $in.Dispose() }
            } finally {
                $out.Dispose()
            }
        }
    } finally {
        $archive.Dispose()
    }
}

function Format-Hash([string]$path) {
    (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
}

$version = Get-WorkspaceVersion
$zipName = "rubrica-v$version-win-x64.zip"
$zipPath = Join-Path $OutDir $zipName

if (-not $Exe) {
    $built = Join-Path $root "target\$Target\release\rubrica-app.exe"
    if (-not $SkipBuild) {
        Write-Host "building rubrica-app $version for $Target"
        & cargo build --release --locked -p rubrica-app --target $Target --manifest-path (Join-Path $root 'Cargo.toml')
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    }
    $Exe = $built
}
if (-not (Test-Path -LiteralPath $Exe)) { throw "no executable at $Exe" }
$Exe = (Resolve-Path -LiteralPath $Exe).Path

# The staging directory is the archive's content, so what -Verify compares is exactly what
# was packed rather than a second copy of the same idea.
$stage = Join-Path $env:TEMP "rubrica-package-$version"
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Path $stage | Out-Null

$shipped = @()
Copy-Item -LiteralPath $Exe -Destination (Join-Path $stage 'rubrica-app.exe')
$shipped += 'rubrica-app.exe'
foreach ($name in @('README.md', 'PRIVACY.md', 'LICENSE-MIT', 'LICENSE-APACHE')) {
    $from = Join-Path $root $name
    if (-not (Test-Path -LiteralPath $from)) { throw "$name is missing, and every archive ships it" }
    Copy-Item -LiteralPath $from -Destination (Join-Path $stage $name)
    $shipped += $name
}
# The two scripts travel with the archive: unpacking it and running install.ps1 is the whole
# installation, and the uninstaller that undoes it is right there beside it.
foreach ($name in @('install.ps1', 'uninstall.ps1')) {
    $from = Join-Path $root "packaging\$name"
    if (-not (Test-Path -LiteralPath $from)) { throw "packaging\$name is missing, and every archive ships it" }
    Copy-Item -LiteralPath $from -Destination (Join-Path $stage $name)
    $shipped += $name
}

$channel = [ordered]@{
    version = $version
    channel = 'stable'
    notes   = @(Get-ReleaseNotes)
    url     = "https://github.com/LiJiaHua1024/rubrica/releases/tag/v$version"
}
$channelText = ($channel | ConvertTo-Json -Depth 4) + "`n"
$utf8 = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText((Join-Path $stage 'update-channel.json'), $channelText, $utf8)
$shipped += 'update-channel.json'

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
New-Archive -stage $stage -zipPath $zipPath
Copy-Item -LiteralPath $Exe -Destination (Join-Path $OutDir 'rubrica-app.exe') -Force
# The release page also carries the documents loose, as it always has: a reader who does not
# want the archive can still read what it would have told them.
foreach ($name in @('README.md', 'PRIVACY.md', 'LICENSE-MIT', 'LICENSE-APACHE')) {
    Copy-Item -LiteralPath (Join-Path $stage $name) -Destination (Join-Path $OutDir $name) -Force
}

$lines = @()
foreach ($name in @('rubrica-app.exe', $zipName)) {
    $lines += ('{0}  {1}' -f (Format-Hash (Join-Path $OutDir $name)), $name)
}
$sums = Join-Path $OutDir 'SHA256SUMS'
[System.IO.File]::WriteAllText($sums, (($lines -join "`n") + "`n"), $utf8)

Write-Host "rubrica $version"
foreach ($name in @('rubrica-app.exe', $zipName, 'SHA256SUMS')) {
    $path = Join-Path $OutDir $name
    Write-Host ("  {0,-34} {1,9} bytes  {2}" -f $name, (Get-Item -LiteralPath $path).Length, (Format-Hash $path).Substring(0, 24))
}
Write-Host ("  archive holds: {0}" -f ($shipped -join ', '))

if ($Verify) {
    Write-Host 'verify: unpacking and comparing every file'
    $unpack = Join-Path $env:TEMP "rubrica-unpack-$version"
    if (Test-Path -LiteralPath $unpack) { Remove-Item -LiteralPath $unpack -Recurse -Force }
    [System.IO.Compression.ZipFile]::ExtractToDirectory($zipPath, $unpack)
    $problems = 0
    foreach ($file in (Get-ChildItem -LiteralPath $stage -File | Sort-Object Name)) {
        $copy = Join-Path $unpack $file.Name
        if (-not (Test-Path -LiteralPath $copy)) {
            Write-Host ("  missing  {0}" -f $file.Name)
            $problems++
            continue
        }
        $packed = Format-Hash $copy
        $original = Format-Hash $file.FullName
        $size = (Get-Item -LiteralPath $copy).Length
        if ($packed -eq $original -and $size -eq $file.Length) {
            Write-Host ("  same     {0,-22} {1,9} bytes  {2}" -f $file.Name, $size, $original.Substring(0, 16))
        } else {
            Write-Host ("  DIFFERS  {0,-22} {1,9} bytes  {2}" -f $file.Name, $size, $packed)
            $problems++
        }
    }
    $staged = @(Get-ChildItem -LiteralPath $stage -File | ForEach-Object { $_.Name } | Sort-Object)
    $unpacked = @(Get-ChildItem -LiteralPath $unpack -File | ForEach-Object { $_.Name } | Sort-Object)
    $extra = @($unpacked | Where-Object { $staged -notcontains $_ })
    if ($extra.Count -gt 0) {
        Write-Host ("  unexpected in archive: {0}" -f ($extra -join ', '))
        $problems++
    }
    if ($unpacked.Count -ne $staged.Count) {
        Write-Host ("  archive holds {0} files, staging had {1}" -f $unpacked.Count, $staged.Count)
        $problems++
    }

    Write-Host 'verify: packing the same files again'
    $again = Join-Path $env:TEMP "rubrica-repack-$version.zip"
    New-Archive -stage $stage -zipPath $again
    $first = Format-Hash $zipPath
    $second = Format-Hash $again
    if ($first -eq $second) {
        Write-Host ("  same     {0} twice: {1}" -f $zipName, $first)
    } else {
        Write-Host ("  DIFFERS  {0}: {1} then {2}" -f $zipName, $first, $second)
        $problems++
    }
    if ($problems -ne 0) { throw "verification found $problems problem(s)" }
    Write-Host 'verify: the archive is the staged files, and repacking reproduces it'
}
