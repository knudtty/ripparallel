use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ripparallel::shell::Shell;
use std::io::Read;
use std::process::{Command, Stdio};

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
    shell.execute(format!("echo {}", n.to_string()));
}

fn criterion_benchmark(c: &mut Criterion) {
    let n: usize = 1;
    //c.bench_function("command_spawn", |b| b.iter(|| command_spawn(black_box(n))));

    c.bench_function("command_shell", |b| {
        let mut shell = Shell::new();
        b.iter(|| command_shell(&mut shell, black_box(n)));
        shell.kill();
    });

    // TODO: Why is zsh getting stuck?
    //c.bench_function("zsh shell", |b| {
    //    let mut shell = Shell::new_with("zsh");
    //    b.iter(|| command_shell(&mut shell, black_box(n)));
    //    shell.kill();
    //});

    c.bench_function("bash shell", |b| {
        let mut shell = Shell::new_with("bash");
        b.iter(|| command_shell(&mut shell, black_box(n)));
        shell.kill();
    });

    c.bench_function("dash shell", |b| {
        let mut shell = Shell::new_with("dash");
        b.iter(|| command_shell(&mut shell, black_box(n)));
        shell.kill();
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
