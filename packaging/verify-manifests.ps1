<#
.SYNOPSIS
    Checks the winget and scoop manifests against the version and licence in Cargo.toml.

.DESCRIPTION
    The manifests are hand-written files that point at a release. They can be wrong in ways
    that only show up when someone submits them: a stale version, an archive name that the
    packaging script does not produce, a package identifier that disagrees between the three
    winget files. This checks those things against the one place the version lives, and says
    plainly which value still has to be filled in by the release job.

    It does not check the hash against an archive, because the archive is built by the
    release job and its hash does not exist yet. A placeholder is reported as pending, not as
    a failure.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File packaging\verify-manifests.ps1
#>
[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$wingetDir = Join-Path $root 'packaging\winget'
$scoopPath = Join-Path $root 'packaging\scoop\rubrica.json'
$versionFile = Join-Path $wingetDir 'Rubrica.Rubrica.yaml'
$installerFile = Join-Path $wingetDir 'Rubrica.Rubrica.installer.yaml'
$localeFile = Join-Path $wingetDir 'Rubrica.Rubrica.locale.en-US.yaml'
$placeholder = 'REPLACE_WITH_RELEASE_SHA256'

$problems = 0
$pending = @()

function Read-Scalar([string]$path, [string]$name) {
    $text = Get-Content -Raw -Encoding UTF8 -LiteralPath $path
    # Indented too: the installer manifest puts its URL and hash inside the installers list,
    # and that is still the field this asks about.
    if ($text -match "(?m)^[ \t]*$name[ \t]*:[ \t]*(.+?)[ \t]*$") { return $Matches[1] }
    return $null
}

function Check([string]$what, [bool]$ok, [string]$detail) {
    if ($ok) {
        Write-Host ("  {0,-52} ok    {1}" -f $what, $detail)
    } else {
        Write-Host ("  {0,-52} FAIL  {1}" -f $what, $detail)
        $script:problems++
    }
}

function Check-Pending([string]$what, [string]$value, [string]$detail) {
    if ($value -eq $placeholder) {
        Write-Host ("  {0,-52} todo  {1}" -f $what, $detail)
        $script:pending += $what
        return
    }
    if ($value -match '^[0-9a-f]{64}$') {
        Write-Host ("  {0,-52} ok    {1}" -f $what, $value)
    } else {
        Write-Host ("  {0,-52} FAIL  {1}" -f $what, "$value is not a lower-case sha256 and not the placeholder")
        $script:problems++
    }
}

$cargo = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $root 'Cargo.toml')
$body = ($cargo -split '\[workspace\.package\]', 2)[1]
if (-not ($body -match 'version\s*=\s*"([^"]+)"')) { throw 'Cargo.toml has no [workspace.package] version' }
$version = $Matches[1]
if (-not ($body -match 'license\s*=\s*"([^"]+)"')) { throw 'Cargo.toml has no [workspace.package] license' }
$licence = $Matches[1]
if (-not ($body -match 'repository\s*=\s*"([^"]+)"')) { throw 'Cargo.toml has no [workspace.package] repository' }
$repository = $Matches[1]
$zipName = "rubrica-v$version-win-x64.zip"

Write-Host "manifests for rubrica $version"
Write-Host ''

Check 'cargo version is a version' ($version -match '^\d+\.\d+\.\d+$') $version
Check 'archive name has no spaces' ($zipName -notmatch '\s') $zipName

$missing = @()
foreach ($path in @($versionFile, $installerFile, $localeFile, $scoopPath)) {
    if (-not (Test-Path -LiteralPath $path)) { $missing += $path }
}
Check 'all four manifest files are there' ($missing.Count -eq 0) ($(if ($missing.Count -eq 0) { 'yes' } else { $missing -join ', ' }))
if ($missing.Count -gt 0) { exit 1 }

# winget: three files, one package, one version.
$ids = @(
    (Read-Scalar $versionFile 'PackageIdentifier'),
    (Read-Scalar $installerFile 'PackageIdentifier'),
    (Read-Scalar $localeFile 'PackageIdentifier')
)
$distinct = @($ids | Sort-Object -Unique)
Check 'the three winget files name one package' ($distinct.Count -eq 1) ($distinct -join ', ')
Check 'package identifier is publisher.name' ($ids[0] -match '^[A-Za-z0-9]+(\.[A-Za-z0-9\-]+)+$') $ids[0]

