//! Recognizes positions while the user moves the device: what changes from the rest position,
//! once it holds still. Bits that move by themselves at rest (noise, analog jitter) never count.

use std::time::{Duration, Instant};

use crate::config::{ByteMatch, HandbrakeInput, Signature};
use crate::decode::field;

/// How long a position must hold still to be taken.
pub const HOLD: Duration = Duration::from_millis(300);
/// How long the rest position is watched for noise.
pub const REST: Duration = Duration::from_millis(1000);

/// The device at rest: its report and the bits that move by themselves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rest {
    pub report: Vec<u8>,
    pub noise: Vec<u8>,
}

/// Watches the rest position for [`REST`].
#[derive(Debug, Default)]
pub struct RestWatch {
    since: Option<Instant>,
    rest: Rest,
}

impl RestWatch {
    /// Feeds the latest report; returns the rest position once watched long enough.
    pub fn feed(&mut self, report: &[u8], now: Instant) -> Option<Rest> {
        let since = *self.since.get_or_insert(now);
        if self.rest.report.is_empty() {
            self.rest.report = report.to_vec();
            self.rest.noise = vec![0; report.len()];
        }
        let len = report.len().max(self.rest.report.len());
        self.rest.noise.resize(len, 0);
        for i in 0..len {
            let old = self.rest.report.get(i).copied().unwrap_or(0);
            let new = report.get(i).copied().unwrap_or(0);
            self.rest.noise[i] |= old ^ new;
        }
        self.rest.report = report.to_vec();
        (now.duration_since(since) >= REST).then(|| self.rest.clone())
    }
}

/// What differs from the rest position, outside the noise.
pub fn signature(rest: &Rest, report: &[u8]) -> Option<Signature> {
    let bytes: Vec<ByteMatch> = report
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let base = rest.report.get(index).copied().unwrap_or(0);
            let noise = rest.noise.get(index).copied().unwrap_or(0);
            let mask = (value ^ base) & !noise;
            (mask != 0).then_some(ByteMatch {
                index,
                mask,
                value: value & mask,
            })
        })
        .collect();
    (!bytes.is_empty()).then_some(Signature { bytes })
}

