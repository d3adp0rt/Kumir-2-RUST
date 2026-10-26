//! Background execution of a Kumir program for the IDE.
//!
//! The interpreter runs in a worker thread. Output and the current source line
//! are streamed to the UI through a channel; console input is sent back.
//! Pausing, stepping and breakpoints are implemented in the `Host` callbacks.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};

use kumir_core::actors::ActorState;
use std::thread::JoinHandle;
use std::time::Duration;

use kumir_core::ast::Program;
use kumir_core::interpreter::{Host, RunOptions};

use kumir_core::STOP_MESSAGE;

#[derive(Clone, Debug)]
pub enum RunEvent {
    Output(String),
    Line(u32),
    InputRequested,
    Vars(Vec<(String, String, String)>),
    Finished(Result<(), String>),
}

#[derive(Default)]
struct RunFlags {
    paused: bool,
    step_once: bool,
}

pub struct RunShared {
    pub stop: AtomicBool,
    pub line: AtomicU32,
    pub awaiting_input: AtomicBool,
    breakpoints: Mutex<BTreeSet<u32>>,
    flags: Mutex<RunFlags>,
    cv: Condvar,
}

impl RunShared {
    pub fn new(breakpoints: BTreeSet<u32>) -> Self {
        Self {
            stop: AtomicBool::new(false),
            line: AtomicU32::new(0),
            awaiting_input: AtomicBool::new(false),
            breakpoints: Mutex::new(breakpoints),
            flags: Mutex::new(RunFlags::default()),
            cv: Condvar::new(),
        }
    }

    pub fn set_breakpoints(&self, breakpoints: BTreeSet<u32>) {
        *self.breakpoints.lock().unwrap() = breakpoints;
    }

    pub fn request_pause(&self) {
        let mut f = self.flags.lock().unwrap();
        f.paused = true;
        self.cv.notify_all();
    }

    pub fn resume(&self) {
        let mut f = self.flags.lock().unwrap();
        f.paused = false;
        f.step_once = false;
        self.cv.notify_all();
    }

    pub fn step(&self) {
        let mut f = self.flags.lock().unwrap();
        f.paused = true;
        f.step_once = true;
        self.cv.notify_all();
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.cv.notify_all();
    }

    pub fn is_paused(&self) -> bool {
        self.flags.lock().unwrap().paused
    }

    fn wait_if_paused(&self, force_pause: bool) {
        let mut f = self.flags.lock().unwrap();
        if force_pause {
            f.paused = true;
        }
        loop {
            if self.stop.load(Ordering::SeqCst) {
                return;
            }
            if !f.paused {
                return;
            }
            if f.step_once {
                f.step_once = false;
                // Stay paused: the next statement will wait again.
                return;
            }
            let (guard, _) = self.cv.wait_timeout(f, Duration::from_millis(100)).unwrap();
            f = guard;
        }
    }
}

struct ThreadHost {
    shared: Arc<RunShared>,
    out: Sender<RunEvent>,
    input: Receiver<String>,
}

impl Host for ThreadHost {
    fn write(&mut self, text: &str) {
        let _ = self.out.send(RunEvent::Output(text.to_string()));
    }

    fn read_line(&mut self) -> Result<String, String> {
        self.shared.awaiting_input.store(true, Ordering::SeqCst);
        let _ = self.out.send(RunEvent::InputRequested);
        loop {
            if self.shared.stop.load(Ordering::SeqCst) {
                self.shared.awaiting_input.store(false, Ordering::SeqCst);
                return Err(STOP_MESSAGE.to_string());
            }
            match self.input.recv_timeout(Duration::from_millis(100)) {
                Ok(line) => {
                    self.shared.awaiting_input.store(false, Ordering::SeqCst);
                    return Ok(line);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.shared.awaiting_input.store(false, Ordering::SeqCst);
                    return Err(STOP_MESSAGE.to_string());
                }
            }
        }
    }

    fn on_line(&mut self, line: u32) {
        self.shared.line.store(line, Ordering::SeqCst);
        let _ = self.out.send(RunEvent::Line(line));
        let hit = {
            let bp = self.shared.breakpoints.lock().unwrap();
            bp.contains(&line)
        };
        self.shared.wait_if_paused(hit);
    }

    fn on_pause(&mut self) {
        let _ = self.out.send(RunEvent::Output("\n[пауза]\n".to_string()));
        self.shared.wait_if_paused(true);
    }

    fn wants_vars(&self) -> bool {
        self.shared.is_paused()
    }

    fn on_vars(&mut self, vars: Vec<(String, String, String)>) {
        let _ = self.out.send(RunEvent::Vars(vars));
    }

    fn should_stop(&self) -> bool {
        self.shared.stop.load(Ordering::SeqCst)
    }
}

pub struct RunHandle {
    pub events: Receiver<RunEvent>,
    pub input: Sender<String>,
    pub shared: Arc<RunShared>,
    join: Option<JoinHandle<()>>,
}

impl RunHandle {
    pub fn stop(&self) {
        self.shared.stop();
        // Unblock a pending read_line.
        let _ = self.input.send(String::new());
    }

    pub fn send_input(&self, line: String) {
        let _ = self.input.send(line);
    }

    pub fn finish(&mut self) {
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for RunHandle {
    fn drop(&mut self) {
        self.stop();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

pub fn start(
    program: Program,
    source_dir: PathBuf,
    breakpoints: BTreeSet<u32>,
    actor: Arc<Mutex<ActorState>>,
    evaluate: bool,
    actor_delay: Duration,
) -> RunHandle {
    let (out_tx, out_rx) = mpsc::channel();
    let (in_tx, in_rx) = mpsc::channel();
    let shared = Arc::new(RunShared::new(breakpoints));
    let thread_shared = shared.clone();
    let thread_out = out_tx.clone();
    let join = std::thread::spawn(move || {
        let mut host = ThreadHost {
            shared: thread_shared,
            out: thread_out,
            input: in_rx,
        };
        let result = kumir_core::run(
            &program,
            &mut host,
            RunOptions {
                source_dir: source_dir.clone(),
                actor: Some(actor),
                evaluate,
                actor_delay,
            },
        );
        let msg = result.map(|_| ()).map_err(|e| e.to_string());
        let _ = out_tx.send(RunEvent::Line(0));
        let _ = out_tx.send(RunEvent::Finished(msg));
    });
    RunHandle {
        events: out_rx,
        input: in_tx,
        shared,
        join: Some(join),
    }
}
