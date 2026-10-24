#!/bin/sh
# Envia um arquivo para a VPS pelos comandos do SSH (o SFTP de lá não funciona), em pedaços de base64,
# e confere o sha256 no fim. Uso: sh enviar.sh <arquivo local> <caminho na VPS, ex. C:\Enshrouded\x.ksd>
set -e
LOCAL="$1"; REMOTO="$2"
SSH="ssh -n -i $HOME/.ssh/enshrouded_vps -o BatchMode=yes -o ConnectTimeout=15 administrador@148.224.62.40"
TMP="$REMOTO.b64parcial"
$SSH "Remove-Item '$TMP' -ErrorAction SilentlyContinue; exit 0"
base64 -w0 "$LOCAL" | fold -w 24000 | while IFS= read -r pedaco; do
    # o sshd do Windows recusa conexões seguidas demais ("exec request failed"): tenta de novo com pausa
    n=0
    until $SSH "[IO.File]::AppendAllText('$TMP', '$pedaco')" 2>/dev/null; do
        n=$((n + 1)); [ $n -ge 8 ] && { echo "falhou o envio de um pedaço"; exit 1; }
        sleep 2
    done
    printf '.'
done
echo
$SSH "[IO.File]::WriteAllBytes('$REMOTO', [Convert]::FromBase64String([IO.File]::ReadAllText('$TMP'))); Remove-Item '$TMP'; (Get-FileHash '$REMOTO').Hash.ToLower()"
echo "$(sha256sum "$LOCAL" | cut -d' ' -f1)  (local)"
