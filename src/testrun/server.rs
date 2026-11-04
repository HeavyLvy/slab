//! A Minecraft dedicated server run as a child process: its log is read on threads, and commands
//! go to its console.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

pub struct Server {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    syncs: u32,
}

impl Server {
    pub fn start(java: &Path, jar: &Path, dir: &Path) -> Result<Server, String> {
        let mut child = Command::new(java)
            .args(["-Xmx1G", "-XX:+UseSerialGC", "-jar"])
            .arg(jar)
            .arg("--nogui")
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", java.display()))?;
        let (tx, lines) = mpsc::channel();
        let out = child.stdout.take().ok_or("no server output")?;
        let err = child.stderr.take().ok_or("no server error output")?;
        for stream in [Box::new(out) as Box<dyn std::io::Read + Send>, Box::new(err)] {
            let tx = tx.clone();
            thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        let stdin = child.stdin.take().ok_or("no server input")?;
        Ok(Server { child, stdin, lines, syncs: 0 })
    }

    /// Runs a console command (it executes on the server's next tick).
    pub fn send(&mut self, command: &str) {
        let _ = writeln!(self.stdin, "{command}");
        let _ = self.stdin.flush();
    }

    /// Reads log lines until one satisfies `found`, or the time is up. Returns every line read
    /// (the matching one last) and whether it was found.
    pub fn read_until(&mut self, timeout: Duration, found: impl Fn(&str) -> bool) -> (Vec<String>, bool) {
        let end = Instant::now() + timeout;
        let mut read = Vec::new();
        loop {
            let left = end.saturating_duration_since(Instant::now());
            match self.lines.recv_timeout(left.max(Duration::from_millis(1))) {
                Ok(line) => {
                    let hit = found(&line);
                    read.push(line);
                    if hit {
                        return (read, true);
                    }
                }
                Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => return (read, false),
            }
            if Instant::now() >= end {
                return (read, false);
            }
        }
    }

    /// Waits until the server has run everything sent so far: a marker message goes after it, and
    /// the log is read up to that marker. Returns the lines before it.
    pub fn sync(&mut self, timeout: Duration) -> Vec<String> {
        self.syncs += 1;
        let marker = format!("[slab-test-sync {}]", self.syncs);
        let say = crate::datapack::Command::say(marker.clone()).to_string();
        self.send(&say);
        let (mut lines, ok) = self.read_until(timeout, |l| l.contains(&marker));
        if ok {
            lines.pop();
        }
        lines
    }

    /// The next log line, if one comes within `timeout`.
    pub fn next_line(&mut self, timeout: Duration) -> Option<String> {
        self.lines.recv_timeout(timeout).ok()
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Stops the server, and kills it when it does not stop by itself.
    pub fn stop(&mut self) {
        self.send(&crate::datapack::Command::Stop.to_string());
        let end = Instant::now() + Duration::from_secs(30);
        while Instant::now() < end {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
