# Deploy do servidor com o guardião (máquina Windows)

## Antes
- Máquina: Windows 10/11 ou Server 2019+, 4 vCPUs, 8 GB, **60 GB de disco** (30 GB não basta).
- No painel do provedor, liberar a entrada **UDP na porta da config** (`queryPort` em `arquivos/enshrouded_server.json`;
  hoje é **9021**, a mesma da BisectHosting).
- No dia da mudança, pegar o `world0.ksd` mais recente da BisectHosting (com o servidor de lá desligado) e pôr em
  `arquivos/` no lugar do que está aqui (backup de 2026-10-20).

## Instalar
1. Copiar a pasta `deploy` inteira para a máquina (por RDP, arrastando).
2. Abrir o PowerShell **como administrador** na pasta e rodar:
   `powershell -ExecutionPolicy Bypass -File instalar.ps1`
3. Ligar: `Start-ScheduledTask 'Enshrouded Servidor'`

O script instala o VC++ 2022, o SteamCMD e o servidor (app 2278520) em `C:\Enshrouded\servidor`, confere o build,
copia mundo, config e guardião, libera a porta no firewall do Windows e cria duas tarefas: **Enshrouded Servidor**
(sobe com o Windows e reinicia se o servidor fechar) e **Enshrouded Backup** (todo dia às 5h, 14 cópias em
`C:\Enshrouded\backups`).

## Conferir
- `C:\Enshrouded\servidor\guardiao-dados\guardiao.log` deve dizer `instalado; modo observar`.
- Cada jogador que entrar aparece em `guardiao-dados\registro.jsonl` (`primeira_entrada`).

## Depois que os 4 entrarem uma vez
1. Parar: `powershell -File C:\Enshrouded\parar.ps1`
2. Trocar `C:\Enshrouded\servidor\guardiao.json` para `{"modo": "aplicar"}`.
3. Limpeza acima do nível (personagens, mundo e baús): copiar `guardiao-dados` e `savegame` para a máquina do
   Nicolas, rodar as ferramentas de `teste/` com a revisão aprovada e trazer de volta.
4. Ligar: `Start-ScheduledTask 'Enshrouded Servidor'`

## Atualização do jogo
Não rode o SteamCMD de novo por conta própria. Quando o jogo atualizar, o guardião se desliga sozinho (o log diz
"não é o build analisado") e o servidor segue sem proteção até a análise ser refeita. Para atualizar:
`parar.ps1`, depois `C:\Enshrouded\steamcmd\steamcmd.exe +force_install_dir C:\Enshrouded\servidor +login anonymous +app_update 2278520 validate +quit`.

## Desfazer o guardião
Apagar `C:\Enshrouded\servidor\dbghelp.dll` e reiniciar o servidor.
