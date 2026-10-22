//! Runs the compiled server binary the way Kubernetes does: start, then SIGTERM.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

#[test]
fn binary_serves_until_terminated() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_backend"))
        .env("BACKEND_ADDR", "127.0.0.1:0")
        .stdout(Stdio::piped())
        .spawn()
        .expect("start backend");
    let mut line = String::new();
    BufReader::new(child.stdout.take().expect("stdout"))
        .read_line(&mut line)
        .expect("startup line");
    assert!(
        line.starts_with("signup backend listening on 127.0.0.1:"),
        "unexpected output: {line}"
    );

    let killed = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("send SIGTERM");
    assert!(killed.success());
    assert!(child.wait().expect("exit status").success());
}
