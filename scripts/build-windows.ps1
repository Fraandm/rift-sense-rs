param(
  [switch]$Debug
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
  $profile = if ($Debug) { "dev" } else { "release" }
  $cargoArgs = @("build", "--bin", "riftsense", "--bin", "riftsense-desktop")
  if (-not $Debug) { $cargoArgs += "--release" }
  $ErrorActionPreference = "Continue"
  & cargo @cargoArgs
  $cargoExit = $LASTEXITCODE
  $ErrorActionPreference = "Stop"
  if ($cargoExit -ne 0) { throw "cargo build fallo" }

  $target = Join-Path $root "target\$profile"
  $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
  $registry = Join-Path $cargoHome "registry\src"
  $dll = Get-ChildItem $registry -Recurse -Filter "WebView2Loader.dll" -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\x64\\" } |
    Select-Object -First 1
  if (-not $dll) {
    throw "No se encontro WebView2Loader.dll (x64) en $registry"
  }
  Copy-Item $dll.FullName (Join-Path $target "WebView2Loader.dll") -Force
  if (Test-Path (Join-Path $root "models")) {
    Copy-Item (Join-Path $root "models") (Join-Path $target "models") -Recurse -Force
  }
  Write-Host "Binarios en $target"
  Get-ChildItem $target -Filter "riftsense*.exe" | Select-Object Name, Length
} finally {
  Pop-Location
}
