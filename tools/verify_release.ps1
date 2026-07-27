<#
.SYNOPSIS
  Gates a release build before it is published.

.DESCRIPTION
  Checks the things that silently go wrong and would otherwise ship: a binary
  whose version resource does not match the tag, and a binary with no embedded
  icon because the resource compiler was missing at build time.

  build.rs only warns when it cannot embed the icon, so the build stays green
  and the mistake reaches users. This script turns that warning into a failure.

.PARAMETER Path
  Path to the built executable.

.PARAMETER ExpectedVersion
  Version the binary must declare, without a leading v. Optional.

.EXAMPLE
  pwsh tools/verify_release.ps1 -Path dist/Ferrite.exe -ExpectedVersion 1.0.0
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Path,

    [string]$ExpectedVersion
)

$ErrorActionPreference = 'Stop'
$problems = @()

if (-not (Test-Path $Path)) {
    Write-Host "FAIL  executable not found: $Path"
    exit 1
}

$exe = (Resolve-Path $Path).Path
$item = Get-Item $exe
$info = $item.VersionInfo

Write-Host "Verifying $($item.Name), $([math]::Round($item.Length / 1MB, 2)) MB"
Write-Host ""

# --- Version resource -------------------------------------------------------

$expectedFields = @{
    ProductName      = 'Ferrite'
    OriginalFilename = 'Ferrite.exe'
    InternalName     = 'Ferrite'
}

foreach ($field in $expectedFields.Keys) {
    $actual = $info.$field
    if ($actual -ne $expectedFields[$field]) {
        $problems += "$field is '$actual', expected '$($expectedFields[$field])'"
    } else {
        Write-Host ("  OK    {0,-16} {1}" -f $field, $actual)
    }
}

foreach ($field in @('FileDescription', 'CompanyName', 'LegalCopyright')) {
    if ([string]::IsNullOrWhiteSpace($info.$field)) {
        $problems += "$field is empty"
    } else {
        Write-Host ("  OK    {0,-16} {1}" -f $field, $info.$field)
    }
}

if ($ExpectedVersion) {
    $wanted = $ExpectedVersion.TrimStart('v')
    foreach ($field in @('FileVersion', 'ProductVersion')) {
        # The resource pads to four components, so compare on the prefix.
        $actual = ($info.$field -replace '\s', '')
        if (-not $actual.StartsWith($wanted)) {
            $problems += "$field is '$actual', expected to start with '$wanted'"
        } else {
            Write-Host ("  OK    {0,-16} {1}" -f $field, $actual)
        }
    }
} else {
    Write-Host ("  SKIP  version check, no expected version given")
}

# --- Embedded icon ----------------------------------------------------------

# ExtractAssociatedIcon falls back to a generic shell icon, so it cannot tell
# an embedded icon from a missing one. ExtractIconEx counts actual icon
# resources in the file, which is the question being asked here.
Add-Type -Namespace Native -Name Shell -MemberDefinition @'
[DllImport("shell32.dll", CharSet = CharSet.Unicode)]
public static extern uint ExtractIconEx(string file, int index, IntPtr[] large, IntPtr[] small, uint count);
'@

$large = New-Object IntPtr[] 1
$small = New-Object IntPtr[] 1
$iconCount = [Native.Shell]::ExtractIconEx($exe, 0, $large, $small, 1)

if ($iconCount -lt 1) {
    $problems += "no icon resource embedded, the Windows SDK resource compiler was probably missing at build time"
} else {
    Write-Host ("  OK    {0,-16} {1} icon resource(s)" -f 'Icon', $iconCount)
}

# --- Result -----------------------------------------------------------------

Write-Host ""
if ($problems.Count -gt 0) {
    Write-Host "FAIL  release build rejected:"
    $problems | ForEach-Object { Write-Host "  - $_" }
    exit 1
}

$hash = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
Write-Host "OK    release build accepted"
Write-Host "SHA256 $hash"
exit 0
