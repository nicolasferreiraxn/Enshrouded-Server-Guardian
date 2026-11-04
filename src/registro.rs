//! Guardian files next to the executable: guardiao.json, guardiao-dados/ (copies, registro.jsonl, guardiao.log).
//! Also the configuration (mode and language) and the messages, which go to guardiao.log and to the server console.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static RAIZ: OnceLock<PathBuf> = OnceLock::new();
static PORTUGUES: AtomicBool = AtomicBool::new(false);
static VERIFICAR: AtomicBool = AtomicBool::new(true);

/// Whether the new-version check is on (the default) in guardiao.json.
pub fn verificar_atualizacao() -> bool {
    VERIFICAR.load(Ordering::Relaxed)
}

/// Text in the language chosen in guardiao.json. English is the default.
pub fn t<'a>(en: &'a str, pt: &'a str) -> &'a str {
    if PORTUGUES.load(Ordering::Relaxed) { pt } else { en }
}

pub fn definir_raiz(dir_exe: &Path) {
    let _ = RAIZ.set(dir_exe.to_path_buf());
    let _ = fs::create_dir_all(dados());
    // BisectHosting's DLL loader writes DLLMods/loader.log and its panel follows that file live, while the
    // server's own stdout never reaches the panel console. When the file is there, mirror our lines into it.
    let loader = dir_exe.join("DLLMods").join("loader.log");
    let _ = ESPELHO.set(loader.is_file().then_some(loader));
}

/// Extra file that receives a copy of every log line (only on hosts whose panel shows it, see definir_raiz).
static ESPELHO: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn raiz() -> PathBuf {
    RAIZ.get().cloned().unwrap_or_else(|| PathBuf::from("."))
}

pub fn dados() -> PathBuf {
    raiz().join("guardiao-dados")
}

/// "2026-10-20T14:01:46Z" without depending on date libraries.
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

/// Console lines held back while the server is still loading (None once released): printed at boot they get
/// buried under hundreds of game lines, so the guardian's block shows up after the world is loaded instead.
static RETIDO: Mutex<Option<Vec<String>>> = Mutex::new(Some(Vec::new()));

/// Prints to the server console and to the host panel's mirror file.
fn emitir(linhas: &[String]) {
    // writeln! and not println!: without a console println! panics, and with panic=abort that would take the server down
    let mut saida = std::io::stdout().lock();
    for l in linhas {
        let _ = writeln!(saida, "[Guardian] {l}");
    }
    let _ = saida.flush();
    if let Some(Some(espelho)) = ESPELHO.get() {
        // append only: the panel reads the file from where it stopped, so rewriting it would garble the console
        if let Ok(mut f) = OpenOptions::new().append(true).open(espelho) {
            for l in linhas {
                let _ = write!(f, "[Guardian] {l}\r\n");
            }
        }
    }
}

/// Lines shown only where a person watches the server (console and the panel's mirror), not in guardiao.log:
/// the boot banner would otherwise repeat in the history on every restart.
pub fn console(linhas: &[&str]) {
    let linhas: Vec<String> = linhas.iter().map(|l| l.to_string()).collect();
    let mut retido = RETIDO.lock().unwrap_or_else(|e| e.into_inner());
    match retido.as_mut() {
        Some(fila) => fila.extend(linhas),
        None => {
            drop(retido);
            emitir(&linhas);
        }
    }
}

/// One line in guardiao.log (right away: it's the history) and in the server console (held back during boot).
pub fn log(msg: &str) {
    log_arquivo(msg);
    console(&[msg]);
}

/// One line in guardiao.log only: technical details that mean nothing in a server console.
pub fn log_arquivo(msg: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(dados().join("guardiao.log")) {
        let _ = writeln!(f, "{} {}", agora(), msg);
    }
}

/// Prints everything held back and from then on prints right away.
fn liberar_console() {
    let fila = RETIDO.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(fila) = fila {
        emitir(&fila);
    }
}

/// Seconds since 1970 of a "YYYY.MM.DD HH:MM:SS" UTC date (days_from_civil, Howard Hinnant).
fn segundos_utc(data: &str) -> Option<i64> {
    let n: Vec<i64> = data.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(-1)).collect();
    let [a, m, d, h, mi, s] = n[..] else { return None };
    let a = if m <= 2 { a - 1 } else { a };
    let era = a.div_euclid(400);
    let yoe = a - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + h * 3600 + mi * 60 + s)
}

/// Releases the held-back console lines once the game's own log says the world finished loading
/// ("Switching state from LoadEcsScene to Run"), or after 3 minutes whatever happens. The log file still holds
/// the previous session while the new one starts, so it only counts once its "Current UTC date" is this boot's.
pub fn liberar_quando_pronto() {
    let inicio = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let json = fs::read_to_string(raiz().join("enshrouded_server.json")).unwrap_or_default();
    let pasta = json.find("\"logDirectory\"").and_then(|i| {
        let resto = json[i + 14..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
        Some(resto[..resto.find('"')?].to_string())
    }).unwrap_or_else(|| "./logs".into());
    let arquivo = raiz().join(pasta).join("enshrouded_server.log");
    std::thread::spawn(move || {
        for _ in 0..90 {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let Ok(b) = fs::read(&arquivo) else { continue };
            let texto = String::from_utf8_lossy(&b);
            let desta_sessao = texto.split("Current UTC date: ").nth(1)
                .and_then(|r| segundos_utc(r.lines().next()?))
                .is_some_and(|t| (t - inicio).abs() < 600);
            if desta_sessao && texto.contains("Switching state from LoadEcsScene to Run") {
                break;
            }
        }
        liberar_console();
    });
}

/// One line of registro.jsonl. `campos` already comes as "key": JSON value pairs.
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

/// Text value of a top-level key in a simple JSON like guardiao.json.
fn campo(json: &str, chave: &str) -> Option<String> {
    let resto = &json[json.find(&format!("\"{chave}\""))? + chave.len() + 2..];
    let resto = resto.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(resto[..resto.find('"')?].trim().to_lowercase())
}

/// Reads guardiao.json, accepting both English and Portuguese names:
/// {"mode": "observe"} or {"modo": "observar"}; {"language": "pt-BR"} or {"idioma": "pt-BR"}.
/// No file or no mode means enforce. Returns whether to enforce. {"checkUpdates": false} turns off the version check.
pub fn ler_config() -> bool {
    let json = fs::read_to_string(raiz().join("guardiao.json")).unwrap_or_default();
    let idioma = campo(&json, "language").or_else(|| campo(&json, "idioma")).unwrap_or_default();
    PORTUGUES.store(idioma.starts_with("pt"), Ordering::Relaxed);
    let desligada = |chave: &str| json.find(&format!("\"{chave}\"")).is_some_and(|i| {
        json[i + chave.len() + 2..].trim_start().strip_prefix(':').is_some_and(|r| r.trim_start().starts_with("false"))
    });
    VERIFICAR.store(!(desligada("checkUpdates") || desligada("verificarAtualizacao")), Ordering::Relaxed);
    let modo = campo(&json, "mode").or_else(|| campo(&json, "modo")).unwrap_or_default();
    !matches!(modo.as_str(), "observe" | "observar")
}