foreach ($pair in @(
        @('winget version manifest', $versionFile),
        @('winget installer manifest', $installerFile),
        @('winget locale manifest', $localeFile))) {
    $named = Read-Scalar $pair[1] 'PackageVersion'
    Check "$($pair[0]) carries $version" ($named -eq $version) ([string]$named)
}

Check 'version manifest says ManifestType: version' ((Read-Scalar $versionFile 'ManifestType') -eq 'version') (Read-Scalar $versionFile 'ManifestType')
Check 'installer manifest says ManifestType: installer' ((Read-Scalar $installerFile 'ManifestType') -eq 'installer') (Read-Scalar $installerFile 'ManifestType')
Check 'locale manifest says ManifestType: defaultLocale' ((Read-Scalar $localeFile 'ManifestType') -eq 'defaultLocale') (Read-Scalar $localeFile 'ManifestType')

$manifestVersion = Read-Scalar $versionFile 'ManifestVersion'
Check 'all three agree on ManifestVersion' (
    $manifestVersion -and
    (Read-Scalar $installerFile 'ManifestVersion') -eq $manifestVersion -and
    (Read-Scalar $localeFile 'ManifestVersion') -eq $manifestVersion
) $manifestVersion

Check 'installer type is a zip of a portable program' (
    (Read-Scalar $installerFile 'InstallerType') -eq 'zip' -and
    (Read-Scalar $installerFile 'NestedInstallerType') -eq 'portable'
) ("{0} / {1}" -f (Read-Scalar $installerFile 'InstallerType'), (Read-Scalar $installerFile 'NestedInstallerType'))

$installerUrl = Read-Scalar $installerFile 'InstallerUrl'
$expectedUrl = "$repository/releases/download/v$version/$zipName"
Check 'installer url is this release archive' ($installerUrl -ceq $expectedUrl) $installerUrl
Check-Pending 'installer sha256' (Read-Scalar $installerFile 'InstallerSha256') 'filled in from the archive the release job uploads'

$releaseDate = Read-Scalar $installerFile 'ReleaseDate'
Check 'release date is a date' ($releaseDate -match '^\d{4}-\d{2}-\d{2}$') $releaseDate

$licenceNamed = Read-Scalar $localeFile 'License'
Check 'winget licence matches Cargo.toml' ($licenceNamed -ceq $licence) ("{0} vs {1}" -f $licenceNamed, $licence)
Check 'winget locale names a publisher and a description' (
    (Read-Scalar $localeFile 'Publisher') -and (Read-Scalar $localeFile 'ShortDescription')
) ("{0}: {1}" -f (Read-Scalar $localeFile 'Publisher'), (Read-Scalar $localeFile 'ShortDescription'))

$scoop = Get-Content -Raw -Encoding UTF8 -LiteralPath $scoopPath | ConvertFrom-Json
Check 'scoop version is the cargo version' ($scoop.version -eq $version) ([string]$scoop.version)
Check 'scoop archive is this release archive' ($scoop.architecture.'64bit'.url -ceq $expectedUrl) $scoop.architecture.'64bit'.url
Check-Pending 'scoop hash' $scoop.architecture.'64bit'.hash 'filled in from the archive the release job uploads'
Check 'scoop licence matches Cargo.toml' ($scoop.license -ceq $licence) ("{0} vs {1}" -f $scoop.license, $licence)
Check 'scoop installs the program and a shortcut' (
    $scoop.bin -eq 'rubrica-app.exe' -and $scoop.shortcuts.Count -eq 1 -and $scoop.shortcuts[0][0] -eq 'rubrica-app.exe'
) ("bin {0}, shortcut {1}" -f $scoop.bin, $scoop.shortcuts[0][1])
Check 'scoop can find the next release itself' (
    $scoop.checkver.github -ceq $repository -and
    $scoop.autoupdate.architecture.'64bit'.url -ceq "$repository/releases/download/v`$version/rubrica-v`$version-win-x64.zip"
) $scoop.checkver.github

Write-Host ''
if ($pending.Count -gt 0) {
    Write-Host ("{0} value(s) still to be filled in after the release job runs: {1}" -f $pending.Count, ($pending -join ', '))
    Write-Host '  The archive hash is printed by packaging\package.ps1 and by the release workflow.'
    Write-Host '  Both manifests are submitted only after a tag is built, never before: the hash of'
    Write-Host '  a build made on this machine is not the hash of the one the release uploads.'
}
if ($problems -ne 0) {
    Write-Host ("{0} check(s) failed" -f $problems)
    exit 1
}
Write-Host 'the manifests agree with Cargo.toml and with what packaging\package.ps1 produces'
