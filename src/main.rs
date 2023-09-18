use clap::Parser;
use crossbeam_channel::unbounded;
use exitcode;
use ripparallel::{
    jobs::{JobOut, Message},
    ordering::WaitingRoom,
    shell::{EndBytes, Shell, RAND_STRING_SIZE},
    thread_pool,
};
use std::io::{self, Read, Seek, Write};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

const MAX_MEMORY_SIZE: usize = 1048; // 2 kb
const CACHE_SIZE: usize = 2048; // 2 kb

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, trailing_var_arg = true)]
struct Args {
    /// Print commands that would be run
    #[arg(short = 'n', long)]
    dryrun: bool,

    /// Number of jobs
    #[arg(short, long)]
    jobs: Option<usize>,

    /// Placeholder
    #[arg(short = 'I')]
    replace_string: Option<String>, // TODO

    /// Remove memory usage limit. Useful to increase speed when stdout is large
    #[arg(short = 'M', long)]
    max_memory: bool, // TODO

    /// value 1-10 to indicate how much memory is available for use. 10 is equivalent to passing -M
    #[arg(short, long)]
    memory: Option<u8>, // TODO

    /// Maintain order that inputs came in
    #[arg(short, long)]
    keep_order: bool,

    /// Surround argument with single quotes
    #[arg(short, long)]
    quotes: bool,

    /// Shell to run in. If not specified, $SHELL is used
    #[arg(long)]
    shell: Option<String>,

    /// job to run
    job: Vec<String>,
}

