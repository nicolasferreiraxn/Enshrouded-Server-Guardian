//! Finds the hook points (achar.rs) and redirects four instructions to the guardian's functions.
//! If any check fails, nothing is written and the server carries on without the guardian.

use crate::achar::{self, Imagem, Secao};
use crate::registro::t;
use crate::{atualizacao, continuidade, estado, registro};
use core::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{
    VirtualAlloc, VirtualProtect, MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

type F4 = unsafe extern "system" fn(usize, usize, usize, usize) -> usize;

static ORIG_ENTRADA: AtomicUsize = AtomicUsize::new(0);
static ORIG_SAIDA: AtomicUsize = AtomicUsize::new(0);
static ORIG_SESSAO: AtomicUsize = AtomicUsize::new(0);

unsafe fn orig(a: &AtomicUsize) -> F4 {
    core::mem::transmute::<usize, F4>(a.load(Ordering::Acquire))
}

/// Reads the client's save (rdx = state), then compares and replaces it if needed.
unsafe extern "system" fn gancho_entrada(rcx: usize, rdx: usize, r8: usize, r9: usize) -> usize {
    let r = orig(&ORIG_ENTRADA)(rcx, rdx, r8, r9);
    estado::entrada(rdx as *mut u8);
    r
}

/// Task that builds the save sent to the client (rcx = pointer to the state pointer).
unsafe extern "system" fn gancho_saida(rcx: usize, rdx: usize, r8: usize, r9: usize) -> usize {
    let st = if rcx != 0 { *(rcx as *const *mut u8) } else { core::ptr::null_mut() };
    let forcado = estado::saida_antes(st);
    let r = orig(&ORIG_SAIDA)(rcx, rdx, r8, r9);
    if let Some(h) = forcado {
        estado::saida_depois(st, h);
    }
    r
}

/// "Remote player added" log: r8 points to the arguments; the third qword is the session record.
unsafe extern "system" fn gancho_sessao(rcx: usize, rdx: usize, r8: usize, r9: usize) -> usize {
    if r8 != 0 {
        estado::sessao(*((r8 + 0x10) as *const *const u8));
    }
    orig(&ORIG_SESSAO)(rcx, rdx, r8, r9)
}

/// Sections of the loaded executable, read from the in-memory PE header.
unsafe fn imagem(base: usize) -> Result<Imagem, String> {
    let e_lfanew = *((base + 0x3c) as *const u32) as usize;
    let nt = base + e_lfanew;
    if *(nt as *const u32) != 0x0000_4550 {
        return Err("PE header not found".into());
    }
    let n_secoes = *((nt + 6) as *const u16) as usize;
    let tam_opcional = *((nt + 20) as *const u16) as usize;
    let tam_imagem = *((nt + 24 + 56) as *const u32) as usize; // SizeOfImage
    let mut secoes = nt + 24 + tam_opcional;
    for _ in 0..n_secoes {
        let nome = core::slice::from_raw_parts(secoes as *const u8, 8);
        if nome.starts_with(b".text") {
            let tam = *((secoes + 8) as *const u32) as usize; // VirtualSize
            let rva = *((secoes + 12) as *const u32) as usize;
            return Ok(Imagem {
                texto: Secao { base: base + rva, tam },
                todas: Secao { base, tam: tam_imagem },
            });
        }
        secoes += 40;
    }
    Err(".text section not found".into())
}

/// Executable block within 2 GB of `perto`, for the absolute jumps.
unsafe fn bloco_perto(perto: usize) -> Option<usize> {
    let mut a = perto.saturating_sub(0x1000_0000) & !0xffff;
    let limite = perto.saturating_sub(0x7000_0000);
    while a > limite {
        let p = VirtualAlloc(a as *const c_void, 0x1000, MEM_RESERVE | MEM_COMMIT, PAGE_EXECUTE_READWRITE);
        if !p.is_null() {
            return Some(p as usize);
        }
        a -= 0x10000;
    }
    None
}

/// Writes `jmp [rip+0]; dq alvo` into the slot and returns the slot address.
unsafe fn salto(slot: usize, alvo: usize) -> usize {
    let p = slot as *mut u8;
    core::ptr::copy_nonoverlapping([0xffu8, 0x25, 0, 0, 0, 0].as_ptr(), p, 6);
    core::ptr::write_unaligned(p.add(6) as *mut usize, alvo);
    slot
}

/// Rewrites the displacement (last 4 bytes of the instruction) to point to `novo`.
unsafe fn trocar_destino(ins: usize, tam: usize, novo: usize) -> bool {
    let campo = ins + tam - 4;
    let disp = novo as isize - (ins + tam) as isize;
    if disp < i32::MIN as isize || disp > i32::MAX as isize {
        return false;
    }
    let mut antigo = 0u32;
    if VirtualProtect(campo as *const c_void, 4, PAGE_EXECUTE_READWRITE, &mut antigo) == 0 {
        return false;
    }
    core::ptr::write_unaligned(campo as *mut i32, disp as i32);
    let mut x = 0u32;
    VirtualProtect(campo as *const c_void, 4, antigo, &mut x);
    FlushInstructionCache(GetCurrentProcess(), ins as *const c_void, tam);
    true
}

const VERSAO: &str = env!("CARGO_PKG_VERSION");

/// Executable timestamp (TimeDateStamp of the PE header): changes with every server build.
unsafe fn build_do_servidor(base: usize) -> String {
    let nt = base + *((base + 0x3c) as *const u32) as usize;
    format!("{:08x}", *((nt + 8) as *const u32))
}

/// Without its hooks the guardian is blind: it sees neither what comes in nor what gets saved. Says so
/// clearly, marks every copy (when it's back, none will be used to undo progress made in the
/// meantime) and installs nothing.
fn desligado(motivo: &str) {
    let n = continuidade::marcar_todas();
    registro::registrar("desligado", &[("motivo", registro::texto(motivo)), ("copiasMarcadas", n.to_string())]);
    registro::log(&format!("{} {motivo}", t("Could not hook into this server build:", "Não consegui instalar os ganchos neste build do servidor:")));
    registro::log(t(
        "The guardian is OFF. It is not observing or changing anything: the server runs exactly as it would without the mod, and players are NOT protected until the guardian is updated.",
        "O guardião está DESLIGADO. Ele não observa nem altera nada: o servidor roda exatamente como sem o mod, e os jogadores NÃO estão protegidos até o guardião ser atualizado."));
    registro::log(t(
        "No progress will be lost: every stored copy was marked as outdated, so when the guardian works again it will refresh each player's copy instead of restoring an old one.",
        "Nenhum progresso será perdido: todas as cópias foram marcadas como desatualizadas, então quando o guardião voltar a funcionar ele vai renovar a cópia de cada jogador em vez de restaurar uma antiga."));
    registro::log(t(
        "Please report it: https://github.com/nicolasferreiraxn/Enshrouded-Server-Guardian/issues or Discord nicolas.f2569",
        "Por favor, avise: https://github.com/nicolasferreiraxn/Enshrouded-Server-Guardian/issues ou Discord nicolas.f2569"));
}

pub fn instalar() {
    let Some(exe) = std::env::current_exe().ok() else { return };
    let Some(dir) = exe.parent() else { return };
    registro::definir_raiz(dir);
    // console output waits for the world to finish loading, so it lands after the game's boot log
    registro::liberar_quando_pronto();
    let aplicar = registro::ler_config();

    unsafe {
        let base = GetModuleHandleW(core::ptr::null()) as usize;
        if base == 0 {
            return desligado(t("could not find the server executable in memory", "não achei o executável do servidor na memória"));
        }
        let build = build_do_servidor(base);
        // boot banner, so the guardian stands out in a busy server console (figlet "slant")
        let assinatura = format!("Enshrouded Server Guardian v{VERSAO} · github.com/nicolasferreiraxn/Enshrouded-Server-Guardian");
        registro::console(&[
            r"   ______                     ___",
            r"  / ____/_  ______ __________/ (_)___ _____",
            r" / / __/ / / / __ `/ ___/ __  / / __ `/ __ \",
            r"/ /_/ / /_/ / /_/ / /  / /_/ / / /_/ / / / /",
            r"\____/\__,_/\__,_/_/   \__,_/_/\__,_/_/ /_/",
            &assinatura,
        ]);
        registro::log(&format!("Enshrouded Server Guardian v{VERSAO} {} ({} {build})",
            t("loaded", "carregado"), t("server build", "build do servidor")));
        // also when the hooks fail: that's exactly when a new version may fix it
        if registro::verificar_atualizacao() {
            atualizacao::verificar(VERSAO);
        }
        // before any hook: did the server run without the guardian since last time?
        continuidade::conferir();

        let img = match imagem(base) {
            Ok(i) => i,
            Err(e) => return desligado(&e),
        };
        let p = match achar::achar(&img) {
            Ok(p) => p,
            Err(e) => return desligado(&e),
        };
        // addresses only in guardiao.log (useful in bug reports); the console just says the hooks are installed
        registro::log_arquivo(&format!(
            "hooks found: join {:#x}->{:#x} | save {:#x},{:#x}->{:#x} | session {:#x}->{:#x}",
            p.entrada_call, p.entrada_alvo, p.saida_lea[0], p.saida_lea[1], p.saida_alvo,
            p.sessao_call, p.sessao_alvo
        ));

        let Some(bloco) = bloco_perto(base) else {
            return desligado(t("no free memory near the server executable", "sem memória livre perto do executável do servidor"));
        };
        ORIG_ENTRADA.store(p.entrada_alvo, Ordering::Release);
        ORIG_SAIDA.store(p.saida_alvo, Ordering::Release);
        ORIG_SESSAO.store(p.sessao_alvo, Ordering::Release);

        let n = estado::iniciar(aplicar, build);

        let s_entrada = salto(bloco, gancho_entrada as *const () as usize);
        let s_saida = salto(bloco + 16, gancho_saida as *const () as usize);
        let s_sessao = salto(bloco + 32, gancho_sessao as *const () as usize);
        FlushInstructionCache(GetCurrentProcess(), bloco as *const c_void, 48);
        // session first: join and save depend on the steamid it records
        let ok = trocar_destino(p.sessao_call, 5, s_sessao)
            && trocar_destino(p.saida_lea[0], 7, s_saida)
            && trocar_destino(p.saida_lea[1], 7, s_saida)
            && trocar_destino(p.entrada_call, 5, s_entrada);
        if !ok {
            return desligado(t("one of the hooks could not be written", "um dos ganchos não pôde ser escrito"));
        }
        continuidade::sinal_de_vida(VERSAO);
        registro::log(&format!("{} {n}", t("Active (hooks installed). Player copies loaded:", "Ativo (ganchos instalados). Cópias de jogadores carregadas:")));
        registro::log(if aplicar {
            t("Mode: ENFORCE. A player who joins with a character different from the server copy gets the server copy back.",
              "Modo: APLICAR. Um jogador que entrar com um personagem diferente da cópia do servidor recebe a cópia do servidor de volta.")
        } else {
            t("Mode: OBSERVE. Differences are only logged; nothing is ever replaced.",
              "Modo: OBSERVAR. As diferenças só são registradas; nada é trocado.")
        });
    }
}
