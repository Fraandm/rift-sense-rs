param(
  [string]$Version = "",
  [string]$DataRoot = "",
  [string]$SignCertificate = $env:RIFTSENSE_SIGN_CERT,
  [string]$SignPassword = $env:RIFTSENSE_SIGN_PASSWORD,
  [string]$TimestampUrl = "http://timestamp.digicert.com",
  [string]$WebView2Bootstrapper = ""
)

function Find-SignTool {
  $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($command) { return $command.Source }
  $kits = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
  if (Test-Path $kits) {
    return Get-ChildItem $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
      Sort-Object FullName -Descending |
      Select-Object -First 1 -ExpandProperty FullName
  }
  return $null
}

function Sign-RiftSenseFile([string]$Path) {
  if (-not $SignCertificate) { return }
  $signtool = Find-SignTool
  if (-not $signtool) { throw "No se encontro signtool.exe. Instala Windows SDK o usa un certificado en el almacen." }
  $arguments = @("sign", "/fd", "SHA256", "/tr", $TimestampUrl, "/td", "SHA256")
  if ($SignCertificate -like "*.pfx") {
    $arguments += @("/f", $SignCertificate)
    if ($SignPassword) { $arguments += @("/p", $SignPassword) }
  } else {
    $arguments += @("/sha1", $SignCertificate)
  }
  $arguments += $Path
  & $signtool @arguments
  if ($LASTEXITCODE -ne 0) { throw "Fallo la firma de $Path" }
  Write-Host "Firmado: $Path" -ForegroundColor DarkGreen
}

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$iss = Join-Path $PSScriptRoot "RiftSense.iss"
$payload = Join-Path $PSScriptRoot "payload"
$dist = Join-Path $root "dist"

if (-not $Version) {
  $match = Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
  if (-not $match) { throw "No se pudo leer la version de Cargo.toml" }
  $Version = $match.Matches[0].Groups[1].Value
}

if (-not $DataRoot) {
  $DataRoot = $env:RIFTSENSE_ROOT
  if (-not $DataRoot) {
    $candidate = Join-Path (Split-Path -Parent $root) "Analizador LOL"
    if (Test-Path (Join-Path $candidate "data\processed\vocab.json")) {
      $DataRoot = $candidate
    }
  }
}
if (-not $DataRoot -or -not (Test-Path (Join-Path $DataRoot "data\processed\vocab.json"))) {
  throw "Falta la raiz de datos. Pasa -DataRoot o define RIFTSENSE_ROOT."
}

$cargo = (Get-Command cargo.exe -ErrorAction SilentlyContinue).Source
if (-not $cargo -and $env:CARGO_HOME) {
  $candidate = Join-Path $env:CARGO_HOME "bin\cargo.exe"
  if (Test-Path $candidate) { $cargo = $candidate }
}
if (-not $cargo) { throw "No se encontro cargo.exe" }

$iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) {
  foreach ($candidate in @(
    (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"),
    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 6\ISCC.exe"
  )) {
    if (Test-Path $candidate) { $iscc = $candidate; break }
  }
}
if (-not $iscc) { throw "No se encontro ISCC.exe (Inno Setup 6)" }

Write-Host "Compilando RiftSense $Version..." -ForegroundColor Cyan
Push-Location $root
try {
  $previous = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  & $cargo build --release --bin riftsense --bin riftsense-desktop
  $cargoExit = $LASTEXITCODE
  $ErrorActionPreference = $previous
  if ($cargoExit -ne 0) { throw "cargo build fallo" }
} finally {
  Pop-Location
}

$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
$target = Join-Path $targetRoot "release"
foreach ($file in @("riftsense.exe", "riftsense-desktop.exe", "WebView2Loader.dll")) {
  if (-not (Test-Path (Join-Path $target $file))) { throw "Falta $target\$file" }
}

if (Test-Path $payload) { Remove-Item $payload -Recurse -Force }
New-Item -ItemType Directory -Path (Join-Path $payload "models") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $payload "data\processed") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $payload "artifacts") -Force | Out-Null

Copy-Item (Join-Path $target "riftsense.exe") $payload -Force
Copy-Item (Join-Path $target "riftsense-desktop.exe") $payload -Force
Copy-Item (Join-Path $target "WebView2Loader.dll") $payload -Force
Copy-Item (Join-Path $root "assets\RiftSense.ico") $payload -Force
if ($WebView2Bootstrapper) {
  if (-not (Test-Path $WebView2Bootstrapper)) { throw "No existe el bootstrapper de WebView2: $WebView2Bootstrapper" }
  Copy-Item $WebView2Bootstrapper (Join-Path $payload "MicrosoftEdgeWebview2Setup.exe") -Force
}
if ($SignCertificate) {
  Sign-RiftSenseFile (Join-Path $payload "riftsense.exe")
  Sign-RiftSenseFile (Join-Path $payload "riftsense-desktop.exe")
} else {
  Write-Warning "Instalador sin firmar: define RIFTSENSE_SIGN_CERT o -SignCertificate para firmar."
}
Copy-Item (Join-Path $root "models\*") (Join-Path $payload "models") -Recurse -Force
Copy-Item (Join-Path $DataRoot "data\processed\*") (Join-Path $payload "data\processed") -Force
foreach ($file in @("meta.json", "draft_report.json", "build_report.json")) {
  $source = Join-Path $DataRoot "artifacts\$file"
  if (Test-Path $source) { Copy-Item $source (Join-Path $payload "artifacts") -Force }
}
Copy-Item (Join-Path $root "README.md") $payload -Force

if (-not (Test-Path $dist)) { New-Item -ItemType Directory -Path $dist -Force | Out-Null }
Write-Host "Compilando instalador con Inno Setup..." -ForegroundColor Cyan
$defines = @("/DMyAppVersion=$Version")
if (Test-Path (Join-Path $payload "MicrosoftEdgeWebview2Setup.exe")) {
  $defines += "/DWebView2Setup=1"
}
& $iscc @defines $iss
if ($LASTEXITCODE -ne 0) { throw "ISCC fallo" }

$setup = Join-Path $dist "RiftSense-Setup-$Version.exe"
if (-not (Test-Path $setup)) { throw "No se genero $setup" }
if ($SignCertificate) { Sign-RiftSenseFile $setup }
$hash = (Get-FileHash $setup -Algorithm SHA256).Hash.ToLower()
$hashFile = "$setup.sha256"
"$hash  $(Split-Path -Leaf $setup)" | Set-Content $hashFile -Encoding ascii
Write-Host ""
Write-Host "Instalador: $setup" -ForegroundColor Green
Write-Host "SHA256:     $hash"
Write-Host "Checksum:   $hashFile"
