use crossbeam_channel;
use rand::distributions::{Alphanumeric, DistString};
use std::io::{Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

const BUF_SIZE: usize = 50;
const RAND_STRING_SIZE: usize = 16;

pub struct JobRes {
    pub n_bytes: usize,
    pub bytes: Box<[u8; BUF_SIZE]>, // TODO: Maybe box?
}

pub enum JobResult {
    Partial(JobRes),
    Completion(JobRes),
}

pub struct Shell {
    slave: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    #[allow(dead_code)]
    stderr: ChildStderr,
    end_string: String,
    sender: Sender,
}

type Sender = crossbeam_channel::Sender<(usize, JobResult)>;
pub type Execution = Option<(Vec<u8>, Option<Vec<u8>>)>;
impl Shell {
    pub fn new(sender: Sender) -> Self {
        return Self::birth(None, sender);
    }

    pub fn new_with(shell_name: &str, sender: Sender) -> Self {
        return Self::birth(Some(shell_name), sender);
    }

    pub fn execute(&mut self, command: String, job_id: usize) {
        self.feed(&command);
        self.harass(job_id);
    }

    pub fn kill(&mut self) {
        self.stdin
            .write(b"exit\n")
            .expect("Failed to kill shell process");
        self.slave.wait().expect("Failed to kill process");
    }

    fn birth(shell_name: Option<&str>, sender: Sender) -> Self {
        // Spawn a new shell process
        let child_end_string =
            Alphanumeric.sample_string(&mut rand::thread_rng(), RAND_STRING_SIZE);
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
            sender,
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

    fn process_is_complete(bytes_read: usize, buf: &[u8; BUF_SIZE], end_bytes: &[u8]) -> bool {
        let bytes_len = end_bytes.len();
        if bytes_read < bytes_len {
            return false;
        }
        for (bytes_i, buf_i) in ((bytes_read - bytes_len)..bytes_read).enumerate() {
            let stdout_byte = buf.get(buf_i).unwrap();
            let bytes_byte = end_bytes.get(bytes_i).unwrap();
            if stdout_byte != bytes_byte {
                return false;
            }
        }
        return true;
    }

    fn send(&mut self, job: (usize, JobResult)) {
        self.sender.send(job).expect("Failed to send");
    }

    fn harass(&mut self, job_id: usize) {
        // figure out streaming stdout and stderr back
        loop {
            //let mut stdout: Vec<u8> = Vec::new();
            //let mut stderr: Vec<u8> = Vec::new();
            let mut buf = [0; BUF_SIZE];
            match self.stdout.read(&mut buf) {
                Ok(0) => {
                    eprintln!("Stuck reading 0");
                }
                Ok(n_bytes_read) => {
                    if Shell::process_is_complete(n_bytes_read, &buf, self.end_string.as_bytes()) {
                        self.send((
                            job_id,
                            JobResult::Completion(JobRes {
                                n_bytes: n_bytes_read - RAND_STRING_SIZE,
                                bytes: Box::new(buf),
                            }),
                        ));
                        break;
                    } else {
                        self.send((
                            job_id,
                            JobResult::Partial(JobRes {
                                n_bytes: n_bytes_read,
                                bytes: Box::new(buf),
                            }),
                        ));
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
    }
}
