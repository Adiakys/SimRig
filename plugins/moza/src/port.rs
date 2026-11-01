//! The serial port of a base, served by a thread of its own, as Boxflat does: it asks over and
//! over what is plugged into the base and which rim is mounted, and runs the plugin's requests
//! in between. The plugin reads what it heard without waiting; a setting read or written waits
//! for its answer, behind at most the one question being asked.

use std::io;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::protocol::{self, Command, Target};
use crate::serial::{ANSWER, Link};

/// How often the base is asked what is plugged into it.
const POLL: Duration = Duration::from_secs(1);

/// How long the port stays with the plugin's requests after one before asking again: setting
/// reads come together (a component described).
const GRACE: Duration = Duration::from_millis(20);

/// How many polls in a row something must miss to count as gone: one late answer would have
/// every component described again.
const MISSES: u8 = 2;

/// How long a rim has to answer when it is looked for: through the base it can take longer than
/// the base itself (Boxflat waits until its next poll, seconds later).
const RIM_ANSWER: Duration = Duration::from_secs(1);

/// What answers every rim: Boxflat reads it to know one is mounted.
const RIM_PROBE: &str = "wheel-telemetry-mode";

/// What the port heard last.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heard {
    /// The base was asked at least once.
    pub polled: bool,
    /// The rim's device id, while one answers.
    pub rim: Option<u8>,
    /// How many times a rim started answering: another count may be another rim.
    pub rims: u32,
    pub shifter: Option<ShifterInfo>,
    pub handbrake: bool,
}

/// A shifter answering at device id 26, read when it starts answering: its lever's height says
/// an HGP (past 100, Boxflat), and an SGP's paddle synchronization and direction its buttons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShifterInfo {
    pub h_pattern: bool,
    pub paddles: bool,
    pub reversed: bool,
}

/// A request of the plugin: a command read, or written when it has a value.
struct Job {
    command: &'static Command,
    device: u8,
    value: Option<u64>,
    reply: Sender<io::Result<Option<u64>>>,
}

pub struct Port {
    jobs: Sender<Job>,
    heard: Arc<Mutex<Heard>>,
    thread: JoinHandle<()>,
}

impl Port {
    /// Serves `link` on a thread of its own, until the port is dropped or fails.
    pub fn start(link: Box<dyn Link>) -> Self {
        let (jobs, inbox) = mpsc::channel();
        let heard = Arc::new(Mutex::new(Heard::default()));
        let worker = Worker {
            link,
            inbox,
            shared: heard.clone(),
            now: Heard::default(),
            misses: Misses::default(),
            rim_next: 0,
        };
        let thread = thread::spawn(move || {
            let _ = worker.serve();
        });
        Self {
            jobs,
            heard,
            thread,
        }
    }

