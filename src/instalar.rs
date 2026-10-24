//! Acha os pontos de ligação (achar.rs) e troca o destino de quatro instruções para as funções do guardião.
//! Se qualquer conferência falhar, nada é escrito e o servidor segue sem guardião.

use crate::achar::{self, Imagem, Secao};
use crate::{estado, registro};
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

/// Lê o save do cliente (rdx = estado) e, depois, compara e troca se preciso.
unsafe extern "system" fn gancho_entrada(rcx: usize, rdx: usize, r8: usize, r9: usize) -> usize {
    let r = orig(&ORIG_ENTRADA)(rcx, rdx, r8, r9);
    estado::entrada(rdx as *mut u8);
    r
}

/// Tarefa que monta o save mandado ao cliente (rcx = ponteiro para o ponteiro do estado).
unsafe extern "system" fn gancho_saida(rcx: usize, rdx: usize, r8: usize, r9: usize) -> usize {
    let st = if rcx != 0 { *(rcx as *const *mut u8) } else { core::ptr::null_mut() };
    let forcado = estado::saida_antes(st);
    let r = orig(&ORIG_SAIDA)(rcx, rdx, r8, r9);
    if let Some(h) = forcado {
        estado::saida_depois(st, h);
    }
    r
}

/// Log "Remote player added": r8 aponta para os argumentos; o terceiro qword é o registro da sessão.
unsafe extern "system" fn gancho_sessao(rcx: usize, rdx: usize, r8: usize, r9: usize) -> usize {
    if r8 != 0 {
        estado::sessao(*((r8 + 0x10) as *const *const u8));
    }
    orig(&ORIG_SESSAO)(rcx, rdx, r8, r9)
}

/// Seções do executável carregado, lidas do cabeçalho PE em memória.
unsafe fn imagem(base: usize) -> Result<Imagem, String> {
    let e_lfanew = *((base + 0x3c) as *const u32) as usize;
    let nt = base + e_lfanew;
    if *(nt as *const u32) != 0x0000_4550 {
        return Err("cabeçalho PE não encontrado".into());
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
    Err("seção .text não encontrada".into())
}

/// Bloco executável a menos de 2 GB de `perto`, para os saltos absolutos.
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

/// Escreve `jmp [rip+0]; dq alvo` no slot e devolve o endereço do slot.
unsafe fn salto(slot: usize, alvo: usize) -> usize {
    let p = slot as *mut u8;
    core::ptr::copy_nonoverlapping([0xffu8, 0x25, 0, 0, 0, 0].as_ptr(), p, 6);
    core::ptr::write_unaligned(p.add(6) as *mut usize, alvo);
    slot
}

/// Troca o deslocamento (últimos 4 bytes da instrução) para apontar para `novo`.
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

pub fn instalar() {
    let Some(exe) = std::env::current_exe().ok() else { return };
    let Some(dir) = exe.parent() else { return };
    registro::definir_raiz(dir);
    registro::log("guardião carregado");

    unsafe {
        let base = GetModuleHandleW(core::ptr::null()) as usize;
        if base == 0 {
            return registro::log("não consegui obter a base do executável; guardião desligado");
        }
        let img = match imagem(base) {
            Ok(i) => i,
            Err(e) => return registro::log(&format!("{e}; guardião desligado")),
        };
        let p = match achar::achar(&img) {
            Ok(p) => p,
            Err(e) => return registro::log(&format!("não achei os pontos de ligação: {e}; guardião desligado")),
        };
        registro::log(&format!(
            "pontos achados: entrada {:#x}->{:#x} | saida {:#x},{:#x}->{:#x} | sessao {:#x}->{:#x}",
            p.entrada_call, p.entrada_alvo, p.saida_lea[0], p.saida_lea[1], p.saida_alvo,
            p.sessao_call, p.sessao_alvo
        ));

        let Some(bloco) = bloco_perto(base) else {
            return registro::log("sem memória perto do executável; guardião desligado");
        };
        ORIG_ENTRADA.store(p.entrada_alvo, Ordering::Release);
        ORIG_SAIDA.store(p.saida_alvo, Ordering::Release);
        ORIG_SESSAO.store(p.sessao_alvo, Ordering::Release);

        let aplicar = registro::ler_modo();
        let n = estado::iniciar(aplicar);

        let s_entrada = salto(bloco, gancho_entrada as *const () as usize);
        let s_saida = salto(bloco + 16, gancho_saida as *const () as usize);
        let s_sessao = salto(bloco + 32, gancho_sessao as *const () as usize);
        FlushInstructionCache(GetCurrentProcess(), bloco as *const c_void, 48);
        // a sessão primeiro: a entrada e a saída dependem do steamid que ela registra
        let ok = trocar_destino(p.sessao_call, 5, s_sessao)
            && trocar_destino(p.saida_lea[0], 7, s_saida)
            && trocar_destino(p.saida_lea[1], 7, s_saida)
            && trocar_destino(p.entrada_call, 5, s_entrada);
        registro::log(&format!(
            "{}; modo {}; {n} cópias carregadas",
            if ok { "instalado" } else { "FALHA ao instalar um dos pontos" },
            if aplicar { "aplicar" } else { "observar" }
        ));
    }
}
