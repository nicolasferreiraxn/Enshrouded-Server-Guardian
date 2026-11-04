//! Guarantee that the guardian never gives a player back a copy older than their real character.
//!
//! Copies only stay current while the guardian is active. If the server runs without it (DLL removed,
//! hooks not found after an update) and players keep playing, the copies get old; enforcing again
//! without knowing that would put the old copy back. So the guardian writes to guardiao-dados/ativo.json,
//! every minute, that it is active. On load, if the world was saved well after the last time it was
//! active, the server ran without it: every copy is marked as outdated, and each player has their copy
//! refreshed on their next join instead of restored.

use crate::registro::{self, t};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Marker, in a player's folder, that their copy predates a period without the guardian.
pub const MARCA: &str = "DESATUALIZADA";

/// Slack between the guardian's last heartbeat and the world's last save: the server saves the world
/// on shutdown, a few seconds after the last heartbeat.
const FOLGA: u64 = 600;

fn segundos(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn arquivo() -> PathBuf {
    registro::dados().join("ativo.json")
}

/// World save folder, from enshrouded_server.json ("saveDirectory"), or ./savegame.
fn pasta_do_mundo() -> PathBuf {
    let json = fs::read_to_string(registro::raiz().join("enshrouded_server.json")).unwrap_or_default();
    let pasta = json
        .find("\"saveDirectory\"")
        .and_then(|i| {
            let resto = json[i + 15..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
            Some(resto[..resto.find('"')?].to_string())
        })
        .unwrap_or_else(|| "./savegame".into());
    registro::raiz().join(pasta)
}

/// Last write of any world file (not descending into backup/).
fn mundo_gravado_em() -> Option<u64> {
    fs::read_dir(pasta_do_mundo())
        .ok()?
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .filter_map(|m| m.modified().ok().map(segundos))
        .max()
}

fn ultimo_sinal() -> Option<u64> {
    let json = fs::read_to_string(arquivo()).ok()?;
    let resto = &json[json.find("\"visto\":")? + 8..];
    resto.trim().split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// Checks, before any hook, whether the server ran without the guardian since the last heartbeat. If it
/// did, marks every copy. Without ativo.json (first load, or coming from 1.0.0), the copies are trusted.
pub fn conferir() {
    let (Some(sinal), Some(mundo)) = (ultimo_sinal(), mundo_gravado_em()) else { return };
    if mundo <= sinal + FOLGA {
        return;
    }
    let n = marcar_todas();
    let horas = (mundo - sinal) / 3600;
    registro::log(&format!(
        "{} {horas}h. {n} {}",
        t("WARNING: the world was saved while the guardian was not running, for about",
          "AVISO: o mundo foi gravado enquanto o guardião não estava rodando, por cerca de"),
        t("stored copies may be older than the players' real characters. Each of those players will have their copy refreshed on their next join, not restored.",
          "cópias guardadas podem estar mais antigas que os personagens reais. Cada um desses jogadores terá a cópia renovada na próxima entrada, e não restaurada."),
    ));
    registro::registrar("copias_marcadas", &[("quantas", n.to_string()), ("semGuardiaoSeg", (mundo - sinal).to_string())]);
}

/// Marks every copy as outdated. Returns how many.
pub fn marcar_todas() -> usize {
    let mut n = 0;
    if let Ok(dirs) = fs::read_dir(registro::dados()) {
        for d in dirs.flatten() {
            if d.file_name().to_string_lossy().parse::<u64>().is_ok() && d.path().is_dir() {
                n += fs::write(d.path().join(MARCA), b"").is_ok() as usize;
            }
        }
    }
    n
}

/// Heartbeat every minute, while the server runs with the guardian active.
pub fn sinal_de_vida(versao: &'static str) {
    std::thread::spawn(move || loop {
        let agora = segundos(SystemTime::now());
        let _ = registro::gravar_atomico(
            &arquivo(),
            format!("{{\"versao\": \"{versao}\", \"visto\": {agora}, \"quando\": \"{}\"}}", registro::agora()).as_bytes(),
        );
        std::thread::sleep(Duration::from_secs(60));
    });
}
