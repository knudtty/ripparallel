use std::io::{Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub static WANT_BYTE: u8 = b"\x04"[0];

pub struct Shell {
    #[allow(dead_code)]
    slave: Child,
    pub stdin: ChildStdin,
    pub stdout: ChildStdout,
}
impl Shell {
    pub fn birth() -> Self {
        //let bash_loop: String = format!("while true; do read line; if [ \"$line\" = \"exit\" ]; then exit; fi; eval \"$line; printf \"{}\"; done", WANT_BYTE);
        // Spawn a new shell process
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("while true; do read line; if [ \"$line\" = \"exit\" ]; then exit; fi; eval \"$line; printf \"\x04\"\"; done")
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn shell process");

        let child_stdin = child.stdin.take().expect("Failed to open stdin");
        let child_stdout = child.stdout.take().expect("Failed to open stdout");
        Shell {
            slave: child,
            stdin: child_stdin,
            stdout: child_stdout,
        }
    }

    pub fn feed(&mut self, command: &String) {
        self.stdin
            .write_all(command.as_bytes())
            .expect("Failed to write to stdin");
        self.stdin
            .write_all(b"\n")
            .expect("Failed to write newline to stdin");
    }

    pub fn harass(&mut self) -> Vec<u8> {
        let mut output: Vec<u8> = Vec::new();
        loop {
            let mut buf = [0; 10];
            match self.stdout.read(&mut buf) {
                Ok(0) => {}
                Ok(n_bytes_read) => {
                    output.extend(buf.iter().take(n_bytes_read));
                    let last_byte = buf.get(n_bytes_read - 1).unwrap();
                    let want_byte = b"\x04"[0];
                    if &want_byte == last_byte {
                        break;
                    }
                }
                Err(_) => {
                    println!("ERROR");
                }
            }
        }
        return output;
    }

    pub fn kill(&mut self) {
        self.stdin
            .write(b"exit\n")
            .expect("Failed to kill shell process");
        self.slave.wait().expect("Failed to kill process");
    }
}
