# Windows: dist\instaoptics-<version>-windows-x64.msi and -portable.zip. Needs the WiX 5 tool:
#   dotnet tool install --global wix --version 5.0.2; wix extension add -g WixToolset.UI.wixext/5.0.2
param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path "$PSScriptRoot\..\.."
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
$dist = "$root\dist"
New-Item -ItemType Directory -Force $dist | Out-Null
if (-not $SkipBuild) { cargo build --release --locked -p instaoptics -p instaoptics-cli }
$bin = "$root\target\release"
$name = "instaoptics-$version-windows-x64"
# WixUI_Minimal shows the licence; WiX wants it as RTF.
$rtf = "$root\target\LICENSE.rtf"
$text = (Get-Content "$root\LICENSE" -Raw) -replace '\\', '\\\\' -replace '\{', '\{' -replace '\}', '\}' -replace "`r?`n", '\par '
Set-Content $rtf "{\rtf1\ansi\deff0{\fonttbl{\f0 Segoe UI;}}\fs18 $text}"
wix build "$PSScriptRoot\instaoptics.wxs" -ext WixToolset.UI.wixext -arch x64 `
  -d Version=$version -d BinDir=$bin -d IconPath="$root\assets\icon.ico" -d LicenseRtf=$rtf -o "$dist\$name.msi"
Write-Host "wrote $dist\$name.msi"
$portable = "$root\target\$name-portable"
New-Item -ItemType Directory -Force $portable | Out-Null
Copy-Item "$bin\instaoptics.exe", "$bin\instaoptics-cli.exe", "$root\README.md", "$root\LICENSE" $portable
Compress-Archive -Force -Path $portable -DestinationPath "$dist\$name-portable.zip"
Write-Host "wrote $dist\$name-portable.zip"
