<#
.SYNOPSIS
    Uninstalls Rubrica for the current user, removing exactly what install.ps1 created.

.DESCRIPTION
    The install folder holds install.json: the files, the shortcut, the PATH entry and the
    App Paths key that one install made. This script removes those and nothing else. It
    does not hunt for a key that might be Rubrica's, and it does not remove the reading
    settings unless asked to.

.PARAMETER InstallDir
    Where Rubrica was installed. Default: %LOCALAPPDATA%\Programs\Rubrica.

.PARAMETER Purge
    Also remove the reading settings under HKCU\Software\Rubrica: recent documents,
    favourites, and every switch the reader set. Without it they are kept, so that
    reinstalling does not cost the reader their place.

.PARAMETER SettingsKey
    Where the reading settings live, if not HKCU\Software\Rubrica. An explicit value wins
    over the one install.json records.

.PARAMETER Quiet
    Say nothing except what went wrong.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File uninstall.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File uninstall.ps1 -Purge
#>
[CmdletBinding()]
param(
    [string]$InstallDir,
    [string]$SettingsKey,
    [switch]$Purge,
    [switch]$Quiet
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$AppName = 'Rubrica'
$ExeName = 'rubrica-app.exe'
if (-not $SettingsKey) { $SettingsKey = 'HKCU:\Software\Rubrica' }
$SettingsKeyBound = $PSBoundParameters.ContainsKey('SettingsKey')
$AppPathKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths\rubrica-app.exe'
$ShortcutPath = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$AppName.lnk"

function Say([string]$text) {
    if (-not $Quiet) { Write-Host $text }
}

if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$AppName" }
$InstallDir = [System.IO.Path]::GetFullPath($InstallDir)

$manifestPath = Join-Path $InstallDir 'install.json'
$manifest = $null
if (Test-Path -LiteralPath $manifestPath) {
    $manifest = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
    Say "uninstalling rubrica $($manifest.version) from $InstallDir"
} else {
    if (-not (Test-Path -LiteralPath (Join-Path $InstallDir $ExeName))) {
        throw "nothing is installed at ${InstallDir}: no install.json and no $ExeName"
    }
    Say "no install.json at $InstallDir; removing the program and the shortcut only"
}

# Where the settings live: install.json records it, and an explicit -SettingsKey wins over
# that record. The override exists so the purge path can be exercised without touching a
# reader's own settings.
if (-not $SettingsKeyBound -and $manifest) {
    $recorded = $manifest.PSObject.Properties['settingsKey']
    if ($recorded -and $recorded.Value) { $SettingsKey = [string]$recorded.Value }
}

# The shortcut the manifest names, or the default one when there is no manifest to ask.
$shortcut = $ShortcutPath
$named = $null
if ($manifest) { $named = $manifest.PSObject.Properties['shortcut'] }
if ($named -and $named.Value) { $shortcut = [string]$named.Value }
if (Test-Path -LiteralPath $shortcut) {
    Remove-Item -LiteralPath $shortcut -Force
    Say "  removed $shortcut"
}

# PATH: the entry is removed only if this install put it there, and only the one segment
# that names this install folder. Every other segment is written back as it was found.
$pathAdded = $false
if ($manifest) {
    $flag = $manifest.PSObject.Properties['pathAdded']
    if ($flag -and $flag.Value -eq $true) { $pathAdded = $true }
}
if ($pathAdded) {
    $environment = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try {
        if (@($environment.GetValueNames()) -contains 'Path') {
            $current = [string]$environment.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            # Split without dropping the empty fields, so that a PATH ending in a separator
            # gets its separator back: only the one segment that names this folder goes.
            $parts = @($current -split ';')
            $kept = @($parts | Where-Object {
                -not ($_.Trim().Length -gt 0 -and $_.Trim().TrimEnd('\') -ieq $InstallDir.TrimEnd('\'))
            })
            $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString
            if ($environment.GetValueKind('Path') -eq [Microsoft.Win32.RegistryValueKind]::String) {
                $kind = [Microsoft.Win32.RegistryValueKind]::String
            }
            $environment.SetValue('Path', ($kept -join ';'), $kind)
            Say "  PATH: removed $InstallDir"
        }
    } finally {
        $environment.Close()
    }
}

if (Test-Path -LiteralPath $AppPathKey) {
    Remove-Item -LiteralPath $AppPathKey -Recurse -Force
    Say "  app paths: removed $AppPathKey"
}

# The files, from the manifest if it is there, and otherwise the ones an install puts down.
$files = @()
if ($manifest) {
    $listed = $manifest.PSObject.Properties['files']
    if ($listed -and $listed.Value) { $files = @($listed.Value) }
}
if ($files.Count -eq 0) {
    $files = @($ExeName, 'README.md', 'PRIVACY.md', 'LICENSE-MIT', 'LICENSE-APACHE', 'update-channel.json', 'uninstall.ps1')
}
foreach ($name in $files) {
    $path = Join-Path $InstallDir $name
    if (Test-Path -LiteralPath $path) {
        Remove-Item -LiteralPath $path -Force
        Say "  removed $path"
    }
}

if ($Purge) {
    if (Test-Path -LiteralPath $SettingsKey) {
        Remove-Item -LiteralPath $SettingsKey -Recurse -Force
        Say "  settings: removed $SettingsKey"
    }
} else {
    Say "  settings: kept ($SettingsKey). Pass -Purge to remove them too."
}

# The folder itself, once the files in it are gone. Anything the reader put there is not
# ours to delete, so a folder that is not empty stays and is named.
if (Test-Path -LiteralPath $InstallDir) {
    if (Test-Path -LiteralPath $manifestPath) { Remove-Item -LiteralPath $manifestPath -Force }
    $left = @(Get-ChildItem -LiteralPath $InstallDir -Force)
    if ($left.Count -eq 0) {
        Remove-Item -LiteralPath $InstallDir -Force
        Say "  removed $InstallDir"
    } else {
        Say ("  {0} still holds {1}; leaving it" -f $InstallDir, (($left | ForEach-Object { $_.Name }) -join ', '))
    }
}

Say ''
Say 'Rubrica is uninstalled.'
