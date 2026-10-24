//! Guardião do personagem. Carregado pelo servidor como dbghelp.dll.
//!
//! O servidor só importa MiniDumpWriteDump do dbghelp; essa função é repassada ao dbghelp do sistema.
//! Na carga, uma thread confere o build e troca o destino de quatro instruções (ver plano.md) para as
//! funções `entrada`, `saida` e `sessao`, que chamam as originais.

mod achar;
mod estado;
mod instalar;
mod registro;

use core::ffi::c_void;
use windows_sys::Win32::Foundation::{BOOL, HANDLE, HMODULE, TRUE};
use windows_sys::Win32::System::LibraryLoader::{DisableThreadLibraryCalls, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_sys::Win32::System::Threading::CreateThread;

#[no_mangle]
pub unsafe extern "system" fn DllMain(modulo: HMODULE, motivo: u32, _: *mut c_void) -> BOOL {
    if motivo == DLL_PROCESS_ATTACH {
        DisableThreadLibraryCalls(modulo);
        // nada de trabalho pesado aqui: a instalação roda numa thread própria
        CreateThread(core::ptr::null(), 0, Some(thread_instalar), core::ptr::null(), 0, core::ptr::null_mut());
    }
    TRUE
}

unsafe extern "system" fn thread_instalar(_: *mut c_void) -> u32 {
    instalar::instalar();
    0
}

type MiniDump = unsafe extern "system" fn(HANDLE, u32, HANDLE, u32, *const c_void, *const c_void, *const c_void) -> BOOL;

/// Repasse para o dbghelp do sistema (usado pelo servidor para gravar dumps de travamento).
#[no_mangle]
pub unsafe extern "system" fn MiniDumpWriteDump(
    processo: HANDLE, pid: u32, arquivo: HANDLE, tipo: u32,
    excecao: *const c_void, usuario: *const c_void, callback: *const c_void,
) -> BOOL {
    let mut dir = [0u16; 260];
    let n = GetSystemDirectoryW(dir.as_mut_ptr(), dir.len() as u32) as usize;
    if n == 0 || n > 230 {
        return 0;
    }
    let mut caminho: Vec<u16> = dir[..n].to_vec();
    caminho.extend("\\dbghelp.dll".encode_utf16());
    caminho.push(0);
    let m = LoadLibraryW(caminho.as_ptr());
    if m.is_null() {
        return 0;
    }
    match GetProcAddress(m, b"MiniDumpWriteDump\0".as_ptr()) {
        Some(f) => {
            let f: MiniDump = core::mem::transmute(f);
            f(processo, pid, arquivo, tipo, excecao, usuario, callback)
        }
        None => 0,
    }
}
