# Changelog

*Leia em [português](CHANGELOG.pt-BR.md).*

## 1.1.0 — 2026-11-04

### Game updates, without losing progress

- **Works on the November 2026 game update (build 1285601)** with no rebuild: the hooks are found by
  byte pattern, as in October.
- **Never rolls a player back.** If the hooks can't be installed, the guardian says so and marks every
  stored copy as outdated; when it works again, it refreshes each player's copy instead of restoring an old one.
- **Notices when the server ran without it.** It writes a heartbeat every minute (`guardiao-dados/ativo.json`);
  if the world was saved long after the last one (DLL removed, hooks missing), copies are marked the same way.
- **A copy from another game version is never used to replace a character.** Each copy now records the
  server build that wrote it; on the first join after a game update, the current character is kept and the
  copy is refreshed. Copies made by 1.0.0 have no build, so each player's first diverging join after
  upgrading is accepted once.
- **History:** the last 10 copies of each player are kept in `historico/`, ready to be put back by hand.

### A guardian you can see

- **Console messages for everything:** load, mode, players connecting, each join decision, saved copies,
  warnings and errors — with the character size before and after when something differs.
- **Boot banner** shown once the world has loaded, so it doesn't get lost among the game's boot lines.
  Hook addresses go only to `guardiao.log`.
- **BisectHosting:** its panel shows `DLLMods/loader.log` but not the server output, so the guardian now
  also appends its lines there. Installing on BisectHosting (upload to `DLLMods/`) is documented.
- **English by default**, Portuguese with `"idioma": "pt-BR"` (or `"language"`) in `guardiao.json`, which
  also accepts `{"mode": "observe"}` besides `{"modo": "observar"}`.
- **When it can't hook in,** it says clearly that it's OFF, that nothing is changed, and how to report it.

### New version notice

- Checks GitHub for a newer guardian on boot and then every 24 hours. When there is one, a framed notice
  appears in the console and is repeated every hour; the checks stop, since the guardian already knows.
- It never downloads anything and sends nothing about your server. Turn it off with `"checkUpdates": false`.
  This is the guardian's only network request.

## 1.0.0 — 2026-10-24

First release: the dedicated server keeps its own copy of each player's character and knowledge and puts it
back when someone joins with a different one, with the rejected save kept as evidence, an observe mode,
verified replacements and hooks found by byte pattern.
