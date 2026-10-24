//! Acha os pontos de ligação pelo formato do código, não por endereço fixo, para sobreviver a
//! atualizações do jogo. Cada busca precisa dar exatamente um resultado, senão a instalação é abortada.
//!
//! Pontos:
//! - entrada: o `call` ao leitor do save, logo antes da mensagem "[save2] Failed to read client savegame for"
//! - saida:   os dois `lea r8, [rip+d]` que apontam para a função que monta a mensagem de save
//! - sessao:  o `call` ao log, logo depois da mensagem "[session] Remote player added. Player handle"

use core::slice;

pub struct Secao {
    pub base: usize,
    pub tam: usize,
}

pub struct Imagem {
    pub texto: Secao,
    pub todas: Secao, // imagem inteira, para procurar as mensagens em qualquer seção de dados
}

/// Prólogo da função que monta a mensagem de save (igual nos builds 1269579 e 1273043).
/// Termina antes do `call` relativo, cujo deslocamento muda entre builds.
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
    let primeiro = achados.next().ok_or("padrão não encontrado")?;
    if achados.next().is_some() {
        return Err("padrão encontrado mais de uma vez".into());
    }
    Ok(primeiro)
}

/// Endereços das instruções `lea reg, [rip+disp32]` (48/4c 8d /r, mod=00 rm=101) que apontam para `alvo`.
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

/// Destino absoluto do `call rel32` em `ins`, se houver um ali e apontar para dentro do código.
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

/// Primeiro `call rel32` numa janela antes (`para_tras`) ou depois do endereço dado.
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
    acha_unico(msg, todas, img.todas.base).map_err(|e| format!("mensagem: {e}"))
}

pub fn achar(img: &Imagem) -> Result<Pontos, String> {
    // entrada: lea da mensagem -> primeiro call antes dela
    let msg = acha_mensagem(img, MSG_ENTRADA)?;
    let refs = leas_para(img, msg, false);
    if refs.len() != 1 {
        return Err(format!("mensagem de entrada referenciada {} vezes (esperado 1)", refs.len()));
    }
    let entrada_call = call_proximo(img, refs[0], 80, true).ok_or("call do leitor não achado")?;
    let entrada_alvo = destino_call(img, entrada_call).unwrap();

    // saida: prólogo do montador -> os dois lea r8 que o apontam
    let t = unsafe { bytes(&img.texto) };
    let saida_alvo =
        acha_unico(PROLOGO_MONTADOR, t, img.texto.base).map_err(|e| format!("montador: {e}"))?;
    let leas = leas_para(img, saida_alvo, true);
    if leas.len() != 2 {
        return Err(format!("montador referenciado por {} lea r8 (esperado 2)", leas.len()));
    }

    // sessao: lea da mensagem -> primeiro call depois dela
    let msg = acha_mensagem(img, MSG_SESSAO)?;
    let refs = leas_para(img, msg, false);
    if refs.len() != 1 {
        return Err(format!("mensagem de sessão referenciada {} vezes (esperado 1)", refs.len()));
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
