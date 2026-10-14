//! Nome do executável da janela em primeiro plano (para trocar de perfil automaticamente).

/// Ex.: `Some("cs2.exe")`, sempre em minúsculas.
#[cfg(windows)]
pub fn foreground_exe() -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    // SAFETY: chamadas Win32 com ponteiros para buffers locais de tamanho informado;
    // o handle do processo é fechado antes de retornar.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        exe_name(&String::from_utf16_lossy(&buf[..len as usize]))
    }
}

#[cfg(not(windows))]
pub fn foreground_exe() -> Option<String> {
    None
}

/// Nome do arquivo de um caminho, em minúsculas.
pub fn exe_name(path: &str) -> Option<String> {
    path.rsplit(['\\', '/']).next().filter(|s| !s.is_empty()).map(str::to_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_from_path() {
        assert_eq!(exe_name(r"C:\Games\CS2\CS2.exe").as_deref(), Some("cs2.exe"));
        assert_eq!(exe_name("/usr/bin/firefox").as_deref(), Some("firefox"));
        assert_eq!(exe_name(r"C:\pasta\"), None);
    }
}
