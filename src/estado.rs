//! Lógica do guardião: cópias por steamid, comparação na entrada, troca e envio completo na saída.
//! Offsets do estado do save e do registro da sessão.

use crate::registro::{self, texto};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::{Mutex, OnceLock};

#[derive(Clone)]
pub struct Copia {
    pub a: Vec<u8>,
    pub k: Vec<u8>, // n ids u32 seguidos de n valores u32
    pub k_estado: u8,
}

/// Lê a cópia de um jogador do disco. O arquivo é a autoridade: assim uma edição feita por fora
/// (pelo editor de saves) passa a valer na próxima entrada, sem reiniciar o servidor.
fn ler_do_disco(sid: u64) -> Option<Copia> {
    let dir = registro::dados().join(sid.to_string());
    let a = fs::read(dir.join("A.bin")).ok()?;
    let k = fs::read(dir.join("K.bin")).ok()?;
    if a.is_empty() || k.len() % 8 != 0 {
        return None;
    }
    let k_estado = fs::read_to_string(dir.join("meta.json"))
        .ok()
        .and_then(|m| {
            m.split("\"kEstado\":")
                .nth(1)?
                .trim()
                .split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse()
                .ok()
        })
        .unwrap_or(1);
    Some(Copia { a, k, k_estado })
}

pub struct Estado {
    pub aplicar: bool,
    pub copias: HashMap<u64, Copia>,
    pub steam_do_handle: HashMap<u32, u64>,
    pub forcar_completo: HashSet<u32>,
}

static ESTADO: OnceLock<Mutex<Estado>> = OnceLock::new();

pub fn iniciar(aplicar: bool) -> usize {
    let mut copias = HashMap::new();
    if let Ok(dirs) = fs::read_dir(registro::dados()) {
        for d in dirs.flatten() {
            let Ok(sid) = d.file_name().to_string_lossy().parse::<u64>() else { continue };
            let p = d.path();
            let (Ok(a), Ok(k), Ok(meta)) = (fs::read(p.join("A.bin")), fs::read(p.join("K.bin")), fs::read_to_string(p.join("meta.json"))) else { continue };
            let k_estado = meta.split("\"kEstado\":").nth(1)
                .and_then(|s| s.trim().split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()).unwrap_or(1);
            copias.insert(sid, Copia { a, k, k_estado });
        }
    }
    let n = copias.len();
    let _ = ESTADO.set(Mutex::new(Estado { aplicar, copias, steam_do_handle: HashMap::new(), forcar_completo: HashSet::new() }));
    n
}

fn com_estado<R>(f: impl FnOnce(&mut Estado) -> R) -> Option<R> {
    let m = ESTADO.get()?;
    let mut g = m.lock().unwrap_or_else(|e| e.into_inner());
    Some(f(&mut g))
}

// ---------------------------------------------------------------- memória do servidor

unsafe fn rd<T: Copy>(p: *const u8, off: usize) -> T {
    core::ptr::read_unaligned(p.add(off) as *const T)
}

unsafe fn wr<T: Copy>(p: *mut u8, off: usize, v: T) {
    core::ptr::write_unaligned(p.add(off) as *mut T, v)
}

type Cresce = unsafe extern "system" fn(*mut u8, u32, u32) -> u8;

/// Array do motor em `base`: {ptr +0, tamanho +8, capacidade +0x10, função de crescimento +0x18}.
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

// ---------------------------------------------------------------- pontos de ligação

/// Log "Remote player added": registro da sessão com handle em +0 e steamid em +0x60.
pub unsafe fn sessao(registro_sessao: *const u8) {
    if registro_sessao.is_null() {
        return;
    }
    let handle: u32 = rd(registro_sessao, 0);
    let sid: u64 = rd(registro_sessao, 0x60);
    com_estado(|e| e.steam_do_handle.insert(handle, sid));
    registro::registrar("sessao", &[("handle", handle.to_string()), ("steamid", texto(&sid.to_string()))]);
}

