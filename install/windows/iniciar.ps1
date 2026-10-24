# Mantém o servidor rodando: se ele fechar, espera 30 s e sobe de novo. Para parar de vez, use parar.ps1.
$Raiz = Split-Path -Parent $MyInvocation.MyCommand.Path
$Servidor = Join-Path $Raiz "servidor"
$Flag = Join-Path $Raiz "PARAR"
$Log = Join-Path $Raiz "iniciar.log"
Remove-Item $Flag -ErrorAction SilentlyContinue
while (-not (Test-Path $Flag)) {
    Add-Content $Log "$(Get-Date -Format s) iniciando o servidor"
    $p = Start-Process (Join-Path $Servidor "enshrouded_server.exe") -WorkingDirectory $Servidor -PassThru -WindowStyle Hidden
    $p.WaitForExit()
    Add-Content $Log "$(Get-Date -Format s) servidor fechou (código $($p.ExitCode))"
    if (-not (Test-Path $Flag)) { Start-Sleep -Seconds 30 }
}
Add-Content $Log "$(Get-Date -Format s) parado pelo parar.ps1"
