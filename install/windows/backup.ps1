# Backup diário: mundo, cópias do guardião e configs num .zip em backups\. Mantém os 14 mais recentes.
$Raiz = Split-Path -Parent $MyInvocation.MyCommand.Path
$Servidor = Join-Path $Raiz "servidor"
$Destino = Join-Path $Raiz "backups"
New-Item -ItemType Directory -Force $Destino | Out-Null
$itens = @("savegame", "guardiao-dados", "enshrouded_server.json", "guardiao.json") |
    ForEach-Object { Join-Path $Servidor $_ } | Where-Object { Test-Path $_ }
$zip = Join-Path $Destino ("{0}.zip" -f (Get-Date -Format "yyyyMMdd_HHmm"))
Compress-Archive -Path $itens -DestinationPath $zip -Force
Get-ChildItem $Destino -Filter *.zip | Sort-Object LastWriteTime -Descending | Select-Object -Skip 14 | Remove-Item
