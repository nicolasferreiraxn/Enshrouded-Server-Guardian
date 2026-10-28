<p align="center">
  <img src=".github/capa.png" alt="Enshrouded Server Guardian" width="100%">
</p>

<h1 align="center">Enshrouded Server Guardian</h1>

<p align="center">
  <b>Fair progress on your dedicated server.</b><br>
  The server becomes the authority over each player's character — not the PC of whoever joins.
</p>

<p align="center">
  <a href="../../releases/latest"><img src="https://img.shields.io/github/v/release/nicolasferreiraxn/Enshrouded-Server-Guardian?style=for-the-badge&label=download&color=d4a537" alt="Download"></a>
  <a href="../../actions/workflows/build.yml"><img src="https://img.shields.io/github/actions/workflow/status/nicolasferreiraxn/Enshrouded-Server-Guardian/build.yml?style=for-the-badge&label=build" alt="Build"></a>
  <img src="https://img.shields.io/badge/Enshrouded-dedicated%20server-8b3fd1?style=for-the-badge" alt="Dedicated server">
  <img src="https://img.shields.io/badge/Rust-~700%20lines-b7410e?style=for-the-badge&logo=rust" alt="Rust">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-3c9d5d?style=for-the-badge" alt="MIT"></a>
</p>

<p align="center">
  🇺🇸 English · 🇧🇷 <a href="README.pt-BR.md">Português</a>
</p>

---

## 🔥 Where it came from

Four friends on a dedicated server. Two problems showed up:

1. **A character from somewhere else.** Enshrouded stores the character on the **client** and sends it when joining.
   You could play in a private world, gear up there and join the shared server with that gear. Or restore an old
   local backup and undo a death.
2. **Progress ahead of the group.** One player unlocked recipes and items from zones far ahead of where the group
   was. Cleaning that up was pointless while anyone could bring it right back.

Both come from the same root: **the client was the source of truth for the character**. The guardian flips that.
The server keeps its own copy of what it last saved for each player — and that copy wins.

## ✨ What it does

| | |
|---|---|
| 🛡️ **The server decides** | on join, it compares the incoming character with the server's last copy and, if they differ, puts the server's copy back |
| 🎒 **Nothing comes from outside** | gear, level, skill tree, inventory, discoveries and recipes are whatever the server saw |
| 🔍 **Evidence for everything** | keeps the rejected save **and** the reference copy from that exact moment, so comparisons don't mislead |
| 👀 **Observe mode** | logs every divergence without changing anything — measure before you enforce |
| ✅ **Verified swap** | after writing, it re-reads and compares; if it didn't stick, it logs the failure instead of pretending |
| 🔧 **Survives updates** | hooks found by byte pattern, not by fixed address |
| 🧩 **Pairs with the Save Editor** | edit a player's copy, kick them, and they come back with your version |

## 🚀 Installation

1. Download **`dbghelp.dll`** from **[Releases](../../releases/latest)**.
2. **Stop** the server.
3. Put the file in the server folder, next to `enshrouded_server.exe`.
4. Start the server. The `guardiao-dados/` folder appears the first time a player joins.

It also works with the server running under **Wine** on Linux, with no extra option — the server itself asks for
`dbghelp.dll`. Convenience scripts for self-hosting are in [`install/`](install/).

### 🟣 On BisectHosting

BisectHosting has its own DLL loader, so the file goes in a different place:

1. **Stop** the server.
2. Upload **`dbghelp.dll`** to the **`DLLMods/`** folder — **not** next to `enshrouded_server.exe`.
3. Start the server. The `guardiao-dados/` folder and `guardiao.json` still live next to `enshrouded_server.exe`.

Load order and timing can be tweaked in `DLLMods/chainloader.ini`, which also has a delay option for DLLs that
need the server binary to be fully loaded. The guardian normally needs no tweaking. To confirm it's active, open
`guardiao-dados/guardiao.log`: it should say `instalado`. If it says `não achei os pontos de ligação` instead, give
it a delay in `chainloader.ini`.

> ⚠️ **On other managed hosts, check that they let you keep your own `dbghelp.dll`.** Some hosts replace that
> file on every boot, because their own mod loader uses the same name.

### Hosts we tested

