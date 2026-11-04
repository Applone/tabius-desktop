//! Native Unix terminals and Windows ConPTY share the same process boundary.
use portable_pty::{CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};
use std::io::{self, Read, Write};
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};

type SharedMaster = Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>;

#[derive(Clone)]
struct Reader(Arc<Mutex<Box<dyn Read + Send>>>);
impl Read for Reader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|e| io::Error::other(e.to_string()))?
            .read(buffer)
    }
}
#[derive(Clone)]
struct Writer(Arc<Mutex<Box<dyn Write + Send>>>);
impl Write for Writer {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|e| io::Error::other(e.to_string()))?
            .write(buffer)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|e| io::Error::other(e.to_string()))?
            .flush()
    }
    fn write_all(&mut self, buffer: &[u8]) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|e| io::Error::other(e.to_string()))?
            .write_all(buffer)
    }
}

#[cfg(any(windows, test))]
struct CursorReader<R, W> {
    reader: R,
    writer: W,
    pending: std::collections::VecDeque<u8>,
    complete: bool,
}
#[cfg(any(windows, test))]
impl<R: Read, W: Write> CursorReader<R, W> {
    fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            pending: Default::default(),
            complete: false,
        }
    }
}
#[cfg(any(windows, test))]
impl<R: Read, W: Write> Read for CursorReader<R, W> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        const QUERY: &[u8] = b"\x1b[6n";
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            // Retain a partial query across reads, without hiding ordinary output.
            let held = if self.complete {
                0
            } else {
                (1..QUERY.len())
                    .rev()
                    .find(|&n| self.pending.make_contiguous().ends_with(&QUERY[..n]))
                    .unwrap_or(0)
            };
            let count = buffer.len().min(self.pending.len() - held);
            if count > 0 {
                for (out, byte) in buffer.iter_mut().zip(self.pending.drain(..count)) {
                    *out = byte;
                }
                return Ok(count);
            }
            if self.complete {
                return self.reader.read(buffer);
            }
            let mut chunk = [0; 4096];
            let count = self.reader.read(&mut chunk)?;
            if count == 0 {
                self.complete = true;
                continue;
            }
            self.pending.extend(&chunk[..count]);
            if let Some(start) = self
                .pending
                .make_contiguous()
                .windows(QUERY.len())
                .position(|bytes| bytes == QUERY)
            {
                // portable-pty requests cursor inheritance. Headless readers must
                // answer it too, or ConPTY can block startup and console teardown.
                // Consume only the initial query; later queries belong to xterm.
                self.pending.drain(start..start + QUERY.len());
                self.writer.write_all(b"\x1b[1;1R")?;
                self.writer.flush()?;
                self.complete = true;
            }
        }
    }
}

