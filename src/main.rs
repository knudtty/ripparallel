use clap::Parser;
use crossbeam_channel::{unbounded, Receiver, Sender};
use fpar::thread_pool;
use std::io::{self, Write};
use std::process::Command;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, trailing_var_arg = true)]
struct Args {
    /// Number of jobs
    #[arg(short, long)]
    jobs: Option<usize>,

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

    let (sender, receiver) = unbounded();

    let (stdout_sender, stdout_receiver): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = unbounded();

    // print lines to stdout
    thread::spawn(move || {
        for line in input.lines() {
            sender.send(line.unwrap()).unwrap();
        }
    });

    let handles: Vec<JoinHandle<()>> = (0..jobs).map(|_| {
        let receiver = receiver.clone();
        let stdout_sender = stdout_sender.clone();
        let this_command = Arc::clone(&command);
        return thread::spawn(move || {
            for line in receiver {
                let res = the_thing(line, &this_command);
                stdout_sender.send(res).unwrap();
            }
        })
    }).collect();

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
