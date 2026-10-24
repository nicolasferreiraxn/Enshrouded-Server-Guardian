# Console de acompanhamento: mostra ao vivo o log do servidor e o do guardião.
# Fechar a janela não para o servidor (ele roda como tarefa agendada). Ctrl+C sai.
$Raiz = Split-Path -Parent $MyInvocation.MyCommand.Path
$Logs = @(
    @{ Nome = "servidor"; Caminho = Join-Path $Raiz "servidor\logs\enshrouded_server.log"; Cor = "Gray" },
    @{ Nome = "guardiao"; Caminho = Join-Path $Raiz "servidor\guardiao-dados\guardiao.log"; Cor = "Cyan" },
    @{ Nome = "registro"; Caminho = Join-Path $Raiz "servidor\guardiao-dados\registro.jsonl"; Cor = "Yellow" },
    @{ Nome = "iniciar";  Caminho = Join-Path $Raiz "iniciar.log"; Cor = "Green" }
)
$Host.UI.RawUI.WindowTitle = "Enshrouded - console do servidor"
$pos = @{}
foreach ($l in $Logs) { $pos[$l.Nome] = -1 }

function Linhas($l) {
    # lê o que foi acrescentado desde a última volta; se o arquivo foi trocado ou encolheu, recomeça
    if (-not (Test-Path $l.Caminho)) { return }
    $fs = [IO.File]::Open($l.Caminho, 'Open', 'Read', 'ReadWrite, Delete')
    try {
        $tam = $fs.Length
        if ($pos[$l.Nome] -lt 0) { $pos[$l.Nome] = [Math]::Max(0, $tam - 4000) }
        if ($tam -lt $pos[$l.Nome]) { $pos[$l.Nome] = 0 }
        $fs.Position = $pos[$l.Nome]
        $sr = New-Object IO.StreamReader($fs, [Text.Encoding]::UTF8)
        $txt = $sr.ReadToEnd()
        $pos[$l.Nome] = $fs.Position
    } finally { $fs.Dispose() }
    foreach ($linha in ($txt -split "`r?`n")) {
        if ($linha -and $linha -notmatch '\[os\] Started thread') {
            $cor = if ($linha -match '^\[E |Error|FALHA|substituido') { "Red" } else { $l.Cor }
            Write-Host ("[{0}] {1}" -f $l.Nome, $linha) -ForegroundColor $cor
        }
    }
}

$proc = Get-Process enshrouded_server -ErrorAction SilentlyContinue
Write-Host ("servidor: " + $(if ($proc) { "rodando (pid $($proc.Id))" } else { "PARADO" })) -ForegroundColor White
while ($true) {
    foreach ($l in $Logs) { try { Linhas $l } catch { } }
    Start-Sleep -Milliseconds 700
}
