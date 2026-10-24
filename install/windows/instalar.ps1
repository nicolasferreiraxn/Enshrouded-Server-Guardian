# Instala o servidor do Enshrouded com o guardião numa máquina Windows nova.
# Rodar como administrador:  powershell -ExecutionPolicy Bypass -File instalar.ps1
# Opções: -Raiz C:\Enshrouded   -Sobrescrever (substitui um savegame que já exista)
param(
    [string]$Raiz = "C:\Enshrouded",
    [switch]$Sobrescrever
)
$ErrorActionPreference = "Stop"
$Aqui = Split-Path -Parent $MyInvocation.MyCommand.Path
$Arquivos = Join-Path $Aqui "arquivos"
$Servidor = Join-Path $Raiz "servidor"
$SteamCmd = Join-Path $Raiz "steamcmd"
$ShaEsperado = "b627639ce4a3f8c940fe90afa1d36ed59db3ff9ced24e935890eb7c99b04d5bf"

function Passo($m) { Write-Host "`n== $m" -ForegroundColor Cyan }

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { throw "Rode este script como administrador." }
foreach ($f in "world0.ksd", "enshrouded_server.json", "dbghelp.dll", "guardiao.json") {
    if (-not (Test-Path (Join-Path $Arquivos $f))) { throw "Falta arquivos\$f" }
}
New-Item -ItemType Directory -Force $Raiz, $Servidor, $SteamCmd, (Join-Path $Raiz "backups") | Out-Null

Passo "Visual C++ 2022 x64"
$vc = Join-Path $env:TEMP "vc_redist.x64.exe"
Invoke-WebRequest "https://aka.ms/vs/17/release/vc_redist.x64.exe" -OutFile $vc
Start-Process $vc -ArgumentList "/install", "/quiet", "/norestart" -Wait

Passo "SteamCMD"
if (-not (Test-Path (Join-Path $SteamCmd "steamcmd.exe"))) {
    $zip = Join-Path $env:TEMP "steamcmd.zip"
    Invoke-WebRequest "https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip" -OutFile $zip
    Expand-Archive $zip -DestinationPath $SteamCmd -Force
}

Passo "Servidor dedicado (app 2278520) - pode levar alguns minutos"
$exe = Join-Path $Servidor "enshrouded_server.exe"
# no primeiro uso o SteamCMD costuma falhar com "Missing configuration"; tentar de novo resolve
for ($i = 1; $i -le 3 -and -not (Test-Path $exe); $i++) {
    if ($i -gt 1) { Write-Host "tentativa $i"; Start-Sleep -Seconds 5 }
    & (Join-Path $SteamCmd "steamcmd.exe") +@sSteamCmdForcePlatformType windows +force_install_dir $Servidor +login anonymous +app_update 2278520 validate +quit
}
if (-not (Test-Path $exe)) { throw "O SteamCMD não instalou o servidor." }

Passo "Conferindo o build"
$sha = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLower()
if ($sha -ne $ShaEsperado) {
    Write-Warning "O executável ($sha) não é o build analisado. O servidor vai funcionar, mas o guardião vai se desligar sozinho até ser atualizado."
} else { Write-Host "build conferido: $sha" }

Passo "Mundo, config e guardião"
$save = Join-Path $Servidor "savegame"
if ((Test-Path (Join-Path $save "world0.ksd")) -and -not $Sobrescrever) {
    throw "Já existe savegame\world0.ksd. Use -Sobrescrever para trocar pelo mundo do pacote."
}
New-Item -ItemType Directory -Force $save | Out-Null
Copy-Item (Join-Path $Arquivos "world0.ksd") $save -Force
Copy-Item (Join-Path $Arquivos "enshrouded_server.json"), (Join-Path $Arquivos "dbghelp.dll"), (Join-Path $Arquivos "guardiao.json") $Servidor -Force
Copy-Item (Join-Path $Aqui "iniciar.ps1"), (Join-Path $Aqui "parar.ps1"), (Join-Path $Aqui "backup.ps1"), (Join-Path $Aqui "console.ps1") $Raiz -Force
# atalho na Área de Trabalho para acompanhar os logs (o servidor roda sem janela, como tarefa do SYSTEM)
$atalho = (New-Object -ComObject WScript.Shell).CreateShortcut((Join-Path ([Environment]::GetFolderPath("Desktop")) "Console do Servidor.lnk"))
$atalho.TargetPath = "powershell.exe"
$atalho.Arguments = "-NoExit -ExecutionPolicy Bypass -File `"$Raiz\console.ps1`""
$atalho.WorkingDirectory = $Raiz
$atalho.Save()

Passo "Firewall"
$porta = (Get-Content (Join-Path $Servidor "enshrouded_server.json") -Raw | ConvertFrom-Json).queryPort
Get-NetFirewallRule -DisplayName "Enshrouded (UDP $porta)" -ErrorAction SilentlyContinue | Remove-NetFirewallRule
New-NetFirewallRule -DisplayName "Enshrouded (UDP $porta)" -Direction Inbound -Protocol UDP -LocalPort $porta -Action Allow | Out-Null
Write-Host "liberada a porta UDP $porta (libere também no painel do provedor)"

Passo "Tarefas agendadas"
$ps = "powershell.exe"
$sistema = New-ScheduledTaskPrincipal -UserId "SYSTEM" -LogonType ServiceAccount -RunLevel Highest
$cfg = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
Register-ScheduledTask -TaskName "Enshrouded Servidor" -Force -Principal $sistema -Settings $cfg `
    -Trigger (New-ScheduledTaskTrigger -AtStartup) `
    -Action (New-ScheduledTaskAction -Execute $ps -Argument "-ExecutionPolicy Bypass -File `"$Raiz\iniciar.ps1`"") | Out-Null
Register-ScheduledTask -TaskName "Enshrouded Backup" -Force -Principal $sistema `
    -Trigger (New-ScheduledTaskTrigger -Daily -At 5am) `
    -Action (New-ScheduledTaskAction -Execute $ps -Argument "-ExecutionPolicy Bypass -File `"$Raiz\backup.ps1`"") | Out-Null

Passo "Pronto"
Write-Host "Servidor em $Servidor. Para ligar agora:  Start-ScheduledTask 'Enshrouded Servidor'"
Write-Host "Log do guardião: $Servidor\guardiao-dados\guardiao.log  (modo inicial: observar)"