/// Waits for a position held still for [`HOLD`] that differs from the rest position and from
/// the positions already taken.
#[derive(Debug, Default)]
pub struct PositionWatch {
    candidate: Option<(Signature, Instant)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Seen {
    /// Nothing yet (at rest, moving, or not held long enough).
    Waiting,
    /// A new position.
    Position(Signature),
    /// Held still on a position already taken, at this index.
    AlreadyTaken(usize),
}

impl PositionWatch {
    pub fn feed(&mut self, rest: &Rest, report: &[u8], taken: &[Signature], now: Instant) -> Seen {
        let Some(sig) = signature(rest, report) else {
            self.candidate = None;
            return Seen::Waiting;
        };
        match &self.candidate {
            Some((held, since)) if *held == sig => {
                if now.duration_since(*since) < HOLD {
                    return Seen::Waiting;
                }
                self.candidate = None;
                match taken.iter().position(|t| *t == sig) {
                    Some(i) => Seen::AlreadyTaken(i),
                    None => Seen::Position(sig),
                }
            }
            _ => {
                self.candidate = Some((sig, now));
                Seen::Waiting
            }
        }
    }
}

/// What a handbrake turned out to be, from its rest and fully pulled reports.
#[derive(Clone, Debug, PartialEq)]
pub struct HandbrakeFound {
    pub input: HandbrakeInput,
    pub min: i64,
    pub max: i64,
}

/// Chooses the handbrake's field: a 16-bit axis when two adjacent bytes moved, an 8-bit axis
/// when one byte moved a lot, a button when only a bit or two flipped.
pub fn handbrake(rest: &Rest, pulled: &[u8]) -> Option<HandbrakeFound> {
    let r = &rest.report;
    let changed: Vec<usize> = (0..pulled.len().min(r.len()))
        .filter(|&i| pulled[i] != r[i])
        .collect();
    if changed.is_empty() {
        return None;
    }
    // Two adjacent bytes: a little-endian 16-bit axis.
    if let Some(&i) = changed.iter().find(|&&i| changed.contains(&(i + 1))) {
        return Some(axis(r, pulled, i, 2));
    }
    // The byte that moved the most.
    let &i = changed
        .iter()
        .max_by_key(|&&i| (i64::from(pulled[i]) - i64::from(r[i])).abs())?;
    let delta = (i64::from(pulled[i]) - i64::from(r[i])).abs();
    let flipped = (pulled[i] ^ r[i]).count_ones();
    // A byte that barely moved, or only flipped a bit or two outside the noise, is a switch.
    let sig = signature(rest, pulled);
    if delta < 8
        || (flipped <= 2 && changed.len() == 1 && sig.as_ref().is_some_and(|s| s.bits() <= 2))
    {
        return sig.map(|pressed| HandbrakeFound {
            input: HandbrakeInput::Button { pressed },
            min: 0,
            max: 1,
        });
    }
    // A byte whose lower neighbour jitters at rest is the high part of a 16-bit axis.
    let noisy_low = i > 0 && rest.noise.get(i - 1).is_some_and(|n| *n != 0);
    Some(if noisy_low {
        axis(r, pulled, i - 1, 2)
    } else {
        axis(r, pulled, i, 1)
    })
}

fn axis(rest: &[u8], pulled: &[u8], index: usize, size: u8) -> HandbrakeFound {
    HandbrakeFound {
        input: HandbrakeInput::Axis { index, size },
        min: field(rest, index, size).unwrap_or(0),
        max: field(pulled, index, size).unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rest(report: &[u8], noise: &[u8]) -> Rest {
        Rest {
            report: report.to_vec(),
            noise: noise.to_vec(),
        }
    }

    #[test]
    fn rest_learns_noise() {
        let t = Instant::now();
        let mut w = RestWatch::default();
        assert_eq!(w.feed(&[1, 0x80, 0], t), None);
        assert_eq!(w.feed(&[1, 0x81, 0], t + Duration::from_millis(500)), None);
        let r = w.feed(&[1, 0x80, 0], t + REST).unwrap();
        assert_eq!(r.noise, vec![0, 1, 0]);
    }

    #[test]
    fn positions_hold_and_ignore_noise() {
        let r = rest(&[0, 0x80, 0], &[0, 0x01, 0]);
        let t = Instant::now();
        let mut w = PositionWatch::default();
        // Only noise: still at rest.
        assert_eq!(w.feed(&r, &[0, 0x81, 0], &[], t), Seen::Waiting);
        // 3rd gear: byte 2 bit 2, held.
        assert_eq!(w.feed(&r, &[0, 0x81, 0x04], &[], t), Seen::Waiting);
        let seen = w.feed(&r, &[0, 0x80, 0x04], &[], t + HOLD);
        let Seen::Position(sig) = seen else {
            panic!("{seen:?}")
        };
        assert_eq!(sig.describe(), "byte 2 bit 2");
        // The same position again is reported as taken.
        w.feed(
            &r,
            &[0, 0x80, 0x04],
            std::slice::from_ref(&sig),
            t + HOLD * 2,
        );
        assert_eq!(
            w.feed(&r, &[0, 0x80, 0x04], &[sig], t + HOLD * 4),
            Seen::AlreadyTaken(0)
        );
    }

    #[test]
    fn handbrake_kinds() {
        // 16-bit axis at bytes 1..3, little endian, 0x0010 at rest, 0x0ff0 pulled.
        let r = rest(&[7, 0x10, 0x00, 0], &[0, 0x03, 0, 0]);
        let found = handbrake(&r, &[7, 0xf0, 0x0f, 0]).unwrap();
        assert_eq!(found.input, HandbrakeInput::Axis { index: 1, size: 2 });
        assert_eq!((found.min, found.max), (0x10, 0xff0));
        // 8-bit axis.
        let r = rest(&[0, 0x05], &[0, 0]);
        let found = handbrake(&r, &[0, 0xf0]).unwrap();
        assert_eq!(found.input, HandbrakeInput::Axis { index: 1, size: 1 });
        // A button.
        let r = rest(&[0, 0], &[0, 0]);
        let found = handbrake(&r, &[0, 0x10]).unwrap();
        assert!(matches!(found.input, HandbrakeInput::Button { .. }));
        assert!(handbrake(&r, &[0, 0]).is_none());
    }
}
