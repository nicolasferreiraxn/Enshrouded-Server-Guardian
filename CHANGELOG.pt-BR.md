# Mudanças

*Read this in [English](CHANGELOG.md).*

## 1.1.0 — 04/11/2026

### Atualizações do jogo, sem perder progresso

- **Funciona na atualização do jogo de novembro de 2026 (build 1285601)** sem recompilar: os ganchos são
  achados por padrão de bytes, como em outubro.
- **Nunca faz um jogador voltar no tempo.** Se os ganchos não puderem ser instalados, o guardião avisa e marca
  todas as cópias como desatualizadas; quando ele voltar a funcionar, renova a cópia de cada jogador em vez de
  restaurar uma antiga.
- **Percebe quando o servidor rodou sem ele.** Ele grava um sinal de vida a cada minuto
  (`guardiao-dados/ativo.json`); se o mundo foi gravado muito depois do último (DLL removida, ganchos não
  achados), as cópias são marcadas do mesmo jeito.
- **Uma cópia de outra versão do jogo nunca é usada para trocar um personagem.** Cada cópia agora guarda o
  build do servidor que a gravou; na primeira entrada depois de uma atualização do jogo, o personagem atual fica
  e a cópia é renovada. As cópias da 1.0.0 não têm build, então a primeira entrada divergente de cada jogador
  depois de atualizar é aceita uma vez.
- **Histórico:** as 10 últimas cópias de cada jogador ficam em `historico/`, prontas para serem postas de volta
  à mão.

### Um guardião que aparece

- **Mensagens no console para tudo:** carregamento, modo, jogadores conectando, a decisão de cada entrada,
  cópias guardadas, avisos e erros — com o tamanho do personagem antes e depois quando algo difere.
- **Letreiro de inicialização** mostrado quando o mundo termina de carregar, para não se perder entre as linhas
  de boot do jogo. Os endereços dos ganchos vão só para o `guardiao.log`.
- **BisectHosting:** o painel mostra o `DLLMods/loader.log`, mas não a saída do servidor; agora o guardião
  também acrescenta as linhas dele lá. A instalação na BisectHosting (enviar para `DLLMods/`) está documentada.
- **Inglês por padrão**, português com `"idioma": "pt-BR"` (ou `"language"`) no `guardiao.json`, que também
  aceita `{"mode": "observe"}` além de `{"modo": "observar"}`.
- **Quando não consegue se ligar ao servidor,** diz claramente que está DESLIGADO, que nada é alterado e como
  avisar.

### Aviso de versão nova

- Consulta o GitHub ao ligar e depois a cada 24 horas. Quando há versão nova, um aviso emoldurado aparece no
  console e se repete a cada hora; as consultas param, porque o guardião já sabe.
- Nunca baixa nada nem envia nada sobre o seu servidor. Desliga com `"checkUpdates": false`. É o único acesso à
  rede do guardião.

## 1.0.0 — 24/10/2026

Primeira versão: o servidor dedicado guarda a sua própria cópia do personagem e do conhecimento de cada
jogador e a põe de volta quando alguém entra com outra, com o save recusado guardado como prova, modo
observar, troca conferida e ganchos achados por padrão de bytes.
