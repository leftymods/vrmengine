#![allow(static_mut_refs)]

//! Native crash handler (Windows SEH).
//!
//! Catches hard crashes (access violations, illegal instructions, stack overflows, ...) that Rust
//! panic hooks cannot see, prints a red `[ERROR] CRASH: ...` line to the console and writes it to
//! the log file before letting the default handler terminate the process.

#[cfg(windows)]
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows_sys::Win32::Storage::FileSystem::WriteFile;
#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    GetStdHandle, SetConsoleTextAttribute, WriteConsoleW, STD_OUTPUT_HANDLE,
};
#[cfg(windows)]
use windows_sys::Win32::System::Diagnostics::Debug::{
    SetUnhandledExceptionFilter, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS,
};

#[cfg(windows)]
static mut LOG_HANDLE: HANDLE = std::ptr::null_mut();
#[cfg(windows)]
static mut MSG_BUF: [u8; 1024] = [0; 1024];
#[cfg(windows)]
static mut WIDE_BUF: [u16; 512] = [0; 512];

#[cfg(windows)]
const FOREGROUND_RED: u16 = 0x4;
#[cfg(windows)]
const FOREGROUND_INTENSITY: u16 = 0x8;

#[cfg(windows)]
extern "system" fn exception_handler(pointers: *const EXCEPTION_POINTERS) -> i32 {
    unsafe {
        let mut code: u32 = 0;
        let mut addr: usize = 0;
        if !pointers.is_null() {
            let rec = (*pointers).ExceptionRecord;
            if !rec.is_null() {
                code = (*rec).ExceptionCode as u32;
                addr = (*rec).ExceptionAddress as usize;
            }
        }
        let name = match code {
            0xC0000005 => "Access violation",
            0xC000001D => "Illegal instruction",
            0xC00000FD => "Stack overflow",
            0xC0000409 => "Stack buffer overrun / fail-fast",
            0xC000000D => "Invalid handle",
            0xC0000094 => "Integer division by zero",
            0x80000003 => "Breakpoint",
            0xE06D7363 => "C++ exception",
            _ => "Unknown",
        };
        let line = format!("[ERROR] CRASH: {} (0x{:08X}) at 0x{:X}\n", name, code, addr);
        let bytes = line.as_bytes();
        let n = bytes.len().min(MSG_BUF.len());
        MSG_BUF[..n].copy_from_slice(&bytes[..n]);
        write_log_file(MSG_BUF.as_ptr(), n);
        write_console_red(MSG_BUF.as_ptr(), n);
    }
    EXCEPTION_CONTINUE_SEARCH
}

#[cfg(windows)]
fn write_log_file(data: *const u8, len: usize) {
    unsafe {
        if LOG_HANDLE.is_null() || LOG_HANDLE == INVALID_HANDLE_VALUE {
            return;
        }
        let mut written: u32 = 0;
        let _ = WriteFile(LOG_HANDLE, data, len as u32, &mut written, std::ptr::null_mut());
    }
}

#[cfg(windows)]
fn write_console_red(data: *const u8, len: usize) {
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return;
        }
        let n = len.min(WIDE_BUF.len());
        for i in 0..n {
            WIDE_BUF[i] = *data.add(i) as u16;
        }
        let _ = SetConsoleTextAttribute(handle, FOREGROUND_RED | FOREGROUND_INTENSITY);
        let mut written: u32 = 0;
        let _ = WriteConsoleW(handle, WIDE_BUF.as_ptr(), n as u32, &mut written, std::ptr::null());
        let _ = SetConsoleTextAttribute(handle, 7);
    }
}

/// Install the native crash handler. Pass the raw handle of the log file.
#[cfg(windows)]
pub fn install(log_file: Option<&std::fs::File>) {
    use std::os::windows::io::AsRawHandle;
    unsafe {
        if let Some(f) = log_file {
            LOG_HANDLE = f.as_raw_handle();
        }
        SetUnhandledExceptionFilter(Some(exception_handler));
    }
}

/// Install the native crash handler. No-op on non-Windows platforms.
#[cfg(not(windows))]
pub fn install(_log_file: Option<&std::fs::File>) {}