| host | result |
|---|---|
| **Nodecraft** | ✅ works — `dbghelp.dll` stays where you put it and loads on boot |
| **Self-hosted (Windows)** | ✅ works, no caveats |
| **Linux VPS + Wine** | ⚠️ works, but ran poorly for us (`wineserver` became the bottleneck; `ntsync` needs kernel ≥ 6.14) |
| **BisectHosting** | ✅ works — upload to `DLLMods/` instead of next to the binary ([details](#-on-bisecthosting)) |

Measured on the 1.0 dedicated server. Host not listed? Drop the DLL in, start the server and check whether the file
is still there.

### 🎚️ Choosing the mode

The guardian has two modes, picked by a `guardiao.json` file in the server root, next to `enshrouded_server.exe`:

| mode | `guardiao.json` | what happens when a player joins with a character that differs from the server's copy |
|---|---|---|
| 🛡️ **Enforce** (default) | `{ "modo": "aplicar" }` — or no file at all | the server's copy is put back, and the rejected save is kept as evidence |
| 👀 **Observe** | `{ "modo": "observar" }` | it's only logged in `guardiao-dados/registro.jsonl`; nothing is changed |

To switch: **stop** the server, edit or create `guardiao.json`, **start** it again. The mode is read only on boot.
`guardiao-dados/guardiao.log` shows which mode is active (`instalado; modo aplicar` or `modo observar`).

A good way to start on a server that already has players: run in **observe** for a few sessions, check
`registro.jsonl` to see who would be affected, then switch to **enforce**. An example file is in
[`install/examples/guardiao.json`](install/examples/guardiao.json).

## ⚙️ How it works

<details>
<summary><b>The hooks</b></summary>
<br>

The server loads `dbghelp.dll` on startup — a common Windows library — so a DLL with that name in the server folder
gets loaded too. The guardian forwards to the real `dbghelp.dll` the only function the server wants
(`MiniDumpWriteDump`) and installs three hooks:

1. **Session start** — the log call announcing a remote player joining, where the Steam ID shows up and is tied to
   the session.
2. **Save read** — right after the server reads the character save the client sent.
3. **Save write** — where the server builds a character save, which is how the guardian learns its version.

The hooks are found **by byte pattern**. That's how the DLL survived a server update (build 1273043) without a
rebuild. If a pattern becomes ambiguous or disappears, it refuses to install that hook instead of patching the wrong
place.
</details>

<details>
<summary><b>The decision on every join</b></summary>
<br>

| decision | meaning |
|---|---|
| `primeira_entrada` | no copy yet; what the client sent becomes the baseline |
| `igual` | byte-for-byte identical to the stored copy; nothing to do |
| `substituido` | they differ; the stored copy is written to the server and verified |
| `divergente_observado` | they differ, but the mode is observe, so it only logs |

If the write doesn't stick, it logs `substituicao_falhou`.
</details>

<details>
<summary><b>What it stores</b></summary>
<br>

Per player, in `guardiao-dados/<steamid>/`:

- `A.bin` — the character: level, xp, skill tree, equipment, inventory
- `K.bin` — knowledge: discoveries and recipes
- `meta.json` — version, sizes and date of the stored copy
- `recusados/<timestamp>_substituido.bin` — the save the client sent that was rejected
- `recusados/<timestamp>_referencia.bin` — the copy the server had at that exact moment

Plus a shared `guardiao-dados/registro.jsonl`, one line per event.

The reference next to the rejected save matters more than it looks: without it, comparing a rejection from hours ago
with today's copy mixes "what the player brought" with "what they played afterwards".
</details>

## 🚫 What it doesn't do

- **It isn't a general anti-cheat.** Whatever the player does during the session is kept — including what a trainer
  does while they're connected.
- **It doesn't touch the world:** buildings, chests and anything outside the character and knowledge blocks stay as
  they are.
- **It doesn't talk to the network** or any service. It only reads and writes files in the server folder.
- **It depends on the server's internals.** If an update moves the patterns, the hooks don't install and the DLL gets
  out of the way.

## 🛠️ Building

Every release is built by **GitHub Actions** from this repository, with a [public log](../../actions/workflows/build.yml).
That's on purpose: this is a DLL that goes into a game server, and nobody should run one downloaded from a stranger
without being able to see what it does. All the code is here — five files, about 700 lines of Rust.

Locally, with Rust for `x86_64-pc-windows-msvc`:

```
cargo build --release      # target/release/dbghelp.dll
```

## 🧩 Same family

| | |
|---|---|
| 💾 **[Enshrouded Save Editor](https://github.com/nicolasferreiraxn/Enshrouded-Save-Editor)** | edit your character with an interface that looks like the game — and, with the guardian, the server's copies |
| 🗺️ **[Enshrouded Interactive Map](https://github.com/nicolasferreiraxn/Enshrouded-Interactive-Map)** | real-time map on your second monitor: your position, friends and everything already done |

## ❤️ Credits

- The character and knowledge block formats were worked out by measurement; nothing is copied from the game's code.
- **[kfc-parser / Enshrouded Mod Loader](https://github.com/Brabb3l/kfc-parser)** by brabb3l — used to read the game's resources.
- **BisectHosting** — Jake and Dylan reached out, made the guardian work with their DLL loader and explained how
  to install it there. Thank you for caring about the community! 💜
- **Keen Games** — for Enshrouded.

---

<p align="center">
  <sub>
    Fan project, not affiliated with Keen Games. No official support — use, read, modify and publish freely.<br>
    Code under <a href="LICENSE">MIT</a>.
  </sub>
</p>
