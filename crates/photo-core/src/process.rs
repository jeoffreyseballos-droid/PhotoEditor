//! Private, bounded helper transport. Production adapters choose fixed programs and arguments.
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) fn output(
    command: &mut Command,
    input: &[u8],
    limit: usize,
    timeout: Duration,
) -> Result<(Vec<u8>, String), String> {
    output_cancellable(
        command,
        input,
        limit,
        timeout,
        &photo_contracts::CancellationToken::default(),
    )
}

pub(crate) fn output_cancellable(
    command: &mut Command,
    input: &[u8],
    limit: usize,
    timeout: Duration,
    cancel: &photo_contracts::CancellationToken,
) -> Result<(Vec<u8>, String), String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("Metadata helper could not start: {e}"))?;
    let start = Instant::now();
    let mut stdin = child.stdin.take().unwrap();
    let input = input.to_vec();
    let writer = thread::spawn(move || stdin.write_all(&input));
    let exceeded = Arc::new(AtomicBool::new(false));
    let read = |mut source: Box<dyn Read + Send>, cap: usize, signal: Arc<AtomicBool>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = source.by_ref().take(cap as u64 + 1).read_to_end(&mut bytes);
            if bytes.len() > cap {
                signal.store(true, Ordering::Release);
            }
            result.map(|_| bytes)
        })
    };
    let stdout = read(
        Box::new(child.stdout.take().unwrap()),
        limit,
        exceeded.clone(),
    );
    let stderr = read(
        Box::new(child.stderr.take().unwrap()),
        64 * 1024,
        exceeded.clone(),
    );
    let mut failure = None;
    loop {
        if cancel.is_cancelled() || exceeded.load(Ordering::Acquire) || start.elapsed() > timeout {
            failure =
                Some("Metadata/preview helper exceeded its output or time budget.".to_owned());
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                failure = Some(error.to_string());
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let out = stdout
        .join()
        .map_err(|_| "Helper output reader failed")?
        .map_err(|e| e.to_string())?;
    let err = stderr
        .join()
        .map_err(|_| "Helper error reader failed")?
        .map_err(|e| e.to_string())?;
    if exceeded.load(Ordering::Acquire) {
        failure = Some("Metadata/preview helper output exceeded the safety budget.".into());
    }
    if let Some(error) = failure {
        let _ = writer.join();
        return Err(error);
    }
    writer
        .join()
        .map_err(|_| "Helper input writer failed")?
        .map_err(|e| e.to_string())?;
    Ok((
        out,
        String::from_utf8_lossy(&err)
            .trim()
            .chars()
            .take(2048)
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_kills_and_reaps_running_helper() {
        #[cfg(windows)]
        let mut command = {
            let mut c = Command::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe");
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 20",
            ]);
            c
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut c = Command::new("/bin/sleep");
            c.arg("20");
            c
        };
        let token = photo_contracts::CancellationToken::default();
        let signal = token.clone();
        let cancel_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            signal.cancel();
        });
        let start = Instant::now();
        let result = output_cancellable(&mut command, b"", 1024, Duration::from_secs(30), &token);
        cancel_thread.join().unwrap();
        assert!(result.is_err());
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
