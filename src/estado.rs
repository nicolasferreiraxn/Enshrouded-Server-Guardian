//! Guardian logic: copies per steamid, comparison on join, replacement, and a full send on save.
//! Offsets of the save state and of the session record.

use crate::continuidade::MARCA;
use crate::registro::{self, t, texto};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// How many previous copies of each player are kept in historico/ (one per join).
const HISTORICO: usize = 10;

#[derive(Clone)]
pub struct Copia {
    pub a: Vec<u8>,
    pub k: Vec<u8>, // n u32 ids followed by n u32 values
    pub k_estado: u8,
    /// Server build that wrote the copy (executable timestamp). Empty in copies made by 1.0.0.
    pub build: String,
    /// The copy predates a period when the server ran without the guardian.
    pub desatualizada: bool,
}

fn campo_meta<'a>(meta: &'a str, chave: &str) -> Option<&'a str> {
    let resto = meta.split(&format!("\"{chave}\":")).nth(1)?.trim_start();
    Some(match resto.strip_prefix('"') {
        Some(r) => &r[..r.find('"')?],
        None => resto.split(|c: char| !c.is_ascii_digit()).next()?,
    })
}

/// Reads a player's copy from disk. The file is the authority: an edit made from outside
/// (by the save editor) takes effect on the next join, without restarting the server.
fn ler_do_disco(sid: u64) -> Option<Copia> {
    let dir = registro::dados().join(sid.to_string());
    let a = fs::read(dir.join("A.bin")).ok()?;
    let k = fs::read(dir.join("K.bin")).ok()?;
    if a.is_empty() || k.len() % 8 != 0 {
        return None;
    }
    let meta = fs::read_to_string(dir.join("meta.json")).unwrap_or_default();
    let k_estado = campo_meta(&meta, "kEstado").and_then(|v| v.parse().ok()).unwrap_or(1);
    let build = campo_meta(&meta, "build").unwrap_or("").to_string();
    Some(Copia { a, k, k_estado, build, desatualizada: dir.join(MARCA).exists() })
}

pub struct Estado {
    pub aplicar: bool,
    pub build: String,
    pub copias: HashMap<u64, Copia>,
    pub steam_do_handle: HashMap<u32, u64>,
    pub forcar_completo: HashSet<u32>,
}

static ESTADO: OnceLock<Mutex<Estado>> = OnceLock::new();

pub fn iniciar(aplicar: bool, build: String) -> usize {
    let mut copias = HashMap::new();
    if let Ok(dirs) = fs::read_dir(registro::dados()) {
        for d in dirs.flatten() {
            let Ok(sid) = d.file_name().to_string_lossy().parse::<u64>() else { continue };
            if let Some(c) = ler_do_disco(sid) {
                copias.insert(sid, c);
            }
        }
    }
    let n = copias.len();
    let _ = ESTADO.set(Mutex::new(Estado { aplicar, build, copias, steam_do_handle: HashMap::new(), forcar_completo: HashSet::new() }));
    n
}

/// Keeps the reference copy of this moment in historico/<timestamp>/, ready to be put back by hand
/// (by copying the three files into the player's folder), and deletes the oldest ones.
fn guardar_historico(dir: &Path, carimbo: &str) {
    let hist = dir.join("historico");
    let destino = hist.join(carimbo);
    if fs::create_dir_all(&destino).is_err() {
        return;
    }
    for nome in ["A.bin", "K.bin", "meta.json"] {
        let _ = fs::copy(dir.join(nome), destino.join(nome));
    }
    let Ok(lista) = fs::read_dir(&hist) else { return };
    let mut nomes: Vec<_> = lista.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    nomes.sort();
    let sobra = nomes.len().saturating_sub(HISTORICO);
    for velho in &nomes[..sobra] {
        let _ = fs::remove_dir_all(velho);
    }
}

/// "A: 1369 -> 1355 bytes, knowledge: 3661 -> 3661"
fn diferenca(c: &Copia, a: &[u8], k: &[u8]) -> String {
    format!("A: {} -> {} bytes, {}: {} -> {}", c.a.len(), a.len(), t("knowledge", "conhecimento"), c.k.len() / 8, k.len() / 8)
}

