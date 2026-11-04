//! Enshrouded Server Guardian. Loaded by the server as dbghelp.dll.
//!
//! The server only imports MiniDumpWriteDump from dbghelp; that function is forwarded to the system dbghelp.
//! On load, a thread finds the hook points and redirects four instructions to the `entrada` (join),
//! `saida` (save) and `sessao` (session) functions, which call the originals.

mod achar;
mod atualizacao;
mod continuidade;
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
        // no heavy work here: installation runs on its own thread
        CreateThread(core::ptr::null(), 0, Some(thread_instalar), core::ptr::null(), 0, core::ptr::null_mut());
    }
    TRUE
}

unsafe extern "system" fn thread_instalar(_: *mut c_void) -> u32 {
    instalar::instalar();
    0
}

type MiniDump = unsafe extern "system" fn(HANDLE, u32, HANDLE, u32, *const c_void, *const c_void, *const c_void) -> BOOL;

/// Forwards to the system dbghelp (used by the server to write crash dumps).
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
