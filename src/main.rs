use clap::Parser;
use crossbeam_channel::bounded;
use ripparallel::shell::Shell;
use ripparallel::thread_pool;
use std::io::{self, Write};
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

fn parse_command(command_args: Arc<Vec<String>>, job_input: String) -> String {
    let cmd_args_len = command_args.len();
    let cmditer = command_args.iter().enumerate();
    let mut command = String::new();
    let mut substituted = false;
    cmditer.for_each(|(i, arg)| {
        if arg.as_str() == "{}" {
            substituted = true;
            command.push_str(job_input.as_str());
            command.push(' ');
        } else {
            command.push_str(arg.as_str());
            if i - 1 != cmd_args_len  {
                command.push(' ');
            }
        }
    });
    if !substituted {
        command.push_str(job_input.as_str());
    }
    command
}
fn main() {
    let args = Args::parse();
    let input = io::stdin();

    let jobs = args.jobs.unwrap_or(thread_pool::max_par() - 1);
    let command = Arc::new(args.job);

    let channel_size = jobs;
    let (job_sender, job_receiver) = bounded(channel_size);
    let (stdout_sender, stdout_receiver) = bounded(channel_size);

    let handles: Vec<JoinHandle<Result<(), ()>>> = (0..jobs)
        .map(|_| {
            let stdout_sender = stdout_sender.clone();
            let command_args = Arc::clone(&command);
            let job_receiver = job_receiver.clone();
            return thread::spawn(move || -> Result<(), ()> {
                let mut shell = Shell::birth();
                // single receiver shared between all threads
                for job in job_receiver {
                    match job {
                        Message::Job((i, job_input)) => {
                            let command_args = command_args.clone();
                            let command = parse_command(command_args, job_input);
                            shell.feed(&command);
                            let res = shell.harass();
                            stdout_sender.send((i, res)).expect("Failed to send job to main thread");
                        }
                        Message::Quit => {
                            shell.kill();
                            break;
                        }
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
        let (i, line_res) = unsafe { lines_iter.next().unwrap_unchecked() };
        job_sender
            .send(Message::Job((i, line_res)))
            .expect("Error sending job to thread");
    }

    // TODO: Make this ordering thing into a function
    let mut next_customer = 0;
    let mut waiting_room: Vec<(usize, JobResult)> = Vec::new();
    let mut stdout = std::io::stdout().lock();
    for (i, output) in stdout_receiver {
        // before calling waiting room function
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
                break;
            }
        } else {
            job_sender
                .send(Message::Quit)
                .expect("Unable to send quit message");
        }
        // before calling waiting room function
        if i == next_customer {
            let mut new_customer = output;
            loop {
                // portable function part
                // increment next_customer and write to stdout
                stdout.write_all(new_customer.as_slice()).expect("Failed to write to stdout");
                // portable function part
                next_customer += 1;
                // check for next customer in waiting_room
                if let Some(idx) = waiting_room
                    .iter()
                    .position(|(customer, _)| customer == &next_customer)
                {
                    // if found, copy entry to new_customer, copy last entry into indexed spot and pop last entry.
                    new_customer = waiting_room.swap_remove(idx).1;
                } else {
                    // else break loop
                    break;
                }
            }
        } else {
            waiting_room.push((i, output));
        }
    }

    // Send lines to the channel from the main thread
    for handle in handles {
        let _ = handle.join();
    }
}