fn com_estado<R>(f: impl FnOnce(&mut Estado) -> R) -> Option<R> {
    let m = ESTADO.get()?;
    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
    Some(f(&mut g))
}

// ---------------------------------------------------------------- server memory

unsafe fn rd<T: Copy>(p: *const u8, off: usize) -> T {
    core::ptr::read_unaligned(p.add(off) as *const T)
}

unsafe fn wr<T: Copy>(p: *mut u8, off: usize, v: T) {
    core::ptr::write_unaligned(p.add(off) as *mut T, v)
}

type Cresce = unsafe extern "system" fn(*mut u8, u32, u32) -> u8;

/// Engine array at `base`: {ptr +0, length +8, capacity +0x10, grow function +0x18}.
unsafe fn ler_array(base: *const u8, tam_elem: usize) -> Vec<u8> {
    let p: *const u8 = rd(base, 0);
    let n: u64 = rd(base, 8);
    if p.is_null() || n == 0 || n > 64 * 1024 * 1024 {
        return Vec::new();
    }
    core::slice::from_raw_parts(p, n as usize * tam_elem).to_vec()
}

unsafe fn gravar_array(base: *mut u8, dados: &[u8], tam_elem: usize) -> Result<(), &'static str> {
    let n = (dados.len() / tam_elem) as u64;
    if n > rd::<u64>(base, 0x10) {
        let f: usize = rd(base, 0x18);
        if f == 0 {
            return Err("array sem função de crescimento");
        }
        let f: Cresce = core::mem::transmute(f);
        f(base, n as u32, 0);
        if n > rd::<u64>(base, 0x10) {
            return Err("crescimento falhou");
        }
    }
    if n > 0 {
        let p: *mut u8 = rd(base, 0);
        core::ptr::copy_nonoverlapping(dados.as_ptr(), p, dados.len());
    }
    wr::<u64>(base, 8, n);
    Ok(())
}

unsafe fn ler_k(st: *const u8) -> Option<Vec<u8>> {
    let ids = ler_array(st.add(0xA0), 4);
    let vals = ler_array(st.add(0xC8), 4);
    if ids.len() != vals.len() {
        return None;
    }
    let mut k = ids;
    k.extend_from_slice(&vals);
    Some(k)
}

unsafe fn gravar_k(st: *mut u8, k: &[u8], k_estado: u8) -> Result<(), &'static str> {
    let meio = k.len() / 2;
    gravar_array(st.add(0xA0), &k[..meio], 4)?;
    gravar_array(st.add(0xC8), &k[meio..], 4)?;
    wr::<u8>(st, 0x98, k_estado);
    Ok(())
}

// ---------------------------------------------------------------- hook points

/// "Remote player added" log: session record with the handle at +0 and the steamid at +0x60.
pub unsafe fn sessao(registro_sessao: *const u8) {
    if registro_sessao.is_null() {
        return;
    }
    let handle: u32 = rd(registro_sessao, 0);
    let sid: u64 = rd(registro_sessao, 0x60);
    com_estado(|e| e.steam_do_handle.insert(handle, sid));
    registro::registrar("sessao", &[("handle", handle.to_string()), ("steamid", texto(&sid.to_string()))]);
    registro::log(&format!("{} {sid} {}", t("Player", "Jogador"), t("is connecting", "está conectando")));
}

