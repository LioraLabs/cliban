//! Raw terminal bytes → crossterm `KeyEvent`s, as they'd arrive on an SSH
//! channel: printable UTF-8, control chars, and CSI/SS3 escape sequences.
//! `ByteSession` wraps the parser as a `Session` with a resize injection
//! point — the headless harness today, the SSH channel's shape later.

use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::session::{Session, SessionEvent};

/// Incremental parser: feed byte chunks, get key events. Incomplete escape
/// sequences and split UTF-8 chars are buffered across feeds.
#[derive(Default)]
pub struct Parser {
    pending: Vec<u8>,
}

impl Parser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Consume `bytes`, returning all completed events. Incomplete trailing
    /// sequences stay buffered for the next feed.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<KeyEvent> {
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            match parse_one(&self.pending) {
                Parsed::Event(ev, used) => {
                    self.pending.drain(..used);
                    out.push(ev);
                }
                Parsed::Skip(used) => {
                    self.pending.drain(..used);
                }
                Parsed::Incomplete => break,
            }
        }
        out
    }

    /// Flush a buffered lone ESC as an Esc key press. Call when no
    /// continuation bytes are coming (e.g. on read timeout).
    pub fn flush(&mut self) -> Option<KeyEvent> {
        if self.pending == [0x1b] {
            self.pending.clear();
            return Some(key(KeyCode::Esc));
        }
        None
    }
}

/// Headless session: bytes in via [`ByteSession::feed_bytes`], resizes
/// injected via [`ByteSession::inject_resize`], events out through
/// [`Session::next_event`]. Never blocks — an empty queue is a `Tick`, like
/// a poll timeout. This is the harness for tests today and the shape the SSH
/// channel plugs into later.
#[derive(Default)]
pub struct ByteSession {
    parser: Parser,
    queue: VecDeque<SessionEvent>,
}

impl ByteSession {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw terminal bytes (as read off an SSH channel or a test script).
    pub fn feed_bytes(&mut self, bytes: &[u8]) {
        for k in self.parser.feed(bytes) {
            self.queue.push_back(SessionEvent::Key(k));
        }
    }

    /// Inject a terminal resize (SSH window-change arrives out-of-band from
    /// the byte stream, so this is a separate entry point).
    pub fn inject_resize(&mut self, cols: u16, rows: u16) {
        self.queue.push_back(SessionEvent::Resize(cols, rows));
    }

    /// Inject a coarse data-changed notification (as the SSH host's dirty
    /// probe would produce when another session writes to the same board).
    pub fn inject_refresh(&mut self) {
        self.queue.push_back(SessionEvent::Refresh);
    }
}

impl Session for ByteSession {
    fn next_event(&mut self, _timeout: Duration) -> io::Result<SessionEvent> {
        if let Some(ev) = self.queue.pop_front() {
            return Ok(ev);
        }
        // Idle: a buffered lone ESC is a real Esc key press.
        if let Some(k) = self.parser.flush() {
            return Ok(SessionEvent::Key(k));
        }
        Ok(SessionEvent::Tick)
    }
}

/// Cap on a buffered-but-unterminated CSI sequence before it's dropped. Real
/// terminals abort runaway escape sequences too; this bounds `Parser.pending`
/// against an SSH client that never sends a final byte.
const MAX_CSI_LEN: usize = 32;

