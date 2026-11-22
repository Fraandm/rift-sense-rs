param(
  [Parameter(Mandatory = $true)]
  [string]$Installer,
  [string]$ExpectedSha256 = "",
  [int]$Port = 8399,
  [switch]$RequireSignature,
  [switch]$AllowExistingInstall
)

$ErrorActionPreference = "Stop"
$installer = (Resolve-Path $Installer).Path
if (-not (Test-Path $installer)) { throw "No existe el instalador: $installer" }

$mainInstall = Join-Path $env:LOCALAPPDATA "Programs\RiftSense"
if ((Test-Path (Join-Path $mainInstall "riftsense.exe")) -and (-not $AllowExistingInstall)) {
  throw "Hay una instalación real en $mainInstall. El instalador comparte AppId y la desinstalación de la copia de prueba puede borrar los accesos directos reales. Valida en una VM/Windows Sandbox o usa -AllowExistingInstall bajo tu responsabilidad."
}

$hash = (Get-FileHash $installer -Algorithm SHA256).Hash.ToLower()
if ($ExpectedSha256 -and $hash -ne $ExpectedSha256.ToLower()) {
  throw "SHA-256 no coincide. Esperado: $ExpectedSha256. Obtenido: $hash"
}

$signature = Get-AuthenticodeSignature $installer
$signed = $signature.Status -eq "Valid"
if ($RequireSignature -and -not $signed) {
  throw "El instalador no tiene una firma Authenticode válida: $($signature.Status)"
}
if (-not $signed) {
  Write-Warning "Instalador sin firma válida ($($signature.Status))."
}

$installDir = Join-Path $env:TEMP "RiftSenseValidate-$PID"
$process = $null
try {
  if (Test-Path $installDir) { Remove-Item $installDir -Recurse -Force }
  $installerProcess = Start-Process -FilePath $installer -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/DIR=$installDir", "/NOICONS" -PassThru
  $installerProcess.WaitForExit()
  Start-Sleep -Seconds 2
  Get-Process riftsense-desktop -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -like "$installDir*" } |
    Stop-Process -Force -ErrorAction SilentlyContinue
  $exe = Join-Path $installDir "riftsense.exe"
  $desktop = Join-Path $installDir "riftsense-desktop.exe"
  if (-not (Test-Path $exe) -or -not (Test-Path $desktop)) {
    throw "La instalación no contiene los ejecutables esperados."
  }
  $version = (Get-Item $exe).VersionInfo.FileVersion

  $process = Start-Process -FilePath $exe -ArgumentList "--port", "$Port" -PassThru -WindowStyle Hidden
  $health = $null
  for ($attempt = 0; $attempt -lt 60; $attempt++) {
    try {
      $health = Invoke-RestMethod "http://127.0.0.1:$Port/api/health" -TimeoutSec 3
      break
    } catch {
      Start-Sleep -Milliseconds 500
    }
  }
  if (-not $health) { throw "El servidor no respondió en el puerto $Port." }

  [pscustomobject]@{
    Installer = $installer
    Sha256 = $hash
    Signature = $signature.Status.ToString()
    Version = $version
    Patch = $health.patch
    Client = (Invoke-RestMethod "http://127.0.0.1:$Port/api/client/status" -TimeoutSec 20).connected
    Installed = $true
    Health = $health.status
  } | Format-List
} finally {
  if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
  Get-Process riftsense-desktop -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -like "$installDir*" } |
    Stop-Process -Force -ErrorAction SilentlyContinue
  $uninstaller = Join-Path $installDir "unins000.exe"
  if (Test-Path $uninstaller) {
    Start-Process -FilePath $uninstaller -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART" -Wait -ErrorAction SilentlyContinue
  }
  if (Test-Path $installDir) { Remove-Item $installDir -Recurse -Force -ErrorAction SilentlyContinue }
}