/// Before the send task. Returns the handle if the send was forced to be complete.
pub unsafe fn saida_antes(st: *mut u8) -> Option<u32> {
    if st.is_null() {
        return None;
    }
    let handle: u32 = rd(st, 8);
    let a = ler_array(st.add(0x18), 1);
    let k = ler_k(st)?;
    let k_estado: u8 = rd(st, 0x98);
    let (versao, final_) = (rd::<u32>(st, 0x10), rd::<u8>(st, 0x14));
    com_estado(|e| {
        let forcado = e.forcar_completo.contains(&handle);
        if forcado {
            wr::<u8>(st, 0x140, rd::<u8>(st, 0x140) | 1);
        }
        let Some(&sid) = e.steam_do_handle.get(&handle) else {
            registro::log(&format!("{} {handle}", t("Server save for an unknown player (no Steam ID yet), session",
                "Gravação do servidor de um jogador desconhecido (ainda sem Steam ID), sessão")));
            return forcado.then_some(handle);
        };
        if !a.is_empty() {
            let dir = registro::dados().join(sid.to_string());
            let _ = fs::create_dir_all(&dir);
            let ok = registro::gravar_atomico(&dir.join("A.bin"), &a).is_ok()
                && registro::gravar_atomico(&dir.join("K.bin"), &k).is_ok()
                && registro::gravar_atomico(&dir.join("meta.json"), format!(
                    "{{\"steamid\": \"{sid}\", \"handle\": {handle}, \"versao\": {versao}, \"final\": {final_}, \"tamA\": {}, \"nK\": {}, \"kEstado\": {k_estado}, \"build\": \"{}\", \"quando\": \"{}\"}}",
                    a.len(), k.len() / 8, e.build, registro::agora()).as_bytes()).is_ok();
            if ok {
                // the copy just came from the server: it is current, whatever the marker said
                let _ = fs::remove_file(dir.join(MARCA));
            } else {
                registro::log(&format!("{} {sid}", t("ERROR: could not write the copy of player", "ERRO: não consegui gravar a cópia do jogador")));
            }
            if final_ != 0 {
                registro::registrar("guardado", &[("steamid", texto(&sid.to_string())), ("versao", versao.to_string()),
                    ("tamA", a.len().to_string()), ("nK", (k.len() / 8).to_string())]);
                registro::log(&format!("{} {sid}", t("Saved the server copy of player", "Cópia do servidor guardada para o jogador")));
            }
            e.copias.insert(sid, Copia { a, k, k_estado, build: e.build.clone(), desatualizada: false });
        }
        forcado.then_some(handle)
    }).flatten()
}

/// After the forced send task: +0xf8 = 1 if the knowledge was sent complete.
pub unsafe fn saida_depois(st: *const u8, handle: u32) {
    let completo: u8 = rd(st, 0xf8);
    if completo != 0 {
        com_estado(|e| e.forcar_completo.remove(&handle));
    }
    registro::registrar("envio_completo", &[("handle", handle.to_string()), ("completo", completo.to_string())]);
    if completo == 0 {
        registro::log(&format!("{} {handle}", t("WARNING: the server copy was not fully sent back yet, session",
            "AVISO: a cópia do servidor ainda não foi mandada inteira de volta, sessão")));
    }
}

