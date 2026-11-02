//! Bounded display of Iris diagnostics; SDL never runs on a core thread.
use crate::settings::DebugOutput;
use iris::diagnostics::{DiagnosticSink, Stream};
use std::{
    collections::VecDeque,
    fmt::{self, Write},
    sync::Mutex,
};

pub const SCREEN_LINES: usize = 12;
const LINE_WIDTH: usize = 110;
const MESSAGE_LIMIT: usize = 1024;

struct State {
    output: DebugOutput,
    lines: VecDeque<String>,
    changed: bool,
}

pub struct DebugLog(Mutex<State>);

impl DebugLog {
    pub fn new(output: DebugOutput) -> Self {
        Self(Mutex::new(State {
            output,
            lines: VecDeque::new(),
            changed: true,
        }))
    }

    pub fn set_output(&self, output: DebugOutput) {
        let mut state = self.0.lock().unwrap();
        if state.output != output {
            state.output = output;
            state.lines.clear();
            state.changed = true;
        }
    }

    // Called once per event-loop iteration, even without a new guest frame.
    pub fn take_update(&self) -> Option<Vec<String>> {
        let mut state = self.0.lock().unwrap();
        if !std::mem::take(&mut state.changed) {
            return None;
        }
        Some(state.lines.iter().cloned().collect())
    }

    // Return console messages to be written after releasing the state lock.
    fn record(&self, message: fmt::Arguments<'_>) -> Option<String> {
        let mut state = self.0.lock().unwrap();
        match state.output {
            DebugOutput::Off => None,
            DebugOutput::Console => {
                drop(state);
                Some(message.to_string())
            }
            DebugOutput::Screen => {
                let mut text = ScreenText(String::new());
                let _ = text.write_fmt(message);
                for line in text.0.lines() {
                    // ScreenText contains only ASCII, so byte chunks are safe.
                    for chunk in line.as_bytes().chunks(LINE_WIDTH) {
                        if state.lines.len() == SCREEN_LINES {
                            state.lines.pop_front();
                        }
                        state
                            .lines
                            .push_back(String::from_utf8(chunk.to_vec()).unwrap());
                    }
                }
                state.changed = true;
                None
            }
        }
    }
}

impl DiagnosticSink for DebugLog {
    fn write(&self, stream: Stream, message: fmt::Arguments<'_>) {
        if let Some(message) = self.record(message) {
            iris::diagnostics::write_console(stream, format_args!("{message}"));
        }
    }
}

// Bound allocation while formatting, and sanitize control/non-ASCII characters
// for SDL's built-in bitmap font. Console output keeps the original message.
struct ScreenText(String);

impl Write for ScreenText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for c in text.chars() {
            if self.0.len() == MESSAGE_LIMIT {
                self.0.truncate(MESSAGE_LIMIT - 3);
                self.0.push_str("...");
                return Err(fmt::Error);
            }
            self.0
                .push(if c == '\n' || (c.is_ascii() && !c.is_control()) {
                    c
                } else {
                    '?'
                });
        }
        Ok(())
    }
}

pub use iris::diagnostics::stderr;
pub fn install(log: std::sync::Arc<DebugLog>) {
    iris::diagnostics::set_sink(Some(log));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destinations_switch_live_and_do_not_replay_disabled_messages() {
        let log = DebugLog::new(DebugOutput::Off);
        log.take_update();
        assert!(log.record(format_args!("hidden probe")).is_none());
        assert!(log.take_update().is_none());
        log.set_output(DebugOutput::Screen);
        assert!(log.record(format_args!("GIO probe {}", 7)).is_none());
        assert_eq!(log.take_update().unwrap(), ["GIO probe 7"]);
        assert!(log.take_update().is_none());
        log.set_output(DebugOutput::Console);
        assert_eq!(
            log.record(format_args!("console é")),
            Some("console é".into())
        );
        assert!(log.take_update().unwrap().is_empty());
        log.set_output(DebugOutput::Screen);
        assert!(log.take_update().unwrap().is_empty());
        log.record(format_args!("new screen line"));
        log.set_output(DebugOutput::Off);
        assert!(log.take_update().unwrap().is_empty());
    }

    #[test]
    fn screen_history_and_message_size_are_bounded_and_text_is_safe() {
        let log = DebugLog::new(DebugOutput::Screen);
        for i in 0..1000 {
            log.record(format_args!("line {i}"));
        }
        let lines = log.take_update().unwrap();
        assert_eq!(lines.len(), SCREEN_LINES);
        assert_eq!(lines.first().unwrap(), "line 988");
        assert_eq!(lines.last().unwrap(), "line 999");
        log.record(format_args!("é\0\t\n{}", "x".repeat(10_000)));
        let lines = log.take_update().unwrap();
        assert!(lines.len() <= SCREEN_LINES);
        assert!(
            lines
                .iter()
                .all(|line| line.len() <= LINE_WIDTH && line.is_ascii())
        );
        assert!(lines.iter().any(|line| line == "???"));
        assert!(lines.last().unwrap().ends_with("..."));
    }

    #[test]
    fn core_threads_can_publish_without_ui_or_framebuffer_work() {
        let log = DebugLog::new(DebugOutput::Screen);
        std::thread::scope(|scope| {
            for id in 0..4 {
                let log = &log;
                scope.spawn(move || {
                    for i in 0..100 {
                        log.write(Stream::Stderr, format_args!("thread {id} probe {i}"));
                    }
                });
            }
        });
        assert_eq!(log.take_update().unwrap().len(), SCREEN_LINES);
    }
}
