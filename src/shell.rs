use rand::distributions::{Alphanumeric, DistString};
use std::env;
use std::io::Write;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

pub const RAND_STRING_SIZE: usize = 16;
pub type EndBytes = [u8; RAND_STRING_SIZE];

pub struct Shell {
    slave: Child,
    stdin: ChildStdin,
    pub stdout: ChildStdout,
    #[allow(dead_code)]
    pub stderr: ChildStderr,
    pub end_string: String,
}

impl Shell {
    pub fn new() -> Self {
        return Self::birth(None);
    }

    pub fn new_with(shell_name: &str) -> Self {
        return Self::birth(Some(shell_name));
    }

    pub fn kill(&mut self) {
        self.stdin
            .write(b"exit\n")
            .expect("Failed to kill shell process");
        self.slave.wait().expect("Failed to kill process");
    }

    fn birth(shell_name: Option<&str>) -> Self {
        // Spawn a new shell process
        let child_end_string =
            Alphanumeric.sample_string(&mut rand::thread_rng(), RAND_STRING_SIZE);
        let mut child = Command::new(shell_name.unwrap_or(env::var("SHELL").unwrap_or("bash".to_string()).as_str()))
            .arg("-c")
            .arg(format!("while true; do read line; if [ \"$line\" = \"exit\" ]; then exit; fi; $line; printf {}; printf {} >&2; done", child_end_string, child_end_string))
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
            stderr: child_stderr,
            end_string: child_end_string,
        }
    }

    pub fn feed(&mut self, command: String) {
        self.stdin
            .write_all(command.as_bytes())
            .expect("Failed to write to stdin");
        self.stdin
            .write_all(b"\n")
            .expect("Failed to write newline to stdin");
    }

    pub fn process_is_complete(end_bytes: &[u8], stdout: &[u8]) -> bool {
        let bytes_len = end_bytes.len();
        let stdout_len = stdout.len();
        if stdout_len < bytes_len {
            return false;
        }
        for (bytes_i, stdout_i) in ((stdout_len - bytes_len)..stdout_len).enumerate() {
            let stdout_byte = stdout.get(stdout_i).unwrap();
            let bytes_byte = end_bytes.get(bytes_i).unwrap();
            if stdout_byte != bytes_byte {
                return false;
            }
        }
        return true;
    }
}
