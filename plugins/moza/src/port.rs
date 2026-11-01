//! The serial port of a base, served by a thread of its own, as Boxflat does: it asks over and
//! over what is plugged into the base and which rim is mounted, and runs the plugin's requests
//! in between. The plugin reads what it heard without waiting; only a setting read or written
//! waits for its answer.

use std::io;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::protocol::{self, Target};
use crate::serial::{ANSWER, Link};

/// How often the base is asked what is plugged into it.
const POLL: Duration = Duration::from_secs(1);

/// How often a rim is looked for while none answers: asking every id takes seconds.
const RIM_LOOK: Duration = Duration::from_secs(5);

/// How long a rim has to answer when it is looked for: through the base it can take longer than
/// the base itself (Boxflat waits until its next poll, seconds later).
const RIM_ANSWER: Duration = Duration::from_secs(1);

/// What answers every rim: Boxflat reads it to know one is mounted.
const RIM_PROBE: &str = "wheel-telemetry-mode";

/// What the port heard last.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heard {
    /// The base was asked at least once.
    pub polled: bool,
    /// The rim's device id, while one answers.
    pub rim: Option<u8>,
    /// How many times a rim started answering: another count may be another rim.
    pub rims: u32,
    pub shifter: Option<ShifterInfo>,
    pub handbrake: bool,
    /// The port failed (unplugged, replaced): it is to be opened again.
    pub failed: bool,
}

/// A shifter answering at device id 26, read when it starts answering: its lever's height says
/// an HGP (past 100, Boxflat), and an SGP's paddle synchronization and direction its buttons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShifterInfo {
    pub h_pattern: bool,
    pub paddles: bool,
    pub reversed: bool,
}

/// A request of the plugin: a frame, and the answer to wait for.
struct Job {
    request: Vec<u8>,
    group: u8,
    device: u8,
    id: Vec<u8>,
    wait: Duration,
    reply: Sender<io::Result<Option<u64>>>,
}

pub struct Port {
    jobs: Sender<Job>,
    heard: Arc<Mutex<Heard>>,
}

impl Port {
    /// Serves `link` on a thread of its own, until the port is dropped or fails.
    pub fn start(link: Box<dyn Link>) -> Self {
        let (jobs, inbox) = mpsc::channel();
        let heard = Arc::new(Mutex::new(Heard::default()));
        let worker = Worker {
            link,
            inbox,
            heard: heard.clone(),
            rim_looked: None,
        };
        thread::spawn(move || worker.serve());
        Self { jobs, heard }
    }

    pub fn heard(&self) -> Heard {
        self.heard
            .lock()
            .map(|h| h.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    /// `Link::exchange`, through the thread.
    pub fn exchange(
        &self,
        request: &[u8],
        (group, device, id): (u8, u8, &[u8]),
        wait: Duration,
    ) -> io::Result<Option<u64>> {
        let closed = || io::Error::other("The serial port was closed.");
        let (reply, answer) = mpsc::channel();
        let job = Job {
            request: request.to_vec(),
            group,
            device,
            id: id.to_vec(),
            wait,
            reply,
        };
        self.jobs.send(job).map_err(|_| closed())?;
        answer.recv().map_err(|_| closed())?
    }
}

/// The port failed: the thread ends.
struct Failed;

struct Worker {
    link: Box<dyn Link>,
    inbox: Receiver<Job>,
    heard: Arc<Mutex<Heard>>,
    rim_looked: Option<Instant>,
}

impl Worker {
    fn serve(mut self) {
        let mut next = Instant::now();
        loop {
            let done = match self
                .inbox
                .recv_timeout(next.saturating_duration_since(Instant::now()))
            {
                Ok(job) => self.run(job),
                Err(RecvTimeoutError::Timeout) => {
                    next = Instant::now() + POLL;
                    self.poll()
                }
                // The device was detached.
                Err(RecvTimeoutError::Disconnected) => return,
            };
            if done.is_err() {
                self.update(|h| h.failed = true);
                return;
            }
        }
    }

    fn run(&mut self, job: Job) -> Result<(), Failed> {
        let answer = self
            .link
            .exchange(&job.request, (job.group, job.device, &job.id), job.wait);
        let failed = answer.is_err();
        let _ = job.reply.send(answer);
        if failed { Err(Failed) } else { Ok(()) }
    }

    /// Runs the plugin's requests waiting, then reads `name` at `device` (its target's when
    /// `None`).
    fn ask(
        &mut self,
        name: &str,
        device: Option<u8>,
        wait: Duration,
    ) -> Result<Option<u64>, Failed> {
        while let Ok(job) = self.inbox.try_recv() {
            self.run(job)?;
        }
        let Some(command) = protocol::command(name) else {
            return Ok(None);
        };
        let device = device.unwrap_or(command.target.ids()[0]);
        let (Some((group, id)), Some(frame)) =
            (command.read, protocol::read_frame(command, device))
        else {
            return Ok(None);
        };
        self.link
            .exchange(&frame, (group, device, id), wait)
            .map_err(|_| Failed)
    }

    /// Asks the base what is plugged into it, as Boxflat does: a shifter answers at 26, a
    /// handbrake at 27; the rim is checked at its id, and looked for at every id now and then
    /// while none answers.
    fn poll(&mut self) -> Result<(), Failed> {
        let was = self
            .heard
            .lock()
            .map(|h| h.clone())
            .unwrap_or_else(|e| e.into_inner().clone());
        let shifter = match (self.ask("sequential-output-y", None, ANSWER)?, was.shifter) {
            (None, _) => None,
            (Some(_), Some(known)) => Some(known),
            (Some(height), None) => Some(ShifterInfo {
                h_pattern: height > 100,
                paddles: self.ask("sequential-paddle-sync", None, ANSWER)? == Some(2),
                reversed: self.ask("sequential-direction", None, ANSWER)? == Some(1),
            }),
        };
        let handbrake = self.ask("handbrake-direction", None, ANSWER)?.is_some();
        let rim = match was.rim {
            Some(id) if self.ask(RIM_PROBE, Some(id), RIM_ANSWER)?.is_some() => Some(id),
            _ if self.rim_looked.is_none_or(|at| at.elapsed() >= RIM_LOOK) => {
                self.rim_looked = Some(Instant::now());
                let mut found = None;
                for &id in Target::Wheel.ids() {
                    if self.ask(RIM_PROBE, Some(id), RIM_ANSWER)?.is_some() {
                        found = Some(id);
                        break;
                    }
                }
                found
            }
            _ => None,
        };
        self.update(|h| {
            if rim.is_some() && rim != h.rim {
                h.rims += 1;
            }
            h.rim = rim;
            h.shifter = shifter;
            h.handbrake = handbrake;
            h.polled = true;
        });
        Ok(())
    }

    fn update(&self, change: impl FnOnce(&mut Heard)) {
        let mut heard = self.heard.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut heard);
    }
}
