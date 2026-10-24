# Para o servidor sem que o iniciar.ps1 suba de novo. Para voltar: Start-ScheduledTask 'Enshrouded Servidor'
$Raiz = Split-Path -Parent $MyInvocation.MyCommand.Path
New-Item -ItemType File -Force (Join-Path $Raiz "PARAR") | Out-Null
Get-Process enshrouded_server -ErrorAction SilentlyContinue | Stop-Process
Write-Host "servidor parado"
