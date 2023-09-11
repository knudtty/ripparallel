use clap::Parser;
use crossbeam_channel::bounded;
use ripparallel::thread_pool;
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

    /// Placeholder
    #[arg(short = 'I')]
    replace_string: Option<String>,

    /// Maintain order that inputs came in
    #[arg(short, long)]
    order: Option<bool>,

    /// job to run
    job: Vec<String>,
}

enum Message {
    Job((usize, String)),
    Quit,
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

    let jobs = args.jobs.unwrap_or(thread_pool::max_par() - 1);
    let command = Arc::new(args.job);

    let (job_sender, job_receiver) = bounded(jobs);
    let (stdout_sender, stdout_receiver) = bounded(jobs);

    // job_sender.len()
    let handles: Vec<JoinHandle<()>> = (0..jobs)
        .map(|_| {
            let stdout_sender = stdout_sender.clone();
            let this_command = Arc::clone(&command);
            let job_receiver = job_receiver.clone();
            return thread::spawn(move || {
                // single receiver shared between all threads
                for job in job_receiver {
                    match job {
                        Message::Job((i, job_input)) => {
                            let res = the_thing(job_input, &this_command);
                            stdout_sender.send((i, res)).unwrap();
                        },
                        Message::Quit => break
                    }
                }
            });
        })
        .collect();

    // all steps necessary in order to preparing to start
    drop(stdout_sender);
    let mut lines_iter = input.lines().enumerate().peekable();
    // fill jobs initially
    for _ in 0..jobs {
        let (i, line_res) = lines_iter.next().unwrap();
        job_sender
            .send(Message::Job((i, line_res.unwrap())))
            .unwrap();
    }

    // TODO: Make this ordering thing into a function
    let mut next_customer = 0;
    let mut waiting_room: Vec<(usize, JobResult)> = Vec::new();
    let mut stdout = std::io::stdout();
    for (i, res) in stdout_receiver {
        // before calling waiting room function
        if lines_iter.peek().is_some() {
            let n_messages_under = jobs - job_sender.len();
            //println!("n_messages_under: {}", n_messages_under);
            for _ in 0..n_messages_under {
                match lines_iter.next() {
                    Some((id, Ok(job))) => {
                        job_sender
                            .send(Message::Job((id, job)))
                            .expect("Broker: Unable to send job");
                    }
                    _ => {}
                }
                break;
            }
        } else {
            job_sender.send(Message::Quit).expect("Unable to send quit message");
        }
        // before calling waiting room function
        if i == next_customer {
            let mut new_customer = res;
            loop {
                // portable function part
                // increment next_customer and write to stdout
                stdout.write_all(new_customer.as_slice()).unwrap();
                // portable function part
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
