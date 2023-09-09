use clap::Parser;
use ripparallel::thread_pool;
use crossbeam_channel::{unbounded, Receiver, Sender};
use std::io::{self, Write};
use std::process::Command;
use std::sync::Arc;
use std::thread;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, trailing_var_arg = true)]
struct Args {
    /// Number of jobs
    #[arg(short, long)]
    jobs: Option<usize>,

    /// Placeholder
    #[arg(short = 'I')]
    replace_string: Option<String>,

    /// job to run
    job: Vec<String>,
}

fn the_thing(line: String, command: &Arc<Vec<String>>) -> Vec<u8> {
    let proper_command: String = command.iter().fold(String::new(), |mut s, c| {
        s.push_str(" ");
        s.push_str(match c.as_str() {
            "{}" => &line,
            _ => c,
        });
        s
    });
    let res = Command::new("sh")
        .arg("-c")
        .arg(&proper_command)
        .output()
        .expect("deez nuts broken");
    res.stdout
}

struct Senders<T> {
    senders: Vec<Sender<T>>,
    cur: usize,
    size: usize,
}

impl<T> Senders<T> {
    fn new(senders: Vec<Sender<T>>) -> Self {
        let len = senders.len();
        Senders {
            senders,
            cur: len,
            size: len,
        }
    }

    fn send_next(&mut self, data: T) {
        if self.cur == self.size {
            self.cur = 0;
        } else {
            self.cur += 1;
        }
        let sender = self.senders.get(self.cur).unwrap();
        sender.send(data).unwrap();
    }
}

fn main() {
    let args = Args::parse();
    let input = io::stdin();

    let jobs = args.jobs.unwrap_or(thread_pool::max_par());
    let command = Arc::new(args.job);
    let mut senders = Senders::new(Vec::with_capacity(jobs));
    let mut receivers = Vec::with_capacity(jobs);
    (0..jobs).for_each(|_| {
        // Create a channel for threads to send lines
        let (sender, receiver) = unbounded();
        senders.senders.push(sender);
        receivers.push(receiver);
    });

    let (stdout_sender, stdout_receiver): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = unbounded();
    // print lines to stdout
    for line in input.lines() {
        senders.send_next(line.unwrap());
    }

    // Create a thread pool with 10 threads
    let mut handles = vec![];
    for receiver in receivers.iter() {
        let this_receiver = receiver.clone();
        let this_sender = stdout_sender.clone();
        let this_command = Arc::clone(&command);
        let handle = thread::spawn(move || {
            for line in this_receiver {
                let res = the_thing(line, &this_command);
                this_sender.send(res).unwrap();
            }
        });
        handles.push(handle);
    }

    drop(senders);
    drop(stdout_sender);
    let stdout = std::io::stdout();
    for res in stdout_receiver {
        stdout.lock().write(res.as_slice()).unwrap();
    }

    // Send lines to the channel from the main thread
    for handle in handles {
        handle.join().unwrap();
    }
}