/// After the server has read the client's save (state at `st`).
pub unsafe fn entrada(st: *mut u8) {
    if st.is_null() {
        return;
    }
    let handle: u32 = rd(st, 8);
    let a = ler_array(st.add(0x18), 1);
    let Some(k) = ler_k(st) else {
        registro::log(t("A player joined with an unreadable knowledge list; nothing was done",
            "Um jogador entrou com a lista de conhecimento ilegível; nada foi feito"));
        return;
    };
    com_estado(|e| {
        let mut campos = vec![("handle", handle.to_string()), ("tamA", a.len().to_string()), ("nK", (k.len() / 8).to_string())];
        let Some(&sid) = e.steam_do_handle.get(&handle) else {
            campos.push(("decisao", texto("sem_steamid")));
            registro::registrar("entrada", &campos);
            registro::log(t("A player joined before their Steam ID was known; nothing was done",
                "Um jogador entrou antes de o Steam ID dele ser conhecido; nada foi feito"));
            return;
        };
        campos.push(("steamid", texto(&sid.to_string())));
        let dir = registro::dados().join(sid.to_string());
        let trazido = [a.as_slice(), k.as_slice()].concat();
        // the disk wins: picks up edits made from outside while the server runs
        let do_disco = ler_do_disco(sid);
        campos.push(("referencia", texto(if do_disco.is_some() { "disco" } else { "memoria" })));
        let referencia = do_disco.or_else(|| e.copias.get(&sid).cloned());
        if let Some(c) = &referencia {
            e.copias.insert(sid, c.clone());
        }
        let carimbo = registro::carimbo();
        if referencia.is_some() {
            guardar_historico(&dir, &carimbo);
        }
        let jogador = format!("{} {sid}", t("Player", "Jogador"));
        let decisao = match referencia.as_ref() {
            None => {
                let _ = fs::create_dir_all(&dir);
                let _ = fs::write(dir.join(format!("{carimbo}_primeira_entrada.bin")), &trazido);
                registro::log(&format!("{jogador} {}", t(
                    "joined for the first time: this character is now the reference copy",
                    "entrou pela primeira vez: este personagem virou a cópia de referência")));
                "primeira_entrada"
            }
            Some(c) if c.a == a && c.k == k => {
                registro::log(&format!("{jogador} {}", t(
                    "joined: character matches the server copy", "entrou: personagem igual à cópia do servidor")));
                "igual"
            }
            Some(c) => {
                campos.push(("guardadoTamA", c.a.len().to_string()));
                campos.push(("guardadoNK", (c.k.len() / 8).to_string()));
                let rec = dir.join("recusados");
                let _ = fs::create_dir_all(&rec);
                // The stored copy is overwritten on every server save, so the reference of this moment is
                // kept here next to the rejected save. Without it, comparing an old rejection only shows
                // what the player did afterwards, not what they brought in.
                let referencia = [c.a.as_slice(), c.k.as_slice()].concat();
                let _ = fs::write(rec.join(format!("{carimbo}_referencia.bin")), &referencia);
                let dif = diferenca(c, &a, &k);
                // Never give back a copy that may be older than the real character: in these two cases the
                // incoming character is accepted and becomes the new reference on the next save.
                if c.desatualizada {
                    let _ = fs::write(rec.join(format!("{carimbo}_copia_desatualizada.bin")), &trazido);
                    registro::log(&format!("{jogador} {} ({dif})", t(
                        "joined with a different character, but their copy is from before a period when the guardian was not running: keeping their current character, the copy will be refreshed",
                        "entrou com um personagem diferente, mas a cópia dele é de antes de um período em que o guardião não estava rodando: o personagem atual fica, e a cópia será renovada")));
                    "copia_desatualizada"
                } else if c.build != e.build {
                    let _ = fs::write(rec.join(format!("{carimbo}_versao_do_jogo_mudou.bin")), &trazido);
                    registro::log(&format!("{jogador} {} ({dif})", t(
                        "joined with a different character, but their copy was saved by a previous game version: keeping their current character, the copy will be refreshed",
                        "entrou com um personagem diferente, mas a cópia dele foi gravada por uma versão anterior do jogo: o personagem atual fica, e a cópia será renovada")));
                    "versao_do_jogo_mudou"
                } else if !e.aplicar {
                    let _ = fs::write(rec.join(format!("{carimbo}_divergente_observado.bin")), &trazido);
                    registro::log(&format!("{jogador} {} ({dif})", t(
                        "joined with a MODIFIED character. Observe mode: nothing was changed, the evidence is in recusados/",
                        "entrou com um personagem MODIFICADO. Modo observar: nada foi alterado, a prova está em recusados/")));
                    "divergente_observado"
                } else {
                    let r = gravar_array(st.add(0x18), &c.a, 1).and_then(|_| gravar_k(st, &c.k, c.k_estado));
                    let conferido = r.is_ok() && ler_array(st.add(0x18), 1) == c.a && ler_k(st).as_deref() == Some(&c.k[..]);
                    let _ = fs::write(rec.join(format!("{carimbo}_substituido.bin")), &trazido);
                    if let Err(m) = r {
                        registro::log(&format!("{jogador}: {} {m}", t("replacing the save failed:", "a troca do save falhou:")));
                    }
                    if conferido {
                        e.forcar_completo.insert(handle);
                        registro::log(&format!("{jogador} {} ({dif})", t(
                            "joined with a MODIFIED character: it was replaced by the server copy. The rejected save is in recusados/",
                            "entrou com um personagem MODIFICADO: ele foi trocado pela cópia do servidor. O save recusado está em recusados/")));
                        "substituido"
                    } else {
                        registro::log(&format!("{jogador} {} ({dif})", t(
                            "joined with a MODIFIED character, but the replacement did NOT stick: the player kept the modified character",
                            "entrou com um personagem MODIFICADO, mas a troca NÃO pegou: o jogador ficou com o personagem modificado")));
                        "substituicao_falhou"
                    }
                }
            }
        };
        campos.push(("decisao", texto(decisao)));
        registro::registrar("entrada", &campos);
    });
}
