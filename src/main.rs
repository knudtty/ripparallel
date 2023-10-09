use clap::Parser;
use crossbeam_channel::unbounded;
use exitcode;
use ripparallel::{
    jobs::{JobOut, Message},
    ordering::WaitingRoom,
    shell::Shell,
    thread_pool,
    tokenize::Token,
};
use std::io::{self, Seek, Write};
use std::sync::Arc;
use std::thread;

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

struct CommandIterator<'a> {
    vec: &'a Vec<String>,
    ch: Option<char>,
    vec_idx: usize,
    read_idx: usize,
}

impl<'a> CommandIterator<'a> {
    // Create an iterator from a Vec<String>
    fn from_vec(vec: &'a Vec<String>) -> Self {
        CommandIterator {
            vec,
            ch: None,
            vec_idx: 0,
            read_idx: 0,
        }
    }

    fn read_char(&mut self) {
        match self.vec.get(self.vec_idx) {
            Some(v) => {
                self.ch = v.chars().nth(self.read_idx);
                if self.ch.is_some() {
                    self.read_idx += 1;
                } else {
                    self.read_idx = 0;
                }
            }
            None => {}
        }
    }
}

impl<'a> Iterator for CommandIterator<'a> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if self.vec.len() == 1 {
            self.read_char();
            return self.ch;
        } else {
            self.read_char();
            if self.ch == None {
                self.vec_idx += 1;
                if self.vec_idx < self.vec.len() {
                    return Some(' ');
                } else {
                    return None;
                }
            } else {
                return self.ch;
            }
        }
    }
}

fn pre_parse_command(command_args: &Vec<String>, quotes: bool) -> Vec<Token> {
    println!("command args: {:?}", command_args);
    let cmditer = CommandIterator::from_vec(command_args);
    let command: String = cmditer.collect();
    //println!("command: {:?}", command);
    let mut tokens = Token::get_tokens(command, quotes);
    // TODO: Fix this case: `seq 1 100 | rp echo {fjkdsal dfjskal {} {}
    //println!("Tokens: {:?}", tokens);
    if !tokens.iter().any(|token| *token == Token::Substitute) {
        match tokens.last_mut() {
            Some(Token::Literal(literal)) => literal.push(' '),
            _ => {}
        }
        tokens.push(Token::Substitute);
    }
    tokens
}

fn parse_command(tokens: &Vec<Token>, job: String) -> String {
    let mut substituted = false;
    let mut parsed = tokens.iter().fold(String::new(), |mut out, token| {
        out.push_str(match token {
            Token::Literal(s) => &s,
            Token::Substitute => {
                substituted = true;
                &job
            }
        });
        out
    });
    if !substituted {
        parsed.push(' ');
        parsed.push_str(job.as_str());
    }
    parsed
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

fn main() {
    let args = Arc::new(Args::parse());
    if args.job.is_empty() {
        eprintln!("rp: Must provide command to execute");
        std::process::exit(exitcode::USAGE);
    }
    let job_tokens = Arc::new(pre_parse_command(&args.job, args.quotes));
    let input = io::stdin();

    let jobs = args.jobs.unwrap_or(thread_pool::max_par() - 1);

    let (job_sender, job_receiver) = unbounded::<Message>();
    let (stdout_sender, stdout_receiver) = unbounded();

    // all steps necessary in order to preparing to start
    let mut lines_iter = input
        .lines()
        .map(|res| res.expect("Error reading line from stdin"))
        .enumerate()
        .peekable();
    // fill jobs initially
    let mut handles = Vec::new();
    for _ in 0..jobs {
        match lines_iter.next() {
            Some(line) => {
                let stdout_sender = stdout_sender.clone();
                let job_receiver = job_receiver.clone();
                let job_tokens = job_tokens.clone();
                let args = args.clone();
                handles.push(thread::spawn(move || -> Result<(), ()> {
                    let mut shell = match args.shell.clone() {
                        Some(shell) => Shell::new_with(shell.as_str()),
                        None => Shell::new(),
                    };
                    let mut job = Message::Job(line);
                    loop {
                        match job {
                            Message::Job(job) => {
                                let command = parse_command(&job_tokens, job.1);
                                if args.dryrun {
                                    let command = command.clone();
                                    stdout_sender
                                        .send((
                                            job.0,
                                            JobOut::Memory(command.as_bytes().to_vec()),
                                            JobOut::None,
                                        ))
                                        .expect("Failed to send job to main thread");
                                } else {
                                    let shell_res = shell.execute(command.as_str());
                                    stdout_sender
                                        .send((job.0, shell_res.0, shell_res.1))
                                        .expect("bingbong");
                                }
                            }
                            Message::Quit => {
                                shell.kill();
                                break;
                            }
                        };
                        match job_receiver.recv() {
                            Ok(res) => job = res,
                            Err(_) => break,
                        }
                    }
                    Ok(())
                }));
            }
            None => break,
        }
    }
    let channel_size = handles.len();
    drop(stdout_sender);

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
