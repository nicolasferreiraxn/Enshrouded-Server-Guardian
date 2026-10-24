//! Arquivos do guardião ao lado do executável: guardiao.json, guardiao-dados/ (cópias, registro.jsonl, guardiao.log).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static RAIZ: OnceLock<PathBuf> = OnceLock::new();

pub fn definir_raiz(dir_exe: &Path) {
    let _ = RAIZ.set(dir_exe.to_path_buf());
    let _ = fs::create_dir_all(dados());
}

pub fn raiz() -> PathBuf {
    RAIZ.get().cloned().unwrap_or_else(|| PathBuf::from("."))
}

pub fn dados() -> PathBuf {
    raiz().join("guardiao-dados")
}

/// "2026-10-20T14:01:46Z" sem depender de bibliotecas de data.
pub fn agora() -> String {
    let s = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (dias, resto) = (s.div_euclid(86400), s.rem_euclid(86400));
    // civil_from_days (Howard Hinnant)
    let z = dias + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{a:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", resto / 3600, resto / 60 % 60, resto % 60)
}

pub fn carimbo() -> String {
    agora().replace(['-', ':'], "").replace('T', "_").trim_end_matches('Z').to_string()
}

pub fn log(msg: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(dados().join("guardiao.log")) {
        let _ = writeln!(f, "{} {}", agora(), msg);
    }
}

/// Uma linha de registro.jsonl. `campos` já vem como pares "chave": valor JSON.
pub fn registrar(tipo: &str, campos: &[(&str, String)]) {
    let mut linha = format!("{{\"kind\": \"{tipo}\", \"quando\": \"{}\"", agora());
    for (k, v) in campos {
        linha.push_str(&format!(", \"{k}\": {v}"));
    }
    linha.push('}');
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(dados().join("registro.jsonl")) {
        let _ = writeln!(f, "{linha}");
    }
}

pub fn texto(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn gravar_atomico(caminho: &Path, dados: &[u8]) -> std::io::Result<()> {
    let tmp = caminho.with_extension("tmp");
    fs::write(&tmp, dados)?;
    fs::rename(&tmp, caminho)
}

/// Modo de guardiao.json: {"modo": "observar"} ou "aplicar" (padrão).
pub fn ler_modo() -> bool {
    match fs::read_to_string(raiz().join("guardiao.json")) {
        Ok(s) => !s.replace(' ', "").contains("\"modo\":\"observar\""),
        Err(_) => true,
    }
}
