//! Native Unix terminals and Windows ConPTY share the same process boundary.
use portable_pty::{CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};
use std::io::{self, Read, Write};
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};

type SharedMaster = Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>;

pub struct Terminal {
    master: SharedMaster,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
}
impl Terminal {
    pub fn try_clone(&self) -> io::Result<Box<dyn Read + Send>> {
        self.master
            .lock()
            .map_err(|e| io::Error::other(e.to_string()))?
            .as_ref()
            .ok_or_else(|| io::Error::other("Terminal is closed."))?
            .try_clone_reader()
            .map_err(io::Error::other)
    }
}
impl Read for Terminal {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buffer)
    }
}
impl Write for Terminal {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.writer.write(buffer)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
pub struct Process {
    pub master: Terminal,
    pub child: Child,
}
pub struct Child {
    pid: u32,
    exit: mpsc::Receiver<io::Result<ExitStatus>>,
}
impl Child {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.exit.recv().map_err(io::Error::other)?
    }
}
pub fn spawn(command: &mut Command, rows: u16, cols: u16) -> Result<Process, String> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: rows.max(1),
            cols: cols.max(1),
            ..PtySize::default()
        })
        .map_err(|e| e.to_string())?;
    let mut builder = CommandBuilder::new(command.get_program());
    builder.args(command.get_args());
    for (name, value) in command.get_envs() {
        if let Some(value) = value {
            builder.env(name, value);
        } else {
            builder.env_remove(name);
        }
    }
    builder.cwd(
        command
            .get_current_dir()
            .map(std::path::Path::to_path_buf)
            .unwrap_or(std::env::current_dir().map_err(|e| e.to_string())?),
    );
    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let mut child = pair
        .slave
        .spawn_command(builder)
        .map_err(|e| format!("Cannot launch SSH/Mosh: {e}"))?;
    drop(pair.slave);
    let pid = child.process_id().ok_or("Missing child process ID.")?;
    let master = Arc::new(Mutex::new(Some(pair.master)));
    let shared = Arc::clone(&master);
    let (sender, exit) = mpsc::channel();
    // ConPTY keeps its output pipe open until ClosePseudoConsole. Wait in
    // parallel and close the console on exit so reads finish, including keygen.
    std::thread::spawn(move || {
        let status = child.wait();
        let console = shared.lock().ok().and_then(|mut master| master.take());
        drop(console);
        let _ = sender.send(status);
    });
    Ok(Process {
        master: Terminal {
            master,
            reader,
            writer,
        },
        child: Child { pid, exit },
    })
}
pub fn resize(master: &Terminal, rows: u16, cols: u16) -> Result<(), String> {
    if rows == 0 || cols == 0 {
        return Err("Invalid terminal dimensions.".into());
    }
    master
        .master
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .ok_or("Terminal is closed.")?
        .resize(PtySize {
            rows,
            cols,
            ..PtySize::default()
        })
        .map_err(|e| e.to_string())
}
pub fn stop(pid: u32) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;
        let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGHUP);
        let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGTERM);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Stop descendants too, including SSH proxy/jump processes.
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x0800_0000)
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_terminal_preserves_environment_and_exit_status() {
        #[cfg(unix)]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "read input; printf '%s' \"$TABIUS_PTY_TEST\"; exit 7"]);
            command
        };
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd.exe");
            command.args([
                "/D",
                "/C",
                "set /p input= & echo %TABIUS_PTY_TEST% & exit /b 7",
            ]);
            command
        };
        command.env("TABIUS_PTY_TEST", "terminal-ready");
        let mut process = spawn(&mut command, 24, 80).unwrap();
        assert!(resize(&process.master, 0, 80).is_err());
        resize(&process.master, 30, 100).unwrap();
        process.master.write_all(b"continue\r").unwrap();
        let mut output = Vec::new();
        // Linux PTYs may finish with EIO instead of EOF.
        let _ = process.master.read_to_end(&mut output);
        assert!(String::from_utf8_lossy(&output).contains("terminal-ready"));
        assert_eq!(process.child.wait().unwrap().exit_code(), 7);
    }
}
