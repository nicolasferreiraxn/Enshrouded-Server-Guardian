//! New-version notice. A single request, on load, for the latest release published on GitHub; the
//! result only shows up in the console and in guardiao.log. Nothing is downloaded and nothing about the
//! server is sent. Turned off with {"checkUpdates": false} (or {"verificarAtualizacao": false}) in guardiao.json.

use crate::registro::{self, t};
use core::ptr::null;
use windows_sys::Win32::Networking::WinHttp::*;

const HOST: &str = "api.github.com";
const CAMINHO: &str = "/repos/nicolasferreiraxn/Enshrouded-Server-Guardian/releases/latest";
pub const PAGINA: &str = "https://github.com/nicolasferreiraxn/Enshrouded-Server-Guardian/releases/latest";

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Body of the API response, or None on any failure (no network, blocked host, timeout).
unsafe fn baixar_resposta(versao: &str) -> Option<String> {
    let sessao = WinHttpOpen(w(&format!("Enshrouded-Server-Guardian/{versao}")).as_ptr(),
        WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, null(), null(), 0);
    if sessao.is_null() {
        return None;
    }
    WinHttpSetTimeouts(sessao, 5000, 5000, 5000, 5000);
    let mut corpo = Vec::new();
    let con = WinHttpConnect(sessao, w(HOST).as_ptr(), INTERNET_DEFAULT_HTTPS_PORT as u16, 0);
    let req = if con.is_null() { null_mut_handle() } else {
        WinHttpOpenRequest(con, w("GET").as_ptr(), w(CAMINHO).as_ptr(), null(), null(), null(), WINHTTP_FLAG_SECURE)
    };
    let cabecalho = w("Accept: application/vnd.github+json");
    let ok = !req.is_null()
        && WinHttpSendRequest(req, cabecalho.as_ptr(), u32::MAX, null(), 0, 0, 0) != 0
        && WinHttpReceiveResponse(req, core::ptr::null_mut()) != 0;
    if ok {
        let mut bloco = [0u8; 4096];
        loop {
            let mut lidos = 0u32;
            if WinHttpReadData(req, bloco.as_mut_ptr().cast(), bloco.len() as u32, &mut lidos) == 0 || lidos == 0 {
                break;
            }
            corpo.extend_from_slice(&bloco[..lidos as usize]);
            if corpo.len() > 1 << 20 {
                break;
            }
        }
    }
    for h in [req, con, sessao] {
        if !h.is_null() {
            WinHttpCloseHandle(h);
        }
    }
    (ok && !corpo.is_empty()).then(|| String::from_utf8_lossy(&corpo).into_owned())
}

fn null_mut_handle() -> *mut core::ffi::c_void {
    core::ptr::null_mut()
}

/// "v1.2.0" -> [1, 2, 0]
fn numeros(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches(['v', 'V']).split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// Latest release tag on GitHub, or None on any failure.
fn ultima_versao(versao: &str) -> Option<String> {
    let json = unsafe { baixar_resposta(versao) }?;
    json.split("\"tag_name\"").nth(1).and_then(|r| r.split('"').nth(1)).map(str::to_string)
}

/// Boxed notice, so a new version stands out in a busy server console.
fn anunciar(tag: &str, versao: &str) {
    let linhas = [
        format!("{} {tag}", t("NEW GUARDIAN VERSION AVAILABLE:", "NOVA VERSÃO DO GUARDIÃO DISPONÍVEL:")),
        format!("{} v{versao}", t("This server is running", "Este servidor está rodando a")),
        PAGINA.to_string(),
    ];
    let largura = linhas.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let borda = format!("+{}+", "=".repeat(largura + 4));
    let mut caixa = vec![borda.clone()];
    caixa.extend(linhas.iter().map(|l| format!("|  {l}{}  |", " ".repeat(largura - l.chars().count()))));
    caixa.push(borda);
    let caixa: Vec<&str> = caixa.iter().map(String::as_str).collect();
    registro::console(&caixa);
}

/// On its own thread, so it never delays the server start: checks on boot and then every 24 h while the server
/// runs. Once a newer version is found the checks stop (the guardian already knows) and the notice is repeated
/// in the console every 60 minutes; guardiao.log gets it once.
pub fn verificar(versao: &'static str) {
    std::thread::spawn(move || {
        let mut primeira = true;
        loop {
            match ultima_versao(versao) {
                Some(tag) if numeros(&tag) > numeros(versao) => {
                    registro::log_arquivo(&format!("{} {tag} {} v{versao}): {PAGINA}",
                        t("A new version of the guardian is available:", "Há uma versão nova do guardião:"),
                        t("(you have", "(você tem a")));
                    loop {
                        anunciar(&tag, versao);
                        std::thread::sleep(std::time::Duration::from_secs(60 * 60));
                    }
                }
                // "up to date" only once, on boot; the daily checks stay silent until there is news
                Some(_) if primeira => registro::log(&format!("{} (v{versao})", t("The guardian is up to date", "O guardião está atualizado"))),
                _ => {}
            }
            primeira = false;
            std::thread::sleep(std::time::Duration::from_secs(24 * 60 * 60));
        }
    });
}
