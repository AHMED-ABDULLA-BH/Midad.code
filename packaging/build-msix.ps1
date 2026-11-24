[CmdletBinding(DefaultParameterSetName='Store')]
param(
    [Parameter(Mandatory, ParameterSetName='Test')][switch]$LocalTest,
    [Parameter(Mandatory, ParameterSetName='Store')][string]$IdentityName,
    [Parameter(Mandatory, ParameterSetName='Store')][string]$Publisher,
    [Parameter(Mandatory, ParameterSetName='Store')][string]$PublisherDisplayName,
    [Parameter(Mandatory, ParameterSetName='Store')][string]$DisplayName
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot
$sdk = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\makeappx.exe" |
    Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1
if (!$sdk) { throw 'Install the Windows 10 or 11 SDK with MakeAppx.' }
$stage = Join-Path $repo ("target/msix-" + [guid]::NewGuid())
New-Item -ItemType Directory $stage | Out-Null
try {
    [xml]$manifest = Get-Content "$PSScriptRoot/msix/AppxManifest.xml" -Raw -Encoding UTF8
    $flavor = 'LocalTest'
    if (!$LocalTest) {
        $flavor = 'Store'
        $manifest.Package.Identity.Name = $IdentityName
        $manifest.Package.Identity.Publisher = $Publisher
        $manifest.Package.Properties.PublisherDisplayName = $PublisherDisplayName
        $manifest.Package.Properties.DisplayName = $DisplayName
        $manifest.Package.Applications.Application.VisualElements.DisplayName = $DisplayName
    }
    $manifest.Save("$stage/AppxManifest.xml")
    Copy-Item "$repo/target/release/midad.exe","$PSScriptRoot/THIRD-PARTY.txt" $stage
    Copy-Item "$PSScriptRoot/msix/Assets" $stage -Recurse
    Copy-Item "$repo/assets/OFL.txt" "$stage/Amiri-OFL.txt"
    foreach ($face in 'arefruqaa','reemkufi','notonastaliqurdu') {
        Copy-Item "$repo/assets/$face/OFL.txt" "$stage/$face-OFL.txt"
    }
    $package = "$repo/target/Midad-$($manifest.Package.Identity.Version)-x64-$flavor.msix"
    & $sdk.FullName pack /o /d $stage /p "$stage.msix"
    if ($LASTEXITCODE) { throw "MakeAppx failed ($LASTEXITCODE)" }
    Move-Item "$stage.msix" $package -Force
    Write-Host "Created $package"
} finally {
    Remove-Item $stage -Recurse -Force
    Remove-Item "$stage.msix" -Force -ErrorAction SilentlyContinue
}
