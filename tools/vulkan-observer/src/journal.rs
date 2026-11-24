use serde_json::Value;
use std::io::Write;

pub struct Journal<W: Write> {
    writer: W,
    remaining: usize,
    records: usize,
    limit: usize,
    footer_remaining: usize,
    pub dropped: u64,
    pub failed: bool,
}
impl<W: Write> Journal<W> {
    pub fn new(writer: W, bytes: usize, records: usize) -> Self {
        Self {
            writer,
            remaining: bytes,
            records: 0,
            limit: records,
            footer_remaining: 16384,
            dropped: 0,
            failed: false,
        }
    }
    pub fn write(&mut self, value: &Value) {
        let Ok(mut bytes) = serde_json::to_vec(value) else {
            self.failed = true;
            return;
        };
        bytes.push(b'\n');
        if self.failed || self.records >= self.limit || bytes.len() > self.remaining {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        self.remaining -= bytes.len();
        self.records += 1;
        if self.writer.write_all(&bytes).is_err() {
            self.failed = true;
        }
    }
    // The footer has a separate bounded reserve. It reports exhausted logging.
    pub fn footer(&mut self, value: &Value) {
        if self.failed {
            return;
        }
        let Ok(mut bytes) = serde_json::to_vec(value) else {
            self.failed = true;
            return;
        };
        bytes.push(b'\n');
        if bytes.len() > self.footer_remaining {
            // A terminal marker prevents an old lifetime footer looking final.
            let _ = self
                .writer
                .write_all(b"{\"event\":\"footer_budget_exhausted\",\"incomplete\":true}\n");
            let _ = self.writer.flush();
            self.failed = true;
            return;
        }
        self.footer_remaining -= bytes.len();
        if self.writer.write_all(&bytes).is_err() || self.writer.flush().is_err() {
            self.failed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exhaustion_still_has_an_explicit_footer() {
        let mut j = Journal::new(Vec::new(), 64, 1);
        j.write(&json!({"event":"one"}));
        j.write(&json!({"event":"two"}));
        assert_eq!(j.dropped, 1);
        j.footer(&json!({"dropped":j.dropped}));
        let text = String::from_utf8(j.writer).unwrap();
        assert!(text.contains("\"dropped\":1"));
        assert!(!text.contains("two"));
    }
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("injected failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn io_failure_is_retained() {
        let mut j = Journal::new(Broken, 128, 10);
        j.write(&json!({"event":"one"}));
        assert!(j.failed);
        j.write(&json!({"event":"two"}));
        assert_eq!(j.dropped, 1);
    }
    #[test]
    fn repeated_footers_have_a_total_bound_and_terminal_marker() {
        let mut j = Journal::new(Vec::new(), 64, 1);
        for _ in 0..2000 {
            j.footer(&json!({"event":"end"}));
        }
        assert!(j.failed);
        assert!(j.writer.len() <= 16448);
        let text = String::from_utf8(j.writer).unwrap();
        assert!(text
            .lines()
            .last()
            .unwrap()
            .contains("footer_budget_exhausted"));
    }
}
