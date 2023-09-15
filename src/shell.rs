use rand::distributions::{Alphanumeric, DistString};
use std::io::{Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

const BUF_SIZE: usize = 50;
const RAND_STRING_SIZE: usize = 16;

pub struct Shell {
    slave: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    #[allow(dead_code)]
    stderr: ChildStderr,
    end_string: String,
}

pub struct StdOut(pub Vec<u8>);
pub struct StdErr(pub Option<Vec<u8>>);
type Execution = (StdOut, StdErr);

impl Shell {
    pub fn new() -> Self {
        return Self::birth(None);
    }

    pub fn new_with(shell_name: &str) -> Self {
        return Self::birth(Some(shell_name));
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

    fn birth(shell_name: Option<&str>) -> Self {
        // Spawn a new shell process
        let child_end_string = Alphanumeric.sample_string(&mut rand::thread_rng(), RAND_STRING_SIZE);
        let mut child = Command::new(shell_name.unwrap_or("sh"))
            .arg("-c")
            .arg(format!("while true; do read line; if [ \"$line\" = \"exit\" ]; then exit; fi; $line; printf {}; done", child_end_string))
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

    fn feed(&mut self, command: &str) {
        self.stdin
            .write_all(command.as_bytes())
            .expect("Failed to write to stdin");
        self.stdin
            .write_all(b"\n")
            .expect("Failed to write newline to stdin");
    }

    fn process_is_complete(end_bytes: &[u8], stdout: &Vec<u8>) -> bool {
        let bytes_len = end_bytes.len();
        let stdout_len = stdout.len();
        if stdout_len < bytes_len {
            return false
        }
        for (bytes_i, stdout_i) in ((stdout_len - bytes_len)..stdout_len).enumerate() {
            let stdout_byte = stdout.get(stdout_i).unwrap();
            let bytes_byte = end_bytes.get(bytes_i).unwrap();
            if stdout_byte != bytes_byte {
                return false
            }
        }
        return true

    }

    fn harass(&mut self) -> Execution {
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        // figure out streaming stdout and stderr back
        loop {
            let mut buf = [0; BUF_SIZE];
            match self.stdout.read(&mut buf) {
                Ok(0) => {
                    eprintln!("Stuck reading 0");
                }
                Ok(n_bytes_read) => {
                    stdout.extend(buf.iter().take(n_bytes_read));
                    if Shell::process_is_complete(self.end_string.as_bytes(), &stdout) {
                        stdout.truncate(stdout.len() - self.end_string.len());
                        break;
                    };
                }
                Err(_) => {
                    eprintln!("ERROR");
                }
            }
        }
        //loop {
        //    let mut buf = [0; BUF_SIZE];
        //    match self.stderr.read(&mut buf) {
        //        Ok(0) => {
        //            if stderr.len() > 0 {
        //                break;
        //            }
        //        }
        //        Ok(n_bytes_read) => {
        //            stderr.extend(buf.iter().take(n_bytes_read));
        //            let last_byte = &buf[n_bytes_read - 1];
        //            if WANT_BYTE == *last_byte {
        //                break;
        //            }
        //        }
        //        Err(_) => {
        //            eprintln!("ERROR");
        //        }
        //    }
        //}
        // only 1 unit
        if stderr.len() == 1 {
            return (StdOut(stdout), StdErr(None));
        }
        let stderr: Vec<_> = Vec::new();
        return (StdOut(stdout), StdErr(Some(stderr)));
    }
}