enum Parsed {
    /// A complete event, consuming `usize` bytes.
    Event(KeyEvent, usize),
    /// Unrecognized-but-complete input, consuming `usize` bytes.
    Skip(usize),
    /// Need more bytes.
    Incomplete,
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn parse_one(buf: &[u8]) -> Parsed {
    let Some(&b0) = buf.first() else {
        return Parsed::Incomplete;
    };
    match b0 {
        0x1b => parse_escape(buf),
        b'\r' | b'\n' => Parsed::Event(key(KeyCode::Enter), 1),
        b'\t' => Parsed::Event(key(KeyCode::Tab), 1),
        0x7f | 0x08 => Parsed::Event(key(KeyCode::Backspace), 1),
        // remaining C0 bytes 0x01-0x1a are Ctrl+letter (Ctrl+A = 0x01 ...)
        0x01..=0x1a => Parsed::Event(
            KeyEvent::new(KeyCode::Char((b0 + 0x60) as char), KeyModifiers::CONTROL),
            1,
        ),
        0x00 | 0x1c..=0x1f => Parsed::Skip(1), // other C0 controls: ignore
        _ => parse_utf8(buf),
    }
}

fn parse_escape(buf: &[u8]) -> Parsed {
    match buf.get(1) {
        // Lone ESC so far: wait for continuation bytes (see `Parser::flush`).
        None => Parsed::Incomplete,
        Some(b'[') => parse_csi(buf),
        Some(b'O') => match buf.get(2) {
            None => Parsed::Incomplete,
            Some(b'A') => Parsed::Event(key(KeyCode::Up), 3),
            Some(b'B') => Parsed::Event(key(KeyCode::Down), 3),
            Some(b'C') => Parsed::Event(key(KeyCode::Right), 3),
            Some(b'D') => Parsed::Event(key(KeyCode::Left), 3),
            Some(b'H') => Parsed::Event(key(KeyCode::Home), 3),
            Some(b'F') => Parsed::Event(key(KeyCode::End), 3),
            Some(_) => Parsed::Skip(3),
        },
        // Same-chunk ESC + printable ASCII = Alt+char (crossterm-compatible).
        Some(&b) if (0x20..0x7f).contains(&b) => Parsed::Event(
            KeyEvent::new(KeyCode::Char(b as char), KeyModifiers::ALT),
            2,
        ),
        // ESC + anything else (another ESC, control byte): emit Esc, re-parse rest.
        Some(_) => Parsed::Event(key(KeyCode::Esc), 1),
    }
}

fn parse_csi(buf: &[u8]) -> Parsed {
    // buf starts with ESC '['. The final byte of a CSI sequence is 0x40-0x7e.
    for (i, &b) in buf.iter().enumerate().skip(2) {
        if (0x40..=0x7e).contains(&b) {
            let used = i + 1;
            let ev = match b {
                b'A' => Some(key(KeyCode::Up)),
                b'B' => Some(key(KeyCode::Down)),
                b'C' => Some(key(KeyCode::Right)),
                b'D' => Some(key(KeyCode::Left)),
                b'H' => Some(key(KeyCode::Home)),
                b'F' => Some(key(KeyCode::End)),
                b'Z' => Some(key(KeyCode::BackTab)),
                b'~' => match &buf[2..i] {
                    b"1" | b"7" => Some(key(KeyCode::Home)),
                    b"3" => Some(key(KeyCode::Delete)),
                    b"4" | b"8" => Some(key(KeyCode::End)),
                    b"5" => Some(key(KeyCode::PageUp)),
                    b"6" => Some(key(KeyCode::PageDown)),
                    _ => None,
                },
                _ => None,
            };
            return match ev {
                Some(e) => Parsed::Event(e, used),
                None => Parsed::Skip(used),
            };
        }
    }
    // Runaway sequence with no final byte: drop it rather than buffering
    // unbounded untrusted input (real terminals abort long sequences too).
    if buf.len() > MAX_CSI_LEN {
        return Parsed::Skip(buf.len());
    }
    Parsed::Incomplete
}

fn parse_utf8(buf: &[u8]) -> Parsed {
    let len = match buf[0] {
        b if b < 0x80 => 1,
        b if (0xc0..0xe0).contains(&b) => 2,
        b if (0xe0..0xf0).contains(&b) => 3,
        b if b >= 0xf0 => 4,
        _ => return Parsed::Skip(1), // stray continuation byte
    };
    if buf.len() < len {
        return Parsed::Incomplete;
    }
    match std::str::from_utf8(&buf[..len]) {
        Ok(s) => {
            let c = s.chars().next().unwrap();
            let mods = if c.is_uppercase() {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            };
            Parsed::Event(KeyEvent::new(KeyCode::Char(c), mods), len)
        }
        Err(_) => Parsed::Skip(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_one(bytes: &[u8]) -> Vec<KeyEvent> {
        Parser::new().feed(bytes)
    }

    #[test]
    fn printable_ascii_maps_to_char_keys() {
        assert_eq!(feed_one(b"q"), vec![key(KeyCode::Char('q'))]);
        assert_eq!(feed_one(b"/"), vec![key(KeyCode::Char('/'))]);
        assert_eq!(feed_one(b" "), vec![key(KeyCode::Char(' '))]);
    }

    #[test]
    fn uppercase_gets_shift_modifier() {
        assert_eq!(
            feed_one(b"H"),
            vec![KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT)]
        );
    }

    #[test]
    fn control_bytes_map_to_named_keys() {
        assert_eq!(feed_one(b"\r"), vec![key(KeyCode::Enter)]);
        assert_eq!(feed_one(b"\n"), vec![key(KeyCode::Enter)]);
        assert_eq!(feed_one(b"\t"), vec![key(KeyCode::Tab)]);
        assert_eq!(feed_one(&[0x7f]), vec![key(KeyCode::Backspace)]);
        assert_eq!(feed_one(&[0x08]), vec![key(KeyCode::Backspace)]);
    }

    #[test]
    fn ctrl_letter_bytes_carry_control_modifier() {
        assert_eq!(
            feed_one(&[0x03]),
            vec![KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)]
        );
    }

    #[test]
    fn utf8_multibyte_char_parses_and_survives_split_feeds() {
        assert_eq!(feed_one("é".as_bytes()), vec![key(KeyCode::Char('é'))]);
        let mut p = Parser::new();
        let bytes = "é".as_bytes();
        assert!(p.feed(&bytes[..1]).is_empty());
        assert_eq!(p.feed(&bytes[1..]), vec![key(KeyCode::Char('é'))]);
    }

    #[test]
    fn multiple_keys_in_one_chunk() {
        assert_eq!(
            feed_one(b" d"),
            vec![key(KeyCode::Char(' ')), key(KeyCode::Char('d'))]
        );
    }

    #[test]
    fn csi_arrows_backtab_home_end() {
        assert_eq!(feed_one(b"\x1b[A"), vec![key(KeyCode::Up)]);
        assert_eq!(feed_one(b"\x1b[B"), vec![key(KeyCode::Down)]);
        assert_eq!(feed_one(b"\x1b[C"), vec![key(KeyCode::Right)]);
        assert_eq!(feed_one(b"\x1b[D"), vec![key(KeyCode::Left)]);
        assert_eq!(feed_one(b"\x1b[Z"), vec![key(KeyCode::BackTab)]);
        assert_eq!(feed_one(b"\x1b[H"), vec![key(KeyCode::Home)]);
        assert_eq!(feed_one(b"\x1b[F"), vec![key(KeyCode::End)]);
    }

    #[test]
    fn ss3_arrows_home_end() {
        assert_eq!(feed_one(b"\x1bOA"), vec![key(KeyCode::Up)]);
        assert_eq!(feed_one(b"\x1bOB"), vec![key(KeyCode::Down)]);
        assert_eq!(feed_one(b"\x1bOC"), vec![key(KeyCode::Right)]);
        assert_eq!(feed_one(b"\x1bOD"), vec![key(KeyCode::Left)]);
        assert_eq!(feed_one(b"\x1bOH"), vec![key(KeyCode::Home)]);
        assert_eq!(feed_one(b"\x1bOF"), vec![key(KeyCode::End)]);
    }

    #[test]
    fn csi_tilde_nav_keys() {
        assert_eq!(feed_one(b"\x1b[1~"), vec![key(KeyCode::Home)]);
        assert_eq!(feed_one(b"\x1b[3~"), vec![key(KeyCode::Delete)]);
        assert_eq!(feed_one(b"\x1b[4~"), vec![key(KeyCode::End)]);
        assert_eq!(feed_one(b"\x1b[5~"), vec![key(KeyCode::PageUp)]);
        assert_eq!(feed_one(b"\x1b[6~"), vec![key(KeyCode::PageDown)]);
    }

    #[test]
    fn unknown_csi_sequences_are_swallowed() {
        assert!(feed_one(b"\x1b[15~").is_empty()); // F5
        assert_eq!(feed_one(b"\x1b[15~j"), vec![key(KeyCode::Char('j'))]);
    }

    #[test]
    fn runaway_csi_sequence_is_capped_and_parser_recovers() {
        let mut p = Parser::new();
        let mut runaway = b"\x1b[".to_vec();
        runaway.extend(std::iter::repeat_n(b'9', 100));
        assert!(p.feed(&runaway).is_empty());
        // The parser must have dropped the buffered garbage rather than
        // waiting forever; a subsequent normal byte parses cleanly.
        assert_eq!(p.feed(b"j"), vec![key(KeyCode::Char('j'))]);
    }

    #[test]
    fn split_csi_sequence_across_feeds() {
        let mut p = Parser::new();
        assert!(p.feed(b"\x1b[").is_empty());
        assert_eq!(p.feed(b"C"), vec![key(KeyCode::Right)]);
    }

    #[test]
    fn lone_esc_is_pending_until_flush() {
        let mut p = Parser::new();
        assert!(p.feed(b"\x1b").is_empty());
        assert_eq!(p.flush(), Some(key(KeyCode::Esc)));
        assert_eq!(p.flush(), None);
    }

    #[test]
    fn esc_plus_printable_in_one_chunk_is_alt_char() {
        // Same-chunk ESC+char is Alt+char (matches crossterm's local parsing);
        // a human pressing Esc then a key arrives as separate reads and goes
        // through the lone-ESC flush path instead.
        assert_eq!(
            feed_one(b"\x1bq"),
            vec![KeyEvent::new(KeyCode::Char('q'), KeyModifiers::ALT)]
        );
    }

    #[test]
    fn byte_session_queues_keys_and_resizes_then_ticks() {
        let mut s = ByteSession::new();
        s.feed_bytes(b"j");
        s.inject_resize(80, 24);
        assert_eq!(
            s.next_event(Duration::ZERO).unwrap(),
            SessionEvent::Key(key(KeyCode::Char('j')))
        );
        assert_eq!(
            s.next_event(Duration::ZERO).unwrap(),
            SessionEvent::Resize(80, 24)
        );
        assert_eq!(s.next_event(Duration::ZERO).unwrap(), SessionEvent::Tick);
    }

    #[test]
    fn byte_session_flushes_pending_esc_when_idle() {
        let mut s = ByteSession::new();
        s.feed_bytes(b"\x1b");
        assert_eq!(
            s.next_event(Duration::ZERO).unwrap(),
            SessionEvent::Key(key(KeyCode::Esc))
        );
        assert_eq!(s.next_event(Duration::ZERO).unwrap(), SessionEvent::Tick);
    }
}
