use crossbeam_channel;
use rand::distributions::{Alphanumeric, DistString};
use std::io::{Read, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};

const RAND_STRING_SIZE: usize = 16;

pub enum JobResult {
    Partial(Vec<u8>),
    Completion(Vec<u8>),
}
impl JobResult {
    pub fn is_complete(&self) -> bool {
        match self {
            Self::Completion(_) => true,
            Self::Partial(_) => false,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Completion(v) => v.is_empty(),
            Self::Partial(v) => v.is_empty(),
        }
    }
}

pub struct Shell {
    slave: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    #[allow(dead_code)]
    stderr: ChildStderr,
    end_string: String,
    sender: Sender,
    last_bytes: [u8; RAND_STRING_SIZE],
    cached_message: Vec<u8>,
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
            last_bytes: [0; RAND_STRING_SIZE],
            cached_message: vec![],
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

    fn process_is_complete(&mut self) -> bool {
        return self.last_bytes == self.end_string.as_bytes();
    }
    fn update_last_bytes(&mut self, buf: &Vec<u8>) {
        let buf_len = buf.len();
        if buf.len() >= RAND_STRING_SIZE {
            let idx = buf_len - RAND_STRING_SIZE;
            self.last_bytes.copy_from_slice(&buf[idx..]);
        } else {
            let idx = RAND_STRING_SIZE - buf_len;
            self.last_bytes.copy_within(idx.., 0);
            for i in 0..buf_len {
                self.last_bytes[i + idx] = buf[i];
            }
        }
    }

    fn send(&mut self, job: (usize, JobResult)) {
        self.sender.send(job).expect("Failed to send");
    }

    fn harass(&mut self, job_id: usize) {
        //eprintln!("received job {}", job_id);
        // figure out streaming stdout and stderr back
        // TODO: Figure out stderr. I think it should be flushed based on line break
        let mut buf_size = 50;
        loop {
            let mut buf = vec![0; buf_size];
            //let mut stderr: Vec<u8> = Vec::new();
            match self.stdout.read(&mut buf) {
                Ok(0) => {
                    //eprintln!("Stuck reading 0");
                }
                Ok(n_bytes_read) => {
                    // child stdout is received
                    // read into a vec
                    // if incoming message is >= RAND_STRING_SIZE
                    //      last bytes are updated from incoming message
                    //      if last bytes == RAND_STRING_SIZE
                    //          send old message as Complete
                    //          old message is set to zero
                    //      else
                    //          send old message as Partial
                    //          old message is set to new message
                    // else if incoming message is + old message > RAND_STRING_SIZE
                    //      last bytes are updated from incoming message and old message
                    //      if last bytes == RAND_STRING_SIZE
                    //          message is stripped of last bytes
                    //          message is sent as Complete
                    //          old message is set to zero
                    //      else
                    //          message is constructed of [incoming message + old message][0..len - RAND_STRING_SIZE]
                    //          message is sent as Partial
                    //          old message is set to remaining
                    // else (implicitly incoming message + old message < RAND_STRING_SIZE)
                    //      old message message is extended with incoming message
                    //      nothing is sent
                    //      last bytes are updated
                    //
                    //
                    let stdout: Vec<u8> = buf[0..n_bytes_read].to_vec();
                    if stdout.len() >= RAND_STRING_SIZE {
                        self.update_last_bytes(&stdout);
                        if self.process_is_complete() {
                            // potentially unnecessary construction
                            //eprintln!("Sending completion");
                            let mut new_message = self.cached_message.clone();
                            new_message.extend_from_slice(&stdout[..]);
                            self.send((
                                job_id,
                                JobResult::Completion(
                                    new_message[..(new_message.len() - RAND_STRING_SIZE)].to_vec(),
                                ),
                            ));
                            self.cached_message = vec![];
                            break;
                        } else {
                            self.send((job_id, JobResult::Partial(self.cached_message.clone())));
                            self.cached_message = stdout;
                        }
                    } else if stdout.len() + self.cached_message.len() > RAND_STRING_SIZE {
                        //let new_message =
                        let mut new_message = self.cached_message.clone();
                        new_message.extend_from_slice(&stdout[..]);
                        self.update_last_bytes(&new_message);
                        if self.process_is_complete() {
                            self.send((
                                job_id,
                                JobResult::Completion(
                                    new_message[..(new_message.len() - RAND_STRING_SIZE)].to_vec(),
                                ),
                            ));
                            self.cached_message = vec![];
                            break;
                        } else {
                            // TODO: Look into sending the bytes up to len - RAND_STRING_SIZE.
                            // Potentailly not necessary.
                            self.cached_message = new_message;
                        }
                    } else {
                        let mut new_message = self.cached_message.clone();
                        new_message.extend_from_slice(&stdout[..]);
                        self.update_last_bytes(&new_message);
                        self.cached_message = new_message;
                    }
                    if n_bytes_read == buf_size {
                        buf_size = buf_size * 14 / 10;
                    }
                    //eprintln!("Cached message: {} end", String::from_utf8(self.cached_message.clone()).unwrap());
                    //self.update_last_bytes(&stdout);
                    //if self.message_is_cleared() {
                    //    if self.process_is_complete() {
                    //        self.send((
                    //            job_id,
                    //            JobResult::Completion(JobRes {
                    //                n_bytes: n_bytes_read,
                    //                bytes: stdout,
                    //            }),
                    //        ));
                    //        break;
                    //    } else {
                    //        self.send((
                    //            job_id,
                    //            JobResult::Partial(JobRes {
                    //                n_bytes: n_bytes_read,
                    //                bytes: stdout,
                    //            }),
                    //        ));
                    //    };
                    //}
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
