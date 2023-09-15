use std::io::{Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

static WANT_BYTE: u8 = b"\x04"[0];
const BASH_LOOP: &'static str = "while true; do read line; if [ \"$line\" = \"exit\" ]; then exit; fi; ${line}; printf \"\x04\"; printf \"\x04\" >&2; done";
const BUF_SIZE: usize = 50;

pub struct Shell {
    #[allow(dead_code)]
    slave: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
}

pub struct StdOut(pub Vec<u8>);
pub struct StdErr(pub Option<Vec<u8>>);
type Execution = (StdOut, StdErr);
impl Shell {
    pub fn new() -> Self {
        return Self::birth(None, BASH_LOOP);
    }

    pub fn new_with(shell_name: &str) -> Self {
        return Self::birth(Some(shell_name), BASH_LOOP);
    }

    pub fn execute(&mut self, command: String) -> Execution {
        self.feed(&command);
        return self.harass();
    }

    pub fn kill(&mut self) {
        self.stdin
            .write(b"exit\n")
            .expect("Failed to kill shell process");
        self.slave.wait().expect("Failed to kill process");
    }

    fn birth(shell_name: Option<&str>, bash_arg: &str) -> Self {
        // Spawn a new shell process
        let mut child = Command::new(shell_name.unwrap_or("sh"))
            .arg("-c")
            .arg(bash_arg)
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn shell process");

        let child_stdin = child.stdin.take().expect("Failed to open stdin");
        let child_stdout = child.stdout.take().expect("Failed to open stdout");
        let child_stderr = child.stderr.take().expect("Failed to open stdout");
        Shell {
            slave: child,
            stdin: child_stdin,
            stdout: child_stdout,
            stderr: child_stderr 
        }
    }

    fn feed(&mut self, command: &str) {
        self.stdin
            .write_all(command.as_bytes())
            .expect("Failed to write to stdin");
        self.stdin
            .write_all(b"\n")
            .expect("Failed to write newline to stdin");
    }

    fn harass(&mut self) -> Execution {
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        loop {
            let mut buf = [0; BUF_SIZE];
            match self.stdout.read(&mut buf) {
                Ok(0) => {
                    if stdout.len() > 0 {
                        break;
                    }
                }
                Ok(n_bytes_read) => {
                    stdout.extend(buf.iter().take(n_bytes_read));
                    let last_byte = &buf[n_bytes_read - 1];
                    // TODO: Fix this bug. Shouldn't exit anytime it finds 4 at the end of the
                    // buffer
                    if WANT_BYTE == *last_byte && n_bytes_read < BUF_SIZE {
                        break;
                    }
                }
                Err(_) => {
                    eprintln!("ERROR");
                }
            }
        }
        loop {
            let mut buf = [0; BUF_SIZE];
            match self.stderr.read(&mut buf) {
                Ok(0) => {
                    if stderr.len() > 0 {
                        break;
                    }
                }
                Ok(n_bytes_read) => {
                    stderr.extend(buf.iter().take(n_bytes_read));
                    let last_byte = &buf[n_bytes_read - 1];
                    if WANT_BYTE == *last_byte {
                        break;
                    }
                }
                Err(_) => {
                    eprintln!("ERROR");
                }
            }
        }
        // only 1 unit
        if stderr.len() == 1 {
            return (StdOut(stdout), StdErr(None));
        }
        let stderr: Vec<_> = Vec::new();
        return (StdOut(stdout), StdErr(Some(stderr)));
    }
}
