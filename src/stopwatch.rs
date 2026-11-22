use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Stopwatch {
    started_at: Option<Instant>,
    accumulated: Duration,
}

impl Stopwatch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_running(&self) -> bool {
        self.started_at.is_some()
    }

    pub fn elapsed(&self) -> Duration {
        self.accumulated
            + self
                .started_at
                .map_or(Duration::ZERO, |started_at| started_at.elapsed())
    }

    pub fn toggle(&mut self) {
        if let Some(started_at) = self.started_at.take() {
            self.accumulated += started_at.elapsed();
        } else {
            self.started_at = Some(Instant::now());
        }
    }

    pub fn reset(&mut self) {
        self.started_at = None;
        self.accumulated = Duration::ZERO;
    }

    pub fn formatted(&self) -> String {
        Self::format_duration(self.elapsed())
    }

    pub fn format_duration(duration: Duration) -> String {
        let total_millis = duration.as_millis();
        let hours = total_millis / 3_600_000;
        let minutes = total_millis / 60_000 % 60;
        let seconds = total_millis / 1_000 % 60;
        let centiseconds = total_millis / 10 % 100;
        format!("{hours:02}:{minutes:02}:{seconds:02}.{centiseconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::Stopwatch;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn formats_elapsed_time() {
        let duration = Duration::from_millis((3_600_000 * 2) + (60_000 * 3) + 4_567);
        assert_eq!(Stopwatch::format_duration(duration), "02:03:04.56");
    }

    #[test]
    fn start_pause_and_resume_preserve_elapsed_time() {
        let mut stopwatch = Stopwatch::new();

        stopwatch.toggle();
        thread::sleep(Duration::from_millis(20));
        stopwatch.toggle();
        let paused = stopwatch.elapsed();

        assert!(!stopwatch.is_running());
        assert!(paused >= Duration::from_millis(15));
        thread::sleep(Duration::from_millis(10));
        assert_eq!(stopwatch.elapsed(), paused);

        stopwatch.toggle();
        thread::sleep(Duration::from_millis(10));
        assert!(stopwatch.is_running());
        assert!(stopwatch.elapsed() >= paused + Duration::from_millis(5));
    }

    #[test]
    fn reset_clears_timer() {
        let mut stopwatch = Stopwatch::new();
        stopwatch.toggle();
        thread::sleep(Duration::from_millis(10));
        stopwatch.reset();

        assert!(!stopwatch.is_running());
        assert!(stopwatch.elapsed().is_zero());
    }

    #[test]
    fn format_rolls_minutes_and_truncates_to_centiseconds() {
        assert_eq!(
            Stopwatch::format_duration(Duration::from_millis(59_999)),
            "00:00:59.99"
        );
        assert_eq!(
            Stopwatch::format_duration(Duration::from_millis(60_009)),
            "00:01:00.00"
        );
    }
}