pub struct Terminal {
    master: SharedMaster,
    reader: Reader,
    writer: Writer,
}
impl Terminal {
    pub fn try_clone(&self) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(self.reader.clone()))
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
    fn write_all(&mut self, buffer: &[u8]) -> io::Result<()> {
        self.writer.write_all(buffer)
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
    let writer = Writer(Arc::new(Mutex::new(
        pair.master.take_writer().map_err(|e| e.to_string())?,
    )));
    #[cfg(windows)]
    let reader: Box<dyn Read + Send> = Box::new(CursorReader::new(reader, writer.clone()));
    let reader = Reader(Arc::new(Mutex::new(reader)));
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
pub fn test_with_timeout(task: impl FnOnce() + Send + 'static) {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(task)));
    });
    if let Err(panic) = receiver
        .recv_timeout(std::time::Duration::from_secs(30))
        .expect("Native process test did not finish within 30 seconds")
    {
        std::panic::resume_unwind(panic);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_handshake_handles_fragmented_reads_and_preserves_later_queries() {
        let input = b"before\x1b[6nafter\x1b[6n";
        for split in 0..=input.len() {
            for size in 1..=input.len() {
                let reader = (&input[..split]).chain(&input[split..]);
                let mut reader = CursorReader::new(reader, Vec::new());
                assert_eq!(reader.read(&mut []).unwrap(), 0);
                assert!(reader.writer.is_empty());
                let mut output = Vec::new();
                let mut buffer = vec![0; size];
                loop {
                    let count = reader.read(&mut buffer).unwrap();
                    if count == 0 {
                        break;
                    }
                    output.extend_from_slice(&buffer[..count]);
                }
                assert_eq!(output, b"beforeafter\x1b[6n");
                assert_eq!(reader.writer, b"\x1b[1;1R");
            }
        }
    }

    #[test]
    fn cursor_handshake_preserves_output_without_a_complete_query() {
        for input in [b"".as_slice(), b"ordinary output", b"\x1b[31mred\x1b["] {
            let mut reader = CursorReader::new(input, Vec::new());
            let mut output = Vec::new();
            reader.read_to_end(&mut output).unwrap();
            assert_eq!(output, input);
            assert!(reader.writer.is_empty());
        }
    }

    #[test]
    fn cursor_handshake_reports_reply_failures() {
        let mut reader = CursorReader::new(b"\x1b[6n".as_slice(), io::Cursor::new([]));
        assert_eq!(
            reader.read(&mut [0; 16]).unwrap_err().kind(),
            io::ErrorKind::WriteZero
        );
    }

    #[test]
    fn native_terminal_preserves_environment_and_exit_status() {
        test_with_timeout(|| {
            #[cfg(unix)]
            let mut command = {
                let mut command = Command::new("sh");
                command.args([
                    "-c",
                    "printf 'input-ready\\n'; read input; printf '%s' \"$TABIUS_PTY_TEST\"; exit 7",
                ]);
                command
            };
            #[cfg(windows)]
            let mut command = {
                let mut command = Command::new("cmd.exe");
                command.args([
                    "/D",
                    "/C",
                    "echo input-ready & set /p input= & echo %TABIUS_PTY_TEST% & exit /b 7",
                ]);
                command
            };
            command.env("TABIUS_PTY_TEST", "terminal-ready");
            let mut process = spawn(&mut command, 24, 80).unwrap();
            let mut reader = process.master.try_clone().unwrap();
            let mut output = Vec::new();
            let mut buffer = [0; 4096];
            // Complete ConPTY's handshake before sending input or resizing it.
            while !String::from_utf8_lossy(&output).contains("input-ready") {
                let count = reader.read(&mut buffer).unwrap();
                assert_ne!(count, 0, "Child exited before requesting input");
                output.extend_from_slice(&buffer[..count]);
            }
            assert!(resize(&process.master, 0, 80).is_err());
            resize(&process.master, 30, 100).unwrap();
            process.master.write_all(b"continue\r").unwrap();
            // Linux PTYs may finish with EIO instead of EOF.
            let _ = reader.read_to_end(&mut output);
            assert!(String::from_utf8_lossy(&output).contains("terminal-ready"));
            assert_eq!(process.child.wait().unwrap().exit_code(), 7);
        });
    }

    #[test]
    fn native_terminal_drains_large_output_before_exit() {
        test_with_timeout(|| {
            #[cfg(unix)]
            let mut command = {
                let mut command = Command::new("sh");
                command.args(["-c", "i=0; while [ \"$i\" -lt 2000 ]; do printf 'terminal-output-%s\\n' \"$i\"; i=$((i+1)); done"]);
                command
            };
            #[cfg(windows)]
            let mut command = {
                let mut command = Command::new("cmd.exe");
                command.args([
                    "/D",
                    "/C",
                    "for /L %i in (1,1,2000) do @echo terminal-output-%i",
                ]);
                command
            };
            let mut process = spawn(&mut command, 24, 80).unwrap();
            let mut output = Vec::new();
            let _ = process.master.read_to_end(&mut output);
            assert!(process.child.wait().unwrap().success());
            assert!(
                String::from_utf8_lossy(&output)
                    .matches("terminal-output-")
                    .count()
                    >= 2000
            );
        });
    }
}
