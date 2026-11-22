//! Retained, line-framed child processes for [SYNTH-HOST-STREAM-SESSION].
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

const INPUT_LIMIT: usize = 2 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;
const STDERR_LIMIT: usize = 65_536;

enum Event {
    Line(Vec<u8>),
    Written,
    Diagnostic(Vec<u8>),
    Failed(String),
}

/// One owner serializes transactions; dropping it kills and reaps its direct child.
pub(super) struct Session {
    child: Child,
    input: Option<SyncSender<String>>,
    events: Option<Receiver<Event>>,
    diagnostics: Vec<u8>,
}

impl Session {
    pub(super) fn open(executable: &str, args: &[String]) -> Result<Self, String> {
        if !std::path::Path::new(executable).is_absolute() {
            return Err("Executable must be an absolute path".to_owned());
        }
        let child = Command::new(executable)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| error.to_string())?;
        let mut session = Self {
            child,
            input: None,
            events: None,
            diagnostics: Vec::new(),
        };
        session.pipes()?;
        Ok(session)
    }

    fn pipes(&mut self) -> Result<(), String> {
        let stdin = self.child.stdin.take().ok_or("Missing session stdin")?;
        let stdout = self.child.stdout.take().ok_or("Missing session stdout")?;
        let stderr = self.child.stderr.take().ok_or("Missing session stderr")?;
        let (sender, events) = mpsc::sync_channel(4);
        let (input, requests) = mpsc::sync_channel(1);
        let writes = sender.clone();
        std::thread::spawn(move || write_requests(stdin, requests, writes));
        let reads = sender.clone();
        std::thread::spawn(move || read_lines(stdout, reads));
        std::thread::spawn(move || drain_diagnostics(stderr, sender));
        self.input = Some(input);
        self.events = Some(events);
        Ok(())
    }

    pub(super) fn exchange(
        &mut self,
        input: &str,
        timeout: u64,
        limit: u64,
    ) -> Result<String, String> {
        if input.len() > INPUT_LIMIT || input.contains(['\n', '\r']) {
            return Err("Session input must be one line of at most 2 MiB".to_owned());
        }
        if timeout == 0 || timeout > 60_000 || limit == 0 || limit > OUTPUT_LIMIT as u64 {
            return Err("Session requires 1..60000 ms and 1..4194304 output bytes".to_owned());
        }
        let result = self.transaction(input, Duration::from_millis(timeout), limit as usize);
        if result.is_err() {
            self.stop();
        }
        result.map_err(|error| format!("{error}; stderr: {}", self.diagnostics()))
    }

    fn transaction(
        &mut self,
        input: &str,
        timeout: Duration,
        limit: usize,
    ) -> Result<String, String> {
        let started = Instant::now();
        self.input
            .as_ref()
            .ok_or("Session is closed")?
            .try_send(format!("{input}\n"))
            .map_err(|error| error.to_string())?;
        let receiver = self.events.as_ref().ok_or("Session is closed")?;
        let mut line = None;
        let mut written = false;
        self.diagnostics.clear();
        while line.is_none() || !written {
            let remaining = timeout
                .checked_sub(started.elapsed())
                .ok_or("Session timed out")?;
            let event = receiver
                .recv_timeout(remaining)
                .map_err(|error| format!("Session receive: {error}"))?;
            absorb(event, limit, &mut line, &mut written, &mut self.diagnostics)?;
        }
        String::from_utf8(line.ok_or("Missing session response")?)
            .map_err(|error| error.to_string())
    }

    pub(super) fn diagnostics(&self) -> String {
        String::from_utf8_lossy(&self.diagnostics).into_owned()
    }

    pub(super) fn stop(&mut self) {
        self.input.take();
        self.events.take();
        super::process::stop(&mut self.child);
    }

    #[cfg(test)]
    pub(super) fn id(&self) -> u32 {
        self.child.id()
    }

    #[cfg(test)]
    pub(super) fn exited(&mut self) -> Result<bool, String> {
        self.child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|error| error.to_string())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

fn absorb(
    event: Event,
    limit: usize,
    line: &mut Option<Vec<u8>>,
    written: &mut bool,
    diagnostics: &mut Vec<u8>,
) -> Result<(), String> {
    match event {
        Event::Line(bytes) if bytes.len() <= limit && line.is_none() => *line = Some(bytes),
        Event::Line(_) => return Err("Unexpected or oversized session response".to_owned()),
        Event::Written => *written = true,
        Event::Diagnostic(bytes) => {
            if diagnostics.len().saturating_add(bytes.len()) > STDERR_LIMIT {
                return Err("Session stderr exceeds 64 KiB".to_owned());
            }
            diagnostics.extend(bytes);
        }
        Event::Failed(error) => return Err(error),
    }
    Ok(())
}

fn write_requests(mut pipe: impl Write, requests: Receiver<String>, sender: SyncSender<Event>) {
    for input in requests {
        let result = pipe.write_all(input.as_bytes()).and_then(|()| pipe.flush());
        let event = match result {
            Ok(()) => Event::Written,
            Err(error) => {
                let _ = sender.send(Event::Failed(error.to_string()));
                return;
            }
        };
        if sender.send(event).is_err() {
            return;
        }
    }
}

fn read_lines(pipe: impl Read, sender: SyncSender<Event>) {
    let mut reader = BufReader::new(pipe);
    loop {
        let mut bytes = Vec::new();
        let result = reader
            .by_ref()
            .take((OUTPUT_LIMIT + 2) as u64)
            .read_until(b'\n', &mut bytes);
        let event = match result {
            Ok(_) if bytes.last() == Some(&b'\n') && bytes.len() <= OUTPUT_LIMIT + 1 => {
                bytes.pop();
                Event::Line(bytes)
            }
            Ok(_) => {
                let _ = sender.send(Event::Failed(
                    "Session stdout ended or exceeded its line limit".to_owned(),
                ));
                return;
            }
            Err(error) => {
                let _ = sender.send(Event::Failed(error.to_string()));
                return;
            }
        };
        if sender.send(event).is_err() {
            return;
        }
    }
}

fn drain_diagnostics(mut pipe: impl Read, sender: SyncSender<Event>) {
    let mut buffer = [0; 4096];
    loop {
        let event = match pipe.read(&mut buffer) {
            Ok(0) => return,
            Ok(size) => Event::Diagnostic(buffer[..size].to_vec()),
            Err(error) => {
                let _ = sender.send(Event::Failed(error.to_string()));
                return;
            }
        };
        if sender.send(event).is_err() {
            return;
        }
    }
}
