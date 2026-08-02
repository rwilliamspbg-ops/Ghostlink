//! Spawns the real `ghost-link` binary as two separate OS processes — a
//! `stage-worker` and a `flow --remote-addr` coordinator — and asserts they
//! actually talk to each other over a real TCP socket. This is deliberately
//! NOT a unit test calling `execute_pipeline_with_remote_stage` in-process:
//! the point is to prove the CLI-level feature works across a genuine
//! process boundary, the same way a user invoking it from two terminals
//! would exercise it. `env!("CARGO_BIN_EXE_ghost-link")` is required here —
//! `std::env::current_exe()` inside a test resolves to the test harness
//! binary, not `ghost-link.exe`.

use std::io::Read;
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::Duration;

#[test]
fn flow_remote_addr_executes_across_real_processes() {
    let bin = env!("CARGO_BIN_EXE_ghost-link");
    // Avoid fixed-port collisions on shared CI runners.
    let bind_addr = {
        let listener =
            TcpListener::bind("127.0.0.1:0").expect("failed to reserve an ephemeral loopback port");
        listener
            .local_addr()
            .expect("failed to read ephemeral bind address")
            .to_string()
    };

    let mut worker = Command::new(bin)
        .arg("stage-worker")
        .arg(&bind_addr)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn stage-worker child process");

    // Give the worker time to bind before the coordinator dials it —
    // matches print_cluster_start's own settle delay for the same reason.
    std::thread::sleep(Duration::from_millis(300));

    let flow_output = Command::new(bin)
        .args([
            "flow",
            "node-local",
            "node-remote",
            "32",
            "32",
            "8",
            "4",
            "tcp",
            "--remote-addr",
            &bind_addr,
        ])
        .output()
        .expect("failed to run flow child process");

    let worker_status = worker
        .wait_timeout_or_kill(Duration::from_secs(20))
        .expect("stage-worker did not exit after handling the coordinator");

    let mut worker_stdout = String::new();
    let mut worker_stderr = String::new();
    worker
        .stdout
        .take()
        .expect("worker stdout was not piped")
        .read_to_string(&mut worker_stdout)
        .expect("failed to read worker stdout");
    worker
        .stderr
        .take()
        .expect("worker stderr was not piped")
        .read_to_string(&mut worker_stderr)
        .expect("failed to read worker stderr");

    let flow_stdout = String::from_utf8_lossy(&flow_output.stdout);

    assert!(
        flow_output.status.success(),
        "flow process exited with {:?}\nstdout:\n{flow_stdout}\nstderr:\n{}",
        flow_output.status,
        String::from_utf8_lossy(&flow_output.stderr)
    );
    assert!(
        worker_status.success(),
        "stage-worker process exited with {worker_status:?}\nstdout:\n{worker_stdout}\nstderr:\n{worker_stderr}"
    );

    assert!(
        flow_stdout.contains("REAL execution"),
        "expected the real cross-process path, not the simulated fallback:\n{flow_stdout}"
    );
    assert!(
        !flow_stdout.contains("SIMULATED execution"),
        "flow unexpectedly fell back to single-process simulation:\n{flow_stdout}"
    );

    assert!(
        worker_stdout.contains("Done: processed"),
        "worker never reported completing a batch exchange:\n{worker_stdout}"
    );
    assert!(
        !worker_stdout.contains("processed 0 batch"),
        "worker reported zero batches processed — no real work crossed the socket:\n{worker_stdout}"
    );
}

/// `std::process::Child` has no built-in wait-with-timeout, so this polls —
/// the same approach used to observe subprocess lifecycles earlier in this
/// project's manual debugging (a `Get-CimInstance` poll loop), just ported
/// into Rust for an automated test.
trait WaitTimeout {
    fn wait_timeout_or_kill(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<std::process::ExitStatus>;
}

impl WaitTimeout for std::process::Child {
    fn wait_timeout_or_kill(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<std::process::ExitStatus> {
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            if start.elapsed() > timeout {
                let _ = self.kill();
                let _ = self.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "process did not exit within the timeout",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
