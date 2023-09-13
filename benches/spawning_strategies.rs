use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn command_spawn(n: usize) {
    use std::fmt::Write;

    // Read and print the output
    let mut output = String::new();
    let mut parent_stdout = String::new();
    // Spawn a new shell process
    let mut child = Command::new("echo")
        .arg(n.to_string())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn shell process");
    child.wait().expect("Failed to wait for child process");
    child
        .stdout
        .unwrap()
        .read_to_string(&mut output)
        .expect("failed to read to string");
    write!(parent_stdout, "{}", output).expect("Failed to write to parent stdout");
}

fn command_shell(shell: &mut Shell, n: usize) {
    // Spawn a new shell process
    let command = format!("echo {}", n.to_string());
    shell
        .stdin
        .write_all(command.as_bytes())
        .expect("Failed to write to stdin");
    shell
        .stdin
        .write_all(b"\n")
        .expect("Failed to write newline to stdin");
    let mut output: Vec<u8> = Vec::new();
    loop {
        let mut buf = [0; 10];
        match shell.stdout.read(&mut buf) {
            Ok(n_bytes_read) => {
                output.extend(buf.iter().take(n_bytes_read));
                let last_byte = buf.get(n_bytes_read - 1).unwrap();
                let want_byte = b"\x04"[0];
                if &want_byte == last_byte {
                    break;
                }
            }
            Err(_) => {
                println!("ERROR");
            }
        }
    }
}

struct Shell {
    slave: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}
impl Shell {
    fn new() -> Self {
        // Spawn a new shell process
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("while true; do read line; if [ \"$line\" = \"exit\" ]; then exit; fi; eval \"$line; printf \"\x04\"\"; done")
            .stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn shell process");

        let child_stdin = child.stdin.take().expect("Failed to open stdin");
        let child_stdout = child.stdout.take().expect("Failed to open stdout");
        Shell {
            slave: child,
            stdin: child_stdin,
            stdout: child_stdout,
        }
    }

    fn kill(&mut self) {
        self.stdin
            .write(b"exit")
            .expect("Failed to kill shell process");
    }
}
fn criterion_benchmark(c: &mut Criterion) {
    let n: usize = 1;
    c.bench_function("command_spawn", |b| b.iter(|| command_spawn(black_box(n))));

    c.bench_function("command_shell", |b| {
        let mut shell = Shell::new();
        b.iter(|| command_shell(&mut shell, black_box(n)));
        shell.kill();
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
