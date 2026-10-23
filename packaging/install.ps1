<#
.SYNOPSIS
    Installs Rubrica for the current user: the files, a Start Menu shortcut, and the two
    registry entries that make it launchable by name.

.DESCRIPTION
    One step. Unpack the release archive and run this script from inside it -- or point it
    at the archive with -Zip and it unpacks it for you. Nothing outside the user's own
    profile is touched: no administrator, no service, no driver.

    What it creates is written down in install.json beside the executable, and
    uninstall.ps1 removes exactly those things and nothing else.

.PARAMETER Zip
    The release archive to install. Default: the folder this script runs from.

.PARAMETER From
    A folder already holding the archive's files, if not the one this script runs from.

.PARAMETER InstallDir
    Where to install. Default: %LOCALAPPDATA%\Programs\Rubrica.

.PARAMETER NoPath
    Do not add the install folder to the user's PATH.

.PARAMETER NoShortcut
    Do not create the Start Menu shortcut.

.PARAMETER Quiet
    Say nothing except what went wrong.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File install.ps1 -Zip .\rubrica-v0.10.3-win-x64.zip
#>
[CmdletBinding()]
param(
    [string]$Zip,
    [string]$From,
    [string]$InstallDir,
    [switch]$NoPath,
    [switch]$NoShortcut,
    [switch]$Quiet
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Add-Type -AssemblyName System.IO.Compression.FileSystem

$AppName = 'Rubrica'
$ExeName = 'rubrica-app.exe'
$SettingsKey = 'HKCU:\Software\Rubrica'
$AppPathKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\App Paths\rubrica-app.exe'
$ShortcutPath = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\$AppName.lnk"

function Say([string]$text) {
    if (-not $Quiet) { Write-Host $text }
}

function Get-ExpectedFiles {
    # uninstall.ps1 goes last: it removes these files in this order, and the script that is
    # running should be the last thing to go.
    @($ExeName, 'README.md', 'PRIVACY.md', 'LICENSE-MIT', 'LICENSE-APACHE', 'update-channel.json', 'uninstall.ps1')
}

# Where the files to install are: an unpacked folder, a zip, or this script's own folder.
$source = $null
$work = $null
if ($Zip) {
    if (-not (Test-Path -LiteralPath $Zip)) { throw "no archive at $Zip" }
    $Zip = (Resolve-Path -LiteralPath $Zip).Path
    $work = Join-Path $env:TEMP ("rubrica-install-" + [System.Guid]::NewGuid().ToString('n').Substring(0, 8))
    New-Item -ItemType Directory -Path $work | Out-Null
    [System.IO.Compression.ZipFile]::ExtractToDirectory($Zip, $work)
    $source = $work
} elseif ($From) {
    if (-not (Test-Path -LiteralPath $From)) { throw "no folder at $From" }
    $source = (Resolve-Path -LiteralPath $From).Path
} else {
    $source = $PSScriptRoot
}

if (-not (Test-Path -LiteralPath (Join-Path $source $ExeName))) {
    throw "no $ExeName in $source. Run this script from the unpacked archive, or pass -Zip <archive>."
}

if (-not $InstallDir) { $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\$AppName" }
$InstallDir = [System.IO.Path]::GetFullPath($InstallDir)
if ($InstallDir.TrimEnd('\') -eq ([System.IO.Path]::GetFullPath($source)).TrimEnd('\')) {
    throw "the source folder is the install folder; unpack the archive somewhere else first"
}

# The version the archive was packed as: the channel file beside the executable is the one
# piece of the archive that names it, so it is read rather than guessed at.
$version = 'unknown'
$channelFile = Join-Path $source 'update-channel.json'
if (Test-Path -LiteralPath $channelFile) {
    try {
        $channel = Get-Content -Raw -Encoding UTF8 -LiteralPath $channelFile | ConvertFrom-Json
        $named = $channel.PSObject.Properties['version']
        if ($named -and $named.Value) { $version = [string]$named.Value }
    } catch {
        Say "  (the channel file could not be read; the version is recorded as unknown)"
    }
}

Say "installing rubrica $version to $InstallDir"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$files = @()
foreach ($name in (Get-ExpectedFiles)) {
    $from = Join-Path $source $name
    if (-not (Test-Path -LiteralPath $from)) {
        if ($name -eq $ExeName) { throw "the archive has no $ExeName" }
        Say "  (no $name to install)"
        continue
    }
    Copy-Item -LiteralPath $from -Destination (Join-Path $InstallDir $name) -Force
    $files += $name
}
Say ("  files: {0}" -f ($files -join ', '))

$exe = Join-Path $InstallDir $ExeName

# The shortcut is a shell link rather than a copy of the program, so upgrading replaces the
# executable behind it and the shortcut keeps pointing at the program rather than at a
# version of it.
$shortcutMade = $false
if (-not $NoShortcut) {
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($ShortcutPath)
    $link.TargetPath = $exe
    $link.WorkingDirectory = $InstallDir
    $link.IconLocation = "$exe,0"
    $link.Description = 'A Markdown reader built around a typesetting engine'
    $link.Save()
    $shortcutMade = $true
    Say "  shortcut: $ShortcutPath"
}

# PATH is a user value that other programs also write, so it is edited as a string and put
# back with the kind it had. A PATH that already names the folder is left alone, which is
# what makes running this twice harmless. The key is opened for writing: the provider hands
# out a read-only handle, and `%VAR%` references must survive the edit unexpanded, which is
# the one thing `Get-ItemProperty` cannot promise.
$pathAdded = $false
$pathKind = 'ExpandString'
if (-not $NoPath) {
    $environment = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
    try {
        $names = @($environment.GetValueNames())
        $current = ''
        if ($names -contains 'Path') {
            $current = [string]$environment.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            if ($environment.GetValueKind('Path') -eq [Microsoft.Win32.RegistryValueKind]::String) {
                $pathKind = 'String'
            }
        }
        $segments = @($current -split ';' | Where-Object { $_ -and $_.Trim().Length -gt 0 })
        $there = $false
        foreach ($segment in $segments) {
            if ($segment.Trim().TrimEnd('\') -ieq $InstallDir.TrimEnd('\')) { $there = $true }
        }
        if ($there) {
            Say '  PATH: already names the install folder'
        } else {
            # Appended to the string rather than rebuilt from its segments, so that every
            # character someone else put in PATH survives the edit. A PATH that ends in a
            # separator keeps it: the new entry goes before the trailing separator, which is
            # what lets the uninstaller put the string back exactly as it found it.
            if ($current.Length -eq 0) {
                $joined = $InstallDir
            } elseif ($current.EndsWith(';')) {
                $joined = $current + $InstallDir + ';'
            } else {
                $joined = $current + ';' + $InstallDir
            }
            $valueKind = [Microsoft.Win32.RegistryValueKind]::ExpandString
            if ($pathKind -eq 'String') { $valueKind = [Microsoft.Win32.RegistryValueKind]::String }
            $environment.SetValue('Path', $joined, $valueKind)
            $pathAdded = $true
            Say "  PATH: added $InstallDir"
        }
    } finally {
        $environment.Close()
    }
}

# App Paths is what lets Win+R, the Run box and `start rubrica-app` find the program by name
# without a PATH at all. It is one key under the user's own hive.
New-Item -Path $AppPathKey -Force | Out-Null
Set-ItemProperty -LiteralPath $AppPathKey -Name '(default)' -Value $exe
Set-ItemProperty -LiteralPath $AppPathKey -Name 'Path' -Value $InstallDir
Say "  app paths: $AppPathKey"

# Explorer holds a copy of the environment from when it started, so it is told the PATH
# changed. A failure here costs nothing but a logoff before PATH is visible to new windows.
try {
    if (-not ('Rubrica.Shell32' -as [type])) {
        Add-Type -Namespace Rubrica -Name Shell32 -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Auto)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, UIntPtr wParam, string lParam, uint fuFlags, uint uTimeout, out UIntPtr lpdwResult);
'@
    }
    $result = [UIntPtr]::Zero
    [Rubrica.Shell32]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result) | Out-Null
} catch {
    Say '  (the change will be visible to new windows after the next sign-in)'
}

# What was created, written down where the uninstaller will find it. Nothing here is a guess
# at what an installer usually does: it is the list of what this run actually did.
$manifestPath = Join-Path $InstallDir 'install.json'
# An install over an older one inherits what the older one created. The PATH entry is added
# once and must be removed once, so a second run that finds it already there must not record
# that there is nothing of Rubrica's in PATH.
$inheritedPathAdded = $false
if (Test-Path -LiteralPath $manifestPath) {
    try {
        $previous = Get-Content -Raw -Encoding UTF8 -LiteralPath $manifestPath | ConvertFrom-Json
        $flag = $previous.PSObject.Properties['pathAdded']
        if ($flag -and $flag.Value -eq $true) { $inheritedPathAdded = $true }
    } catch {
        Say '  (the previous install.json could not be read; taking nothing from it)'
    }
}
$manifest = [ordered]@{
    name         = $AppName
    version      = $version
    installedAt  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    installDir   = $InstallDir
    executable   = $exe
    files        = @($files)
    shortcut     = $(if ($shortcutMade) { $ShortcutPath } else { $null })
    pathAdded    = ($pathAdded -or $inheritedPathAdded)
    pathKind     = $pathKind
    appPathKey   = $AppPathKey
    settingsKey  = $SettingsKey
    archive      = $(if ($Zip) { $Zip } else { $null })
    archiveHash  = $(if ($Zip) { (Get-FileHash -LiteralPath $Zip -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null })
}
$json = ($manifest | ConvertTo-Json -Depth 4) + "`n"
[System.IO.File]::WriteAllText($manifestPath, $json, [System.Text.UTF8Encoding]::new($false))
Say "  manifest: $manifestPath"

if ($work) { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }

Say ''
Say "Rubrica $version is installed."
Say "  Run it from the Start Menu, or: `"$exe`""
Say "  Uninstall with: powershell -ExecutionPolicy Bypass -File `"$(Join-Path $InstallDir 'uninstall.ps1')`""
