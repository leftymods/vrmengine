//! Simple colored logging system.
//!
//! Writes to stdout (with ANSI colors applied only to the `[LEVEL]` token) and to
//! `vrmengine.log` (plain text). ANSI is only emitted when stdout is a real console,
//! otherwise the output stays plain (no escape-code garbage in piped shells like PowerShell).
//! Also installs a panic hook so crashes are captured in the log file.

use std::io::Write;
use std::sync::Mutex;

#[cfg(not(windows))]
use std::io::IsTerminal;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    GetStdHandle, SetConsoleTextAttribute, WriteConsoleW, STD_OUTPUT_HANDLE,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    #[cfg(not(windows))]
    fn color(&self) -> &'static str {
        match self {
            Level::Debug => "\x1b[96m",
            Level::Info => "\x1b[92m",
            Level::Warn => "\x1b[93m",
            Level::Error => "\x1b[91m",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }
}

static LOG_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);
static LOGGER_INITIALIZED: std::sync::Once = std::sync::Once::new();
static START_TIME: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
static USE_COLOR: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Initialize the logger: enable ANSI on Windows, open the log file and install a panic hook.
pub fn init() {
    LOGGER_INITIALIZED.call_once(|| {
        START_TIME.get_or_init(std::time::Instant::now);

        #[cfg(windows)]
        let use_color = false; // ANSI is mangled by the Windows console; keep output plain.
        #[cfg(not(windows))]
        let use_color = std::io::stdout().is_terminal();
        let _ = USE_COLOR.set(use_color);

        if let Ok(file) = open_log_file() {
            let _ = file.set_len(0);
            if let Ok(mut lf) = LOG_FILE.lock() {
                *lf = Some(file);
            }
        }
        // Install the native crash handler (SEH) so hard crashes are reported in red.
        let guard = LOG_FILE.lock();
        let log_file: Option<&std::fs::File> = match &guard {
            Ok(g) => g.as_ref(),
            Err(_) => None,
        };
        crate::crash::install(log_file);
        let log_path = if let Ok(lf) = LOG_FILE.lock() {
            lf.as_ref().map(|f| log_path_of(f))
        } else {
            None
        };
        if let Some(path) = &log_path {
            info(&format!("Log file: {}", path));
        } else {
            warn("Could not open vrmengine.log; logging to console only");
        }

        std::panic::set_hook(Box::new(|panic_info| {
            let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
                (*s).to_string()
            } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            let loc = panic_info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_else(|| "?".to_string());
            error(&format!("PANIC: {} ({})", msg, loc));
            let backtrace = std::backtrace::Backtrace::force_capture();
            error(&format!("BACKTRACE:\n{}", backtrace));
        }));

        info("Logger initialized");
    });
}

fn open_log_file() -> std::io::Result<std::fs::File> {
    use std::io::ErrorKind;
    match open_file_path("vrmengine.log") {
        Ok(f) => Ok(f),
        Err(first) => {
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    let path = dir.join("vrmengine.log");
                    if let Some(p) = path.to_str() {
                        match open_file_path(p) {
                            Ok(f) => return Ok(f),
                            Err(_) => return Err(first),
                        }
                    }
                }
            }
            Err(std::io::Error::new(ErrorKind::Other, "cannot create log file"))
        }
    }
}

fn open_file_path(path: &str) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .append(true)
        .open(path)
}

fn log_path_of(_f: &std::fs::File) -> String {
    std::env::current_dir()
        .map(|d| d.join("vrmengine.log").display().to_string())
        .unwrap_or_else(|_| "vrmengine.log".to_string())
}

fn timestamp() -> String {
    if let Some(start) = START_TIME.get() {
        let e = start.elapsed();
        let mins = e.as_secs() / 60;
        let secs = e.as_secs() % 60;
        let millis = e.subsec_millis();
        format!("{:02}:{:02}.{:03}", mins, secs, millis)
    } else {
        String::new()
    }
}

fn log(level: Level, msg: &str) {
    let ts = timestamp();
    let label = level.label();
    let plain = format!("[{}] [{}] {}", ts, label, msg);

    write_console_line(level, &ts, label, msg, &plain);

    if let Ok(mut lf) = LOG_FILE.lock() {
        if let Some(f) = lf.as_mut() {
            let _ = writeln!(f, "{}", plain);
            let _ = f.flush();
        }
    }
}

#[cfg(windows)]
fn write_console_line(level: Level, ts: &str, label: &str, msg: &str, plain: &str) {
    use std::io::Write;
    use windows_sys::Win32::System::Console::GetConsoleMode;
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            let _ = writeln!(std::io::stdout(), "{}", plain);
            let _ = std::io::stdout().flush();
            return;
        }
        let mut mode: u32 = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            // stdout is a pipe (e.g. PowerShell); plain text.
            let _ = writeln!(std::io::stdout(), "{}", plain);
            let _ = std::io::stdout().flush();
            return;
        }
        let color = match level {
            Level::Debug => 11, // bright cyan
            Level::Info => 10,  // bright green
            Level::Warn => 14,  // bright yellow
            Level::Error => 12, // bright red
        };
        write_console(handle, 7, &format!("[{}] ", ts));
        write_console(handle, color, &format!("[{}]", label));
        write_console(handle, 7, &format!(" {}\n", msg));
    }
}

#[cfg(windows)]
unsafe fn write_console(handle: HANDLE, color: u16, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let _ = SetConsoleTextAttribute(handle, color);
    let mut written: u32 = 0;
    let _ = WriteConsoleW(handle, wide.as_ptr(), wide.len() as u32, &mut written, std::ptr::null());
}

#[cfg(not(windows))]
fn write_console_line(level: Level, ts: &str, label: &str, msg: &str, plain: &str) {
    let use_color = USE_COLOR.get().copied().unwrap_or(false);
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    if use_color {
        // Color only the `[LEVEL]` token; timestamp and message stay plain.
        let _ = writeln!(handle, "[{}] \x1b[{}m[{}]\x1b[0m {}", ts, level.color(), label, msg);
    } else {
        let _ = writeln!(handle, "{}", plain);
    }
    let _ = handle.flush();
}

pub fn debug(msg: &str) {
    log(Level::Debug, msg);
}

pub fn info(msg: &str) {
    log(Level::Info, msg);
}

pub fn warn(msg: &str) {
    log(Level::Warn, msg);
}

pub fn error(msg: &str) {
    log(Level::Error, msg);
}
