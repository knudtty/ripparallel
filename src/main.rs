use clap::Parser;
use crossbeam_channel::bounded;
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

    /// Maintain order that inputs came in
    #[arg(short, long)]
    order: Option<bool>,

    /// job to run
    job: Vec<String>,
}

type JobResult = Vec<u8>;

fn the_thing(line: String, command: &Arc<Vec<String>>) -> JobResult {
    // TODO: Construct command without sh -c
    let mut cmditer = command.iter();
    let mut cmd: Command;
    let mut substituted = false;
    match cmditer.next().expect("shid broken").as_ref() {
        "{}" => {
            substituted = true;
            cmd = Command::new("sh");
            cmd.arg("-c").arg(line.as_str());
        }
        first => cmd = Command::new(first),
    };
    cmditer.for_each(|arg| {
        if arg.as_str() == "{}" {
            substituted = true;
            cmd.arg(line.as_str());
        } else {
            cmd.arg(arg);
        }
    });
    if !substituted {
        cmd.arg(line.as_str());
    };
    let res = cmd.output().expect("deez nuts broken");
    res.stdout.into()
}

fn main() {
    let args = Args::parse();
    let input = io::stdin();

    let jobs = args.jobs.unwrap_or(thread_pool::max_par()-1);
    let command = Arc::new(args.job);

    let (job_senders, job_receivers) =
        (0..jobs).fold((Vec::new(), Vec::new()), |(mut s, mut r), _| {
            let (sender, receiver) = bounded(1);
            s.push(sender);
            r.push(receiver);
            (s, r)
        });
    let (broker_sender, job_status_receiver) = bounded(jobs);
    let (stdout_sender, stdout_receiver) = bounded(jobs);

    let handles: Vec<JoinHandle<()>> = job_receivers
        .iter()
        .cloned()
        .enumerate()
        .map(|(id, job_receiver)| {
            let stdout_sender = stdout_sender.clone();
            let broker_sender = broker_sender.clone();
            let this_command = Arc::clone(&command);
            return thread::spawn(move || {
                // single receiver shared between all threads
                for (i, job_input) in job_receiver {
                    let res = the_thing(job_input, &this_command);
                    // TODO: Test order. I think this would be better?
                    stdout_sender.send((i, res)).unwrap();
                    let _ = broker_sender.send(id);
                }
            });
        })
        .collect();

    // Broker thread
    thread::spawn(move || {
        // It might be expensive to read from stdin. Investigate reading all to vec, then brokering
        let mut lines_iter = input.lines().enumerate();
        job_senders.iter().for_each(|sender| {
            let (i, line_res) = lines_iter.next().unwrap();
            sender.send((i, line_res.unwrap())).unwrap();
        });
        for (job_id, line_res) in lines_iter {
            match job_status_receiver.recv() {
                Ok(thread_id) => job_senders
                    .get(thread_id)
                    .unwrap()
                    .send((job_id, line_res.unwrap()))
                    .unwrap(),
                _ => {}
            }
        }
    });
    drop(stdout_sender);

    // TODO: Make into a function
    let mut next_customer = 0;
    let mut waiting_room: Vec<(usize, JobResult)> = Vec::new();
    let mut stdout = std::io::stdout();
    for (i, res) in stdout_receiver {
        if i == next_customer {
            let mut new_customer = res;
            loop {
                // increment next_customer and write to stdout
                stdout.write_all(new_customer.as_slice()).unwrap();
                next_customer += 1;
                // check for next customer in waiting_room
                if let Some(idx) = waiting_room
                    .iter()
                    .position(|(customer, _)| customer == &next_customer)
                {
                    // if found, copy entry to new_customer, copy last entry into indexed spot and pop last entry.
                    let tmp = waiting_room.last().unwrap().clone();
                    let entry = waiting_room.get_mut(idx).unwrap();
                    new_customer = entry.1.clone();
                    *entry = tmp;
                    waiting_room.pop();
                } else {
                    // else break loop
                    break;
                }
            }
        } else {
            waiting_room.push((i, res));
        }
    }

    // Send lines to the channel from the main thread
    for handle in handles {
        handle.join().unwrap();
    }
}
