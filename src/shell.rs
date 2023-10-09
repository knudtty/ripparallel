use rand::distributions::{Alphanumeric, DistString};
use std::env;
use std::io::{Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

use crate::jobs::JobOut;

pub const RAND_STRING_SIZE: usize = 16;
const MAX_MEMORY_SIZE: usize = 8192; // 8 kb
const CACHE_SIZE: usize = 2048; // 2 kb
                                //
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

    pub fn execute(&mut self, command: &str) -> (JobOut, JobOut) {
        self.feed(command);
        return self.read();
    }

    fn birth(shell_name: Option<&str>) -> Self {
        // Spawn a new shell process
        let child_end_string =
            Alphanumeric.sample_string(&mut rand::thread_rng(), RAND_STRING_SIZE);
        let mut child = Command::new(
            shell_name.unwrap_or(env::var("SHELL").unwrap_or("dash".to_string()).as_str()),
        )
        .arg("-c")
        .arg(format!(
            "while true; do read line; eval \"$line\"; printf {}; printf {} >&2; done",
            child_end_string, child_end_string
        ))
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
            .expect("Failed to write to stdin");
    }

    fn read(&mut self) -> (JobOut, JobOut) {
        let mut buf_size = 50; // starting bufsize
        let mut stdout = JobOut::None;
        let mut stderr = JobOut::None;
        let mut complete: bool;
        let mut byte_history: [u8; RAND_STRING_SIZE] = [0; RAND_STRING_SIZE];
        let mut uncleared_message = Vec::new();
        let mut cached_message = Vec::new();
        loop {
            let mut buf = vec![0; buf_size]; // I have found no performance
                                             // loss using a vec over a stack
                                             // allocated array. Also I can do
                                             // dynamic growth now.
            if let Ok(n_bytes_read) = self.stdout.read(&mut buf) {
                (stdout, complete) = handle_reads(
                    stdout,
                    &buf,
                    n_bytes_read,
                    self.end_string.as_bytes(),
                    &mut byte_history,
                    &mut uncleared_message,
                    &mut cached_message,
                );
                if complete {
                    // theoretically stderr should only pop up when a command
                    // fails
                    let mut err_end: bool;
                    let mut buf = vec![0; buf_size]; // I have found no performance
                    loop {
                        // TODO: Have 2 different signals from the shell. One
                        // if there was an error, and one if there wasn't
                        if let Ok(n_bytes_read) = self.stderr.read(&mut buf) {
                            if n_bytes_read > 0 {
                                stderr = handle_stderr_reads(stderr, &buf, n_bytes_read);
                                (stderr, err_end) = match stderr {
                                    JobOut::Memory(mut v) => {
                                        if Shell::process_is_complete(
                                            self.end_string.as_bytes(),
                                            &v[v.len() - self.end_string.len()..],
                                        ) {
                                            if v.len() == self.end_string.len() {
                                                (JobOut::None, true)
                                            } else {
                                                v.truncate(v.len() - self.end_string.len());
                                                (JobOut::Memory(v), true)
                                            }
                                        } else {
                                            (JobOut::Memory(v), false)
                                        }
                                    }
                                    JobOut::None => (JobOut::None, false),
                                    _ => {
                                        panic!("Stderr should never be a file")
                                    }
                                };
                                if err_end {
                                    return (stdout, stderr);
                                }
                            }
                        }
                    }
                }
                if buf_size == n_bytes_read {
                    buf_size = (buf_size * 14 / 10).min(2048);
                }
            } else {
                break;
            }
        }
        return (JobOut::None, JobOut::None);
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

fn handle_reads(
    job: JobOut,
    buf: &Vec<u8>,
    n_bytes_read: usize,
    end_indication_bytes: &[u8],
    byte_history: &mut [u8; RAND_STRING_SIZE],
    uncleared_message: &mut Vec<u8>,
    cached_message: &mut Vec<u8>,
) -> (JobOut, bool) {
    let cleared_message = if n_bytes_read >= RAND_STRING_SIZE {
        let last_message_bytes = n_bytes_read - RAND_STRING_SIZE;
        let mut cleared_message = uncleared_message.clone();
        cleared_message.extend_from_slice(&buf[..last_message_bytes]);
        *uncleared_message = buf[last_message_bytes..n_bytes_read].to_vec();
        Some(cleared_message)
    } else if n_bytes_read + uncleared_message.len() > RAND_STRING_SIZE {
        let n_bytes = n_bytes_read + uncleared_message.len();
        let cleared_message = if n_bytes - RAND_STRING_SIZE >= uncleared_message.len() {
            uncleared_message.to_owned()
        } else {
            uncleared_message[..n_bytes - RAND_STRING_SIZE].to_vec()
        };
        *uncleared_message = if n_bytes - RAND_STRING_SIZE >= uncleared_message.len() {
            buf[n_bytes - RAND_STRING_SIZE..n_bytes_read].to_vec()
        } else {
            let mut cleared_message = uncleared_message[n_bytes - RAND_STRING_SIZE..].to_vec();
            cleared_message.extend_from_slice(&buf[..n_bytes_read]);
            cleared_message
        };
        Some(cleared_message)
    } else {
        // implicit n_bytes_read + uncleared_message <= RAND_STRING_SIZE; TODO Might not be right
        uncleared_message.extend_from_slice(&buf[..n_bytes_read]);
        None
    };
    update_byte_history(byte_history, &uncleared_message);
    let complete = Shell::process_is_complete(end_indication_bytes, byte_history);
    // Probably have to hold the state of the last 16 bytes in case it spans across byte
    // boundaries.
    let job = match job {
        JobOut::Memory(mut v) => {
            if let Some(cleared) = cleared_message {
                if v.len() + cleared.len() > MAX_MEMORY_SIZE {
                    let mut f = tempfile::tempfile().expect("Failed to create tempfile");
                    f.write_all(&v).expect("Failed to write to file");
                    f.write_all(&cleared).expect("Failed to write to file");
                    JobOut::File(f)
                } else {
                    v.extend_from_slice(&cleared);
                    JobOut::Memory(v)
                }
            } else {
                JobOut::Memory(v)
            }
        }
        JobOut::File(mut f) => {
            if let Some(cleared) = cleared_message {
                if complete || cleared.len() + cached_message.len() > CACHE_SIZE {
                    f.write_all(&cached_message)
                        .expect("Failed to write to file");
                    f.write_all(&cleared).expect("Failed to write to file");
                    *cached_message = vec![];
                } else {
                    cached_message.extend_from_slice(&cleared);
                }
            }
            JobOut::File(f)
        }
        JobOut::None => {
            if cleared_message.is_some() {
                JobOut::Memory(cleared_message.unwrap())
            } else {
                JobOut::None
            }
        }
    };
    return (job, complete);
}
fn handle_stderr_reads(job: JobOut, buf: &Vec<u8>, n_bytes_read: usize) -> JobOut {
    // Probably have to hold the state of the last 16 bytes in case it spans across byte
    // boundaries.
    let job = match job {
        JobOut::Memory(mut v) => {
            v.extend_from_slice(&buf[..n_bytes_read]);
            JobOut::Memory(v)
        }
        JobOut::None => {
            if n_bytes_read > 0 {
                JobOut::Memory(buf[..n_bytes_read].to_vec())
            } else {
                JobOut::None
            }
        }
        _ => {
            panic!("Stderr is being written to a file, this shouldn't be possible")
        }
    };
    return job;
}

fn update_byte_history(byte_history: &mut EndBytes, buf: &[u8]) {
    let buf_len = buf.len();
    if buf.len() >= RAND_STRING_SIZE {
        let idx = buf_len - RAND_STRING_SIZE;
        byte_history.copy_from_slice(&buf[idx..]);
    } else {
        let idx = RAND_STRING_SIZE - buf_len;
        byte_history.copy_within(idx.., 0);
        for i in 0..buf_len {
            byte_history[i + idx] = buf[i];
        }
    }
}
