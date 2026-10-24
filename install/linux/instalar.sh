#!/bin/bash
# Servidor do Enshrouded (Windows) no Ubuntu 22.04 via Wine, com o guardião (dbghelp.dll).
# Rodar como root na pasta do pacote:  bash instalar.sh
# O pacote traz: arquivos/world0.ksd, arquivos/enshrouded_server.json, arquivos/dbghelp.dll, arquivos/guardiao.json
set -euo pipefail
AQUI="$(cd "$(dirname "$0")" && pwd)"
USUARIO=enshrouded
CASA=/home/$USUARIO
SERVIDOR=$CASA/servidor
SHA_ESPERADO=b627639ce4a3f8c940fe90afa1d36ed59db3ff9ced24e935890eb7c99b04d5bf
passo() { echo; echo "== $*"; }

[ "$(id -u)" = 0 ] || { echo "rode como root"; exit 1; }
for f in world0.ksd enshrouded_server.json dbghelp.dll guardiao.json; do
    [ -f "$AQUI/arquivos/$f" ] || { echo "falta arquivos/$f"; exit 1; }
done
PORTA=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['queryPort'])" "$AQUI/arquivos/enshrouded_server.json")

passo "Pacotes (Wine da WineHQ, Xvfb, SteamCMD)"
export DEBIAN_FRONTEND=noninteractive
dpkg --add-architecture i386
mkdir -pm755 /etc/apt/keyrings
wget -qO /etc/apt/keyrings/winehq-archive.key https://dl.winehq.org/wine-builds/winehq.key
wget -qNP /etc/apt/sources.list.d/ https://dl.winehq.org/wine-builds/ubuntu/dists/jammy/winehq-jammy.sources
apt-get update -q
apt-get install -yq --install-recommends winehq-stable
apt-get install -yq xvfb lib32gcc-s1 ca-certificates curl ufw cron

passo "Usuário $USUARIO"
id $USUARIO >/dev/null 2>&1 || useradd -m -s /bin/bash $USUARIO
mkdir -p $CASA/steamcmd $SERVIDOR/savegame $CASA/backups
chown -R $USUARIO:$USUARIO $CASA

passo "SteamCMD e servidor (app 2278520) - alguns minutos"
sudo -u $USUARIO bash -c "cd $CASA/steamcmd && [ -x steamcmd.sh ] || curl -sqL https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz | tar zxf -"
for i in 1 2 3; do
    [ -f $SERVIDOR/enshrouded_server.exe ] && break
    sudo -u $USUARIO $CASA/steamcmd/steamcmd.sh +@sSteamCmdForcePlatformType windows +force_install_dir $SERVIDOR \
        +login anonymous +app_update 2278520 validate +quit || true
done
[ -f $SERVIDOR/enshrouded_server.exe ] || { echo "o SteamCMD não instalou o servidor"; exit 1; }

passo "Conferindo o build"
SHA=$(sha256sum $SERVIDOR/enshrouded_server.exe | cut -d' ' -f1)
if [ "$SHA" != "$SHA_ESPERADO" ]; then
    echo "AVISO: executável $SHA não é o build analisado; o guardião vai se desligar sozinho."
else echo "build conferido: $SHA"; fi

passo "Mundo, config e guardião"
if [ -f $SERVIDOR/savegame/world0.ksd ] && [ "${SOBRESCREVER:-0}" != 1 ]; then
    echo "já existe savegame/world0.ksd; rode com SOBRESCREVER=1 para trocar"; exit 1
fi
install -o $USUARIO -g $USUARIO -m 644 "$AQUI/arquivos/world0.ksd" $SERVIDOR/savegame/world0.ksd
for f in enshrouded_server.json dbghelp.dll guardiao.json; do
    install -o $USUARIO -g $USUARIO -m 644 "$AQUI/arquivos/$f" $SERVIDOR/$f
done

passo "Prefixo do Wine"
sudo -u $USUARIO env WINEPREFIX=$CASA/.wine WINEDEBUG=-all xvfb-run -a wineboot --init >/dev/null 2>&1 || true

passo "Serviço systemd"
cat > /etc/systemd/system/enshrouded.service <<EOF
[Unit]
Description=Servidor Enshrouded (Wine) com guardiao
After=network-online.target
Wants=network-online.target

[Service]
User=$USUARIO
WorkingDirectory=$SERVIDOR
Environment=WINEPREFIX=$CASA/.wine
Environment=WINEDEBUG=-all
Environment=WINEDLLOVERRIDES=dbghelp=native,builtin
ExecStart=/usr/bin/xvfb-run -a /usr/bin/wine $SERVIDOR/enshrouded_server.exe
Restart=on-failure
RestartSec=30
KillSignal=SIGINT
TimeoutStopSec=60

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable enshrouded >/dev/null

passo "Backup diário (5h, 14 cópias) e console"
cat > /usr/local/bin/backup-enshrouded <<EOF
#!/bin/bash
cd $SERVIDOR && tar czf $CASA/backups/\$(date +%Y%m%d_%H%M).tar.gz savegame guardiao-dados enshrouded_server.json guardiao.json 2>/dev/null
ls -1t $CASA/backups/*.tar.gz | tail -n +15 | xargs -r rm --
EOF
chmod 755 /usr/local/bin/backup-enshrouded
echo "0 5 * * * root /usr/local/bin/backup-enshrouded" > /etc/cron.d/enshrouded-backup
cat > /usr/local/bin/console-enshrouded <<EOF
#!/bin/bash
# acompanha ao vivo o log do servidor e o do guardião (Ctrl+C sai; o servidor continua)
systemctl --no-pager status enshrouded | head -3
cd $SERVIDOR && tail -n 30 -F logs/enshrouded_server.log guardiao-dados/guardiao.log guardiao-dados/registro.jsonl 2>/dev/null | grep --line-buffered -v 'Started thread'
EOF
chmod 755 /usr/local/bin/console-enshrouded

passo "Firewall (SSH e UDP $PORTA)"
ufw allow OpenSSH >/dev/null
ufw allow $PORTA/udp >/dev/null
ufw --force enable >/dev/null
ufw status | head -8

passo "Pronto"
echo "ligar:    systemctl start enshrouded"
echo "logs:     console-enshrouded"
echo "parar:    systemctl stop enshrouded"
echo "guardião: $SERVIDOR/guardiao-dados/guardiao.log (modo inicial: observar)"