/// Antes da tarefa de envio. Devolve o handle se o envio foi forçado a ir completo.
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
            registro::log(&format!("save de saída do handle {handle} sem steamid conhecido"));
            return forcado.then_some(handle);
        };
        if !a.is_empty() {
            let dir = registro::dados().join(sid.to_string());
            let _ = fs::create_dir_all(&dir);
            let ok = registro::gravar_atomico(&dir.join("A.bin"), &a).is_ok()
                && registro::gravar_atomico(&dir.join("K.bin"), &k).is_ok()
                && registro::gravar_atomico(&dir.join("meta.json"), format!(
                    "{{\"steamid\": \"{sid}\", \"handle\": {handle}, \"versao\": {versao}, \"final\": {final_}, \"tamA\": {}, \"nK\": {}, \"kEstado\": {k_estado}, \"quando\": \"{}\"}}",
                    a.len(), k.len() / 8, registro::agora()).as_bytes()).is_ok();
            if !ok {
                registro::log(&format!("falha ao gravar a cópia de {sid}"));
            }
            if final_ != 0 {
                registro::registrar("guardado", &[("steamid", texto(&sid.to_string())), ("versao", versao.to_string()),
                    ("tamA", a.len().to_string()), ("nK", (k.len() / 8).to_string())]);
            }
            e.copias.insert(sid, Copia { a, k, k_estado });
        }
        forcado.then_some(handle)
    }).flatten()
}

/// Depois da tarefa de envio forçada: +0xf8 = 1 se o conhecimento foi completo.
pub unsafe fn saida_depois(st: *const u8, handle: u32) {
    let completo: u8 = rd(st, 0xf8);
    if completo != 0 {
        com_estado(|e| e.forcar_completo.remove(&handle));
    }
    registro::registrar("envio_completo", &[("handle", handle.to_string()), ("completo", completo.to_string())]);
}

/// Depois que o servidor leu o save do cliente (estado em `st`).
pub unsafe fn entrada(st: *mut u8) {
    if st.is_null() {
        return;
    }
    let handle: u32 = rd(st, 8);
    let a = ler_array(st.add(0x18), 1);
    let Some(k) = ler_k(st) else {
        registro::log("entrada com listas de conhecimento de tamanhos diferentes; nada feito");
        return;
    };
    com_estado(|e| {
        let mut campos = vec![("handle", handle.to_string()), ("tamA", a.len().to_string()), ("nK", (k.len() / 8).to_string())];
        let Some(&sid) = e.steam_do_handle.get(&handle) else {
            campos.push(("decisao", texto("sem_steamid")));
            registro::registrar("entrada", &campos);
            return;
        };
        campos.push(("steamid", texto(&sid.to_string())));
        let dir = registro::dados().join(sid.to_string());
        let trazido = [a.as_slice(), k.as_slice()].concat();
        // o disco manda: pega edições feitas por fora enquanto o servidor roda
        let do_disco = ler_do_disco(sid);
        campos.push(("referencia", texto(if do_disco.is_some() { "disco" } else { "memoria" })));
        let referencia = do_disco.or_else(|| e.copias.get(&sid).cloned());
        if let Some(c) = &referencia {
            e.copias.insert(sid, c.clone());
        }
        let decisao = match referencia.as_ref() {
            None => {
                let _ = fs::create_dir_all(&dir);
                let _ = fs::write(dir.join(format!("{}_primeira_entrada.bin", registro::carimbo())), &trazido);
                "primeira_entrada"
            }
            Some(c) if c.a == a && c.k == k => "igual",
            Some(c) => {
                campos.push(("guardadoTamA", c.a.len().to_string()));
                campos.push(("guardadoNK", (c.k.len() / 8).to_string()));
                let rec = dir.join("recusados");
                let _ = fs::create_dir_all(&rec);
                let carimbo = registro::carimbo();
                // A cópia guardada é sobrescrita a cada save do servidor, então guardamos aqui a
                // referência deste instante junto do save recusado. Sem ela, comparar uma recusa
                // antiga só mostra o que o jogador fez depois, e não o que ele trouxe.
                let referencia = [c.a.as_slice(), c.k.as_slice()].concat();
                let _ = fs::write(rec.join(format!("{carimbo}_referencia.bin")), &referencia);
                if !e.aplicar {
                    let _ = fs::write(rec.join(format!("{carimbo}_divergente_observado.bin")), &trazido);
                    "divergente_observado"
                } else {
                    let r = gravar_array(st.add(0x18), &c.a, 1).and_then(|_| gravar_k(st, &c.k, c.k_estado));
                    let conferido = r.is_ok() && ler_array(st.add(0x18), 1) == c.a && ler_k(st).as_deref() == Some(&c.k[..]);
                    let _ = fs::write(rec.join(format!("{carimbo}_substituido.bin")), &trazido);
                    if let Err(m) = r {
                        registro::log(&format!("troca do save de {sid} falhou: {m}"));
                    }
                    if conferido {
                        e.forcar_completo.insert(handle);
                        "substituido"
                    } else {
                        "substituicao_falhou"
                    }
                }
            }
        };
        campos.push(("decisao", texto(decisao)));
        registro::registrar("entrada", &campos);
    });
}
