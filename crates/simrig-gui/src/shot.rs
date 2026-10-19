//! `--screenshot`: the real app drawn by the software renderer into a PNG instead of a window.
//! Used to check screens without a display.

use std::collections::VecDeque;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use slint::Rgb8Pixel;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{EventLoopProxy, Platform, WindowAdapter};
use slint::{EventLoopError, PhysicalSize, PlatformError};

type Job = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Queue {
    jobs: Mutex<VecDeque<Job>>,
    quit: AtomicBool,
}

struct Proxy(Arc<Queue>);

impl EventLoopProxy for Proxy {
    fn quit_event_loop(&self) -> Result<(), EventLoopError> {
        self.0.quit.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn invoke_from_event_loop(&self, event: Job) -> Result<(), EventLoopError> {
        self.0
            .jobs
            .lock()
            .map_err(|_| EventLoopError::EventLoopTerminated)?
            .push_back(event);
        Ok(())
    }
}

struct Offscreen {
    window: Rc<MinimalSoftwareWindow>,
    queue: Arc<Queue>,
    start: Instant,
}

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> Duration {
        self.start.elapsed()
    }

    fn run_event_loop(&self) -> Result<(), PlatformError> {
        while !self.queue.quit.load(Ordering::SeqCst) {
            slint::platform::update_timers_and_animations();
            let jobs: Vec<Job> = match self.queue.jobs.lock() {
                Ok(mut jobs) => jobs.drain(..).collect(),
                Err(_) => break,
            };
            for job in jobs {
                job();
                if self.queue.quit.load(Ordering::SeqCst) {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(4));
        }
        Ok(())
    }

    fn new_event_loop_proxy(&self) -> Option<Box<dyn EventLoopProxy>> {
        Some(Box::new(Proxy(self.queue.clone())))
    }
}

/// Installs the offscreen platform; call before creating any window.
pub fn install(width: u32, height: u32) -> Result<Rc<MinimalSoftwareWindow>> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.set_size(PhysicalSize::new(width, height));
    slint::platform::set_platform(Box::new(Offscreen {
        window: window.clone(),
        queue: Arc::default(),
        start: Instant::now(),
    }))
    .map_err(|e| anyhow::anyhow!("cannot install the offscreen platform: {e}"))?;
    Ok(window)
}

/// Draws the window and writes it as a PNG.
pub fn capture(window: &MinimalSoftwareWindow, path: &Path) -> Result<()> {
    let size = window.size();
    let (width, height) = (size.width as usize, size.height as usize);
    let mut buffer = vec![Rgb8Pixel::new(0, 0, 0); width * height];
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, width);
    });
    let file = std::fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size.width, size.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    let bytes: Vec<u8> = buffer.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    writer.write_image_data(&bytes)?;
    Ok(())
}