fn parse_command(command_args: &Vec<String>, job_input: String, quotes: bool) -> String {
    let cmd_args_len = command_args.len();
    let cmditer = command_args.iter().enumerate();
    let mut command = String::new();
    let mut substituted = false;
    cmditer.for_each(|(i, arg)| {
        if arg.as_str() == "{}" {
            substituted = true;
            if quotes {
                command.push('\'');
                command.push_str(job_input.as_str());
                command.push('\'');
            } else {
                command.push_str(job_input.as_str());
            }
            command.push(' ');
        } else {
            command.push_str(arg.as_str());
            // not end of command, should push a space
            if i != cmd_args_len - 1 {
                command.push(' ');
            // end of command, should push a space if substitution is
            // expected to take place at end
            } else if !substituted {
                command.push(' ');
            }
        }
    });
    if !substituted {
        if quotes {
            command.push('\'');
            command.push_str(job_input.as_str());
            command.push('\'');
        } else {
            command.push_str(job_input.as_str());
        }
    }
    command.push('\n');
    command
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
                    //let mut buf = String::new();
                    //f.read_to_string(&mut buf);
                    //println!("{}", buf);
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
                    //println!("writing: {:?}", cleared);
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

fn print_outputs(outputs: (JobOut, JobOut)) {
    // portable function part
    // increment next_customer and write to stdout
    let (job_output, job_err) = outputs;
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    match job_output {
        JobOut::File(mut f) => {
            f.seek(io::SeekFrom::Start(0))
                .expect("Failed to return to start");
            io::copy(&mut f, &mut stdout).expect("Failed to write file to stdout");
            drop(f); // signals to the OS to remove the tempfile
        }
        JobOut::Memory(v) => {
            stdout
                .write_all(v.as_slice())
                .expect("Failed to write to stdout");
        }
        JobOut::None => { /* do nothing */ }
    }
    match job_err {
        JobOut::File(mut f) => {
            eprintln!("At memory...");
            io::copy(&mut f, &mut stderr).expect("Failed to write file to stdout");
            drop(f);
        }
        JobOut::Memory(v) => {
            stderr
                .write_all(v.as_slice())
                .expect("Failed to write to stdout");
        }
        JobOut::None => { /* do nothing */ }
    }
}

struct JobExecutor {
    job_args: Arc<Args>,
    sender: crossbeam_channel::Sender<(usize, JobOut, JobOut)>,
    shell: Shell,
}

impl JobExecutor {
    fn new(
        job_args: Arc<Args>,
        sender: crossbeam_channel::Sender<(usize, JobOut, JobOut)>,
    ) -> Self {
        let shell = Shell::new();
        JobExecutor {
            job_args,
            sender,
            shell,
        }
    }

    fn execute_job(&mut self, job: Message) -> bool {
        match job {
            Message::Job((i, job_input)) => {
                let command = parse_command(&self.job_args.job, job_input, self.job_args.quotes);
                if self.job_args.dryrun {
                    let command = command.clone();
                    self.sender
                        .send((i, JobOut::Memory(command.as_bytes().to_vec()), JobOut::None))
                        .expect("Failed to send job to main thread");
                    return false;
                }
                self.shell.feed(command);
                //let mut stderr: Vec<u8> = Vec::new();
                // figure out streaming stdout and stderr back
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
                    if let Ok(n_bytes_read) = self.shell.stdout.read(&mut buf) {
                        (stdout, complete) = handle_reads(
                            stdout,
                            &buf,
                            n_bytes_read,
                            self.shell.end_string.as_bytes(),
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
                                if let Ok(n_bytes_read) = self.shell.stderr.read(&mut buf) {
                                    if n_bytes_read > 0 {
                                        stderr = handle_stderr_reads(stderr, &buf, n_bytes_read);
                                        (stderr, err_end) = match stderr {
                                            JobOut::Memory(mut v) => {
                                                if Shell::process_is_complete(
                                                    self.shell.end_string.as_bytes(),
                                                    &v[v.len() - self.shell.end_string.len()..],
                                                ) {
                                                    if v.len() == self.shell.end_string.len() {
                                                        (JobOut::None, true)
                                                    } else {
                                                        v.truncate(
                                                            v.len() - self.shell.end_string.len(),
                                                        );
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
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        if buf_size == n_bytes_read {
                            buf_size = (buf_size * 14 / 10).min(2048);
                        }
                    } else {
                        return false;
                    }
                    if complete {
                        self.sender
                            .send((i, stdout, stderr))
                            .expect("Failed to send job to main thread");
                        return false;
                    }
                }
            }
            Message::Quit => {
                self.shell.kill();
                return true;
            }
        }
    }
}

fn main() {
    let args = Arc::new(Args::parse());
    if args.job.is_empty() {
        eprintln!("rp: Must provide command to execute");
        std::process::exit(exitcode::USAGE);
    }
    let input = io::stdin();

    let jobs = args.jobs.unwrap_or(thread_pool::max_par() - 1);

    let channel_size = jobs;
    let (job_sender, job_receiver) = unbounded::<Message>();
    let (stdout_sender, stdout_receiver) = unbounded();

    let handles: Vec<JoinHandle<Result<(), ()>>> = (0..jobs)
        .map(|_| {
            let mut job_executor = JobExecutor::new(args.clone(), stdout_sender.clone());
            let job_receiver = job_receiver.clone();
            return thread::spawn(move || -> Result<(), ()> {
                for job in job_receiver {
                    let complete = job_executor.execute_job(job);
                    if complete {
                        break;
                    }
                }
                Ok(())
            });
        })
        .collect();

    // all steps necessary in order to preparing to start
    drop(stdout_sender);
    let mut lines_iter = input
        .lines()
        .map(|res| res.expect("Error reading line from stdin"))
        .enumerate()
        .peekable();
    // fill jobs initially
    for _ in 0..channel_size {
        let (i, line_res) = lines_iter.next().unwrap();
        job_sender
            .send(Message::Job((i, line_res)))
            .expect("Error sending job to thread");
    }

    let mut waiting_room = WaitingRoom::new(print_outputs);
    for (i, job_output, job_err) in stdout_receiver {
        if lines_iter.peek().is_some() {
            let n_messages_under = channel_size - job_sender.len();
            for _ in 0..n_messages_under {
                match lines_iter.next() {
                    Some((id, job)) => {
                        job_sender
                            .send(Message::Job((id, job)))
                            .expect("Broker: Unable to send job");
                    }
                    _ => {}
                }
            }
        } else {
            job_sender
                .send(Message::Quit)
                .expect("Unable to send quit message");
        }
        if args.keep_order {
            waiting_room.serve_customer(i, (job_output, job_err));
        } else {
            print_outputs((job_output, job_err));
        }
    }

    // Send lines to the channel from the main thread
    for handle in handles {
        let _ = handle.join();
    }
}
