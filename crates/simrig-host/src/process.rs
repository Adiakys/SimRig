//! A running plugin adapter and the JSON-RPC client talking to it.

use std::collections::{HashMap, VecDeque};
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;
use simrig_protocol::{Message, method, read_message, write_message};

use crate::error::HostError;

const STDERR_LINES: usize = 200;

type Pending = Arc<Mutex<HashMap<u64, Sender<Message>>>>;

pub(crate) struct PluginProcess {
    plugin: String,
    child: Child,
    stdin: ChildStdin,
    next_id: u64,
    pending: Pending,
    notifications: Receiver<Message>,
    stderr: Arc<Mutex<VecDeque<String>>>,
    timeout: Duration,
}

impl PluginProcess {
    pub(crate) fn spawn(
        plugin: &str,
        exec: &Path,
        dir: &Path,
        timeout: Duration,
    ) -> io::Result<Self> {
        let mut child = Command::new(exec)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("no stderr"))?;

        let pending: Pending = Arc::default();
        let (notify_tx, notifications) = channel();
        let routes = Arc::clone(&pending);
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(message)) = read_message(&mut reader) {
                if message.is_response() {
                    let sender = message.id.and_then(|id| routes.lock().ok()?.remove(&id));
                    if let Some(sender) = sender {
                        let _ = sender.send(message);
                    }
                } else if message.is_notification() && notify_tx.send(message).is_err() {
                    break;
                }
            }
            // The plugin is gone: dropping the senders fails every pending call.
            if let Ok(mut routes) = routes.lock() {
                routes.clear();
            }
        });

        let log: Arc<Mutex<VecDeque<String>>> = Arc::default();
        let lines = Arc::clone(&log);
        let echo = std::env::var_os("SIMRIG_PLUGIN_LOG").is_some();
        let name = plugin.to_string();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if echo {
                    eprintln!("[{name}] {line}");
                }
                if let Ok(mut lines) = lines.lock() {
                    if lines.len() == STDERR_LINES {
                        lines.pop_front();
                    }
                    lines.push_back(line);
                }
            }
        });

        Ok(Self {
            plugin: plugin.into(),
            child,
            stdin,
            next_id: 1,
            pending,
            notifications,
            stderr: log,
            timeout,
        })
    }

    pub(crate) fn call<P: Serialize, R: DeserializeOwned>(
        &mut self,
        name: &str,
        params: P,
    ) -> Result<R, HostError> {
        let id = self.next_id;
        self.next_id += 1;
        let (tx, rx) = channel();
        self.pending
            .lock()
            .map_err(|_| HostError::PluginExited(self.plugin.clone()))?
            .insert(id, tx);
        if write_message(&mut self.stdin, &Message::request(id, name, params)).is_err() {
            return Err(HostError::PluginExited(self.plugin.clone()));
        }
        let response = match rx.recv_timeout(self.timeout) {
            Ok(response) => response,
            Err(RecvTimeoutError::Timeout) => {
                if let Ok(mut pending) = self.pending.lock() {
                    pending.remove(&id);
                }
                return Err(HostError::Timeout {
                    plugin: self.plugin.clone(),
                    method: name.into(),
                });
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(HostError::PluginExited(self.plugin.clone()));
            }
        };
        if let Some(error) = response.error {
            return Err(HostError::from_rpc(&self.plugin, error));
        }
        let result = response.result.unwrap_or(serde_json::Value::Null);
        serde_json::from_value(result).map_err(|e| HostError::Plugin {
            plugin: self.plugin.clone(),
            error: simrig_protocol::RpcError::new(
                simrig_protocol::code::INTERNAL,
                format!("bad result: {e}"),
            ),
        })
    }

    /// Next notification, waiting at most until `deadline`.
    pub(crate) fn next_notification(&self, deadline: Instant) -> Option<Message> {
        self.notifications
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()
    }

    /// Notifications already received.
    pub(crate) fn pending_notifications(&self) -> Vec<Message> {
        self.notifications.try_iter().collect()
    }

    /// Last lines the plugin wrote on stderr.
    pub(crate) fn stderr_tail(&self) -> Vec<String> {
        self.stderr
            .lock()
            .map(|l| l.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Asks the plugin to exit; kills it if it does not within the timeout.
    pub(crate) fn shutdown(mut self) {
        let _: Result<(), _> = self.call(method::SHUTDOWN, ());
        let deadline = Instant::now() + self.timeout;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
