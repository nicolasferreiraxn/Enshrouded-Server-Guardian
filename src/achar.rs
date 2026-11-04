//! Finds the hook points by the shape of the code, not by fixed address, so they survive game
//! updates. Every search must yield exactly one result, otherwise installation is aborted.
//!
//! Hook points:
//! - entrada (join):   the `call` to the save reader, right before the message "[save2] Failed to read client savegame for"
//! - saida (save):    the two `lea r8, [rip+d]` pointing to the function that builds the save message
//! - sessao (session): the `call` to the logger, right after the message "[session] Remote player added. Player handle"

use core::slice;

pub struct Secao {
    pub base: usize,
    pub tam: usize,
}

pub struct Imagem {
    pub texto: Secao,
    pub todas: Secao, // whole image, to search for the messages in any data section
}

/// Prologue of the function that builds the save message (identical in builds 1269579, 1273043 and 1285601).
/// It stops before the relative `call`, whose displacement changes between builds.
const PROLOGO_MONTADOR: &[u8] = &[
    0x40, 0x55, 0x53, 0x41, 0x56, 0x48, 0x8d, 0xac, 0x24, 0xc0, 0xff, 0xfe, 0xff, 0xb8, 0x40, 0x01,
    0x01, 0x00,
];
const MSG_ENTRADA: &[u8] = b"[save2] Failed to read client savegame for";
const MSG_SESSAO: &[u8] = b"[session] Remote player added. Player handle";

pub struct Pontos {
    pub entrada_call: usize,
    pub entrada_alvo: usize,
    pub saida_lea: [usize; 2],
    pub saida_alvo: usize,
    pub sessao_call: usize,
    pub sessao_alvo: usize,
}

unsafe fn bytes(s: &Secao) -> &'static [u8] {
    slice::from_raw_parts(s.base as *const u8, s.tam)
}

fn acha_unico(agulha: &[u8], palheiro: &[u8], base: usize) -> Result<usize, String> {
    let mut achados = palheiro
        .windows(agulha.len())
        .enumerate()
        .filter(|(_, w)| *w == agulha)
        .map(|(i, _)| base + i);
    let primeiro = achados.next().ok_or("pattern not found")?;
    if achados.next().is_some() {
        return Err("pattern found more than once".into());
    }
    Ok(primeiro)
}

/// Addresses of the `lea reg, [rip+disp32]` instructions (48/4c 8d /r, mod=00 rm=101) that point to `alvo`.
fn leas_para(img: &Imagem, alvo: usize, so_r8: bool) -> Vec<usize> {
    let t = unsafe { bytes(&img.texto) };
    let mut out = Vec::new();
    for i in 0..t.len().saturating_sub(7) {
        let (p, m) = (t[i], t[i + 2]);
        let prefixo_ok = if so_r8 { p == 0x4c } else { p == 0x48 || p == 0x4c };
        let modrm_ok = if so_r8 { m == 0x05 } else { m & 0xc7 == 0x05 };
        if prefixo_ok && t[i + 1] == 0x8d && modrm_ok {
            let disp = i32::from_le_bytes([t[i + 3], t[i + 4], t[i + 5], t[i + 6]]) as isize;
            if (img.texto.base + i + 7).wrapping_add(disp as usize) == alvo {
                out.push(img.texto.base + i);
            }
        }
    }
    out
}

/// Absolute target of the `call rel32` at `ins`, if there is one and it points into the code.
fn destino_call(img: &Imagem, ins: usize) -> Option<usize> {
    let t = unsafe { bytes(&img.texto) };
    let i = ins.checked_sub(img.texto.base)?;
    if i + 5 > t.len() || t[i] != 0xe8 {
        return None;
    }
    let disp = i32::from_le_bytes([t[i + 1], t[i + 2], t[i + 3], t[i + 4]]) as isize;
    let dst = (ins + 5).wrapping_add(disp as usize);
    (img.texto.base..img.texto.base + img.texto.tam).contains(&dst).then_some(dst)
}

/// First `call rel32` in a window before (`para_tras`) or after the given address.
fn call_proximo(img: &Imagem, de: usize, janela: usize, para_tras: bool) -> Option<usize> {
    let passo: isize = if para_tras { -1 } else { 1 };
    let inicio: isize = if para_tras { -5 } else { 7 };
    for k in 0..janela as isize {
        let ins = (de as isize + inicio + passo * k) as usize;
        if destino_call(img, ins).is_some() {
            return Some(ins);
        }
    }
    None
}

fn acha_mensagem(img: &Imagem, msg: &[u8]) -> Result<usize, String> {
    let todas = unsafe { bytes(&img.todas) };
    acha_unico(msg, todas, img.todas.base).map_err(|e| format!("join message: {e}"))
}

pub fn achar(img: &Imagem) -> Result<Pontos, String> {
    // join: lea of the message -> first call before it
    let msg = acha_mensagem(img, MSG_ENTRADA)?;
    let refs = leas_para(img, msg, false);
    if refs.len() != 1 {
        return Err(format!("join message referenced {} times (expected 1)", refs.len()));
    }
    let entrada_call = call_proximo(img, refs[0], 80, true).ok_or("call do leitor não achado")?;
    let entrada_alvo = destino_call(img, entrada_call).unwrap();

    // save: builder prologue -> the two lea r8 pointing to it
    let t = unsafe { bytes(&img.texto) };
    let saida_alvo =
        acha_unico(PROLOGO_MONTADOR, t, img.texto.base).map_err(|e| format!("save builder: {e}"))?;
    let leas = leas_para(img, saida_alvo, true);
    if leas.len() != 2 {
        return Err(format!("save builder referenced by {} lea r8 (expected 2)", leas.len()));
    }

    // session: lea of the message -> first call after it
    let msg = acha_mensagem(img, MSG_SESSAO)?;
    let refs = leas_para(img, msg, false);
    if refs.len() != 1 {
        return Err(format!("session message referenced {} times (expected 1)", refs.len()));
    }
    let sessao_call = call_proximo(img, refs[0], 90, false).ok_or("call do log não achado")?;
    let sessao_alvo = destino_call(img, sessao_call).unwrap();

    Ok(Pontos {
        entrada_call,
        entrada_alvo,
        saida_lea: [leas[0], leas[1]],
        saida_alvo,
        sessao_call,
        sessao_alvo,
    })
}