    pub fn heard(&self) -> Heard {
        *self.heard.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The port failed (unplugged, replaced): its thread ended, and it is to be opened again.
    pub fn failed(&self) -> bool {
        self.thread.is_finished()
    }

    /// A command's value at `device`, or the answer to writing `value`: `None` when it does not
    /// answer.
    pub fn exchange(
        &self,
        command: &'static Command,
        device: u8,
        value: Option<u64>,
    ) -> io::Result<Option<u64>> {
        let closed = || io::Error::other("The serial port was closed.");
        let (reply, answer) = mpsc::channel();
        let job = Job {
            command,
            device,
            value,
            reply,
        };
        self.jobs.send(job).map_err(|_| closed())?;
        answer.recv().map_err(|_| closed())?
    }
}

/// The thread ends: the port failed, or the plugin let it go.
struct Stop;

/// Polls missed in a row.
#[derive(Default)]
struct Misses {
    shifter: u8,
    handbrake: u8,
    rim: u8,
}

/// Whether something that was there still is: answering, or late fewer than `MISSES` times.
fn kept(was: bool, answers: bool, misses: &mut u8) -> bool {
    if answers {
        *misses = 0;
        return true;
    }
    *misses = misses.saturating_add(1);
    was && *misses < MISSES
}

struct Worker {
    link: Box<dyn Link>,
    inbox: Receiver<Job>,
    shared: Arc<Mutex<Heard>>,
    /// What it heard, published after each poll.
    now: Heard,
    misses: Misses,
    /// The rim id to look at next while none answers: one per poll, so a request waits behind
    /// one probe at most.
    rim_next: usize,
}

impl Worker {
    fn serve(mut self) -> Result<(), Stop> {
        let mut next = Instant::now();
        loop {
            match self
                .inbox
                .recv_timeout(next.saturating_duration_since(Instant::now()))
            {
                Ok(job) => self.run(job)?,
                Err(RecvTimeoutError::Timeout) => {
                    self.poll()?;
                    next = Instant::now() + POLL;
                }
                // The device was detached.
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }

    /// Runs a request of the plugin, and those that follow it closely.
    fn run(&mut self, first: Job) -> Result<(), Stop> {
        let mut job = first;
        loop {
            let answer = self.send(job.command, job.device, job.value, ANSWER);
            let failed = answer.is_err();
            let _ = job.reply.send(answer);
            if failed {
                return Err(Stop);
            }
            match self.inbox.recv_timeout(GRACE) {
                Ok(next) => job = next,
                Err(RecvTimeoutError::Timeout) => return Ok(()),
                Err(RecvTimeoutError::Disconnected) => return Err(Stop),
            }
        }
    }

    /// A command read at `device`, or written when there is a value.
    fn send(
        &mut self,
        command: &Command,
        device: u8,
        value: Option<u64>,
        wait: Duration,
    ) -> io::Result<Option<u64>> {
        let (rw, frame) = match value {
            None => (command.read, protocol::read_frame(command, device)),
            Some(v) => (command.write, protocol::write_frame(command, device, v)),
        };
        let (Some((group, id)), Some(frame)) = (rw, frame) else {
            return Ok(None);
        };
        self.link.exchange(&frame, (group, device, id), wait)
    }

    /// Runs the plugin's requests waiting, then reads `name` at `device` (its target's when
    /// `None`).
    fn ask(&mut self, name: &str, device: Option<u8>, wait: Duration) -> Result<Option<u64>, Stop> {
        match self.inbox.try_recv() {
            Ok(job) => self.run(job)?,
            Err(TryRecvError::Disconnected) => return Err(Stop),
            Err(TryRecvError::Empty) => {}
        }
        let Some(command) = protocol::command(name) else {
            return Ok(None);
        };
        let device = device.unwrap_or(command.target.ids()[0]);
        self.send(command, device, None, wait).map_err(|_| Stop)
    }

    /// Asks the base what is plugged into it, as Boxflat does: a shifter answers at 26, a
    /// handbrake at 27; the rim is checked at its id, or looked for at the next id while none
    /// answers.
    fn poll(&mut self) -> Result<(), Stop> {
        let lever = self.ask("sequential-output-y", None, ANSWER)?;
        let still = kept(
            self.now.shifter.is_some(),
            lever.is_some(),
            &mut self.misses.shifter,
        );
        self.now.shifter = match (lever, self.now.shifter) {
            (Some(height), None) => Some(ShifterInfo {
                h_pattern: height > 100,
                paddles: self.ask("sequential-paddle-sync", None, ANSWER)? == Some(2),
                reversed: self.ask("sequential-direction", None, ANSWER)? == Some(1),
            }),
            (_, known) if still => known,
            _ => None,
        };
        let handbrake = self.ask("handbrake-direction", None, ANSWER)?.is_some();
        self.now.handbrake = kept(self.now.handbrake, handbrake, &mut self.misses.handbrake);
        self.now.rim = match self.now.rim {
            Some(id) => {
                let answers = self.ask(RIM_PROBE, Some(id), RIM_ANSWER)?.is_some();
                kept(true, answers, &mut self.misses.rim).then_some(id)
            }
            None => {
                let ids = Target::Wheel.ids();
                let id = ids[self.rim_next % ids.len()];
                self.rim_next += 1;
                let found = self.ask(RIM_PROBE, Some(id), RIM_ANSWER)?.is_some();
                if found {
                    self.now.rims += 1;
                    self.misses.rim = 0;
                }
                found.then_some(id)
            }
        };
        self.now.polled = true;
        *self.shared.lock().unwrap_or_else(|e| e.into_inner()) = self.now;
        Ok(())
    }
}
