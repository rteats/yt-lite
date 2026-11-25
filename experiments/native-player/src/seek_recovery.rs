//! Conservative loss detector for the exact-seek recovery proof.
use std::time::Duration;

#[derive(Clone, Copy)]
pub struct SeekEvidence {
    pub restarted: bool,
    pub position: Option<f64>,
    pub audio: Option<f64>,
    pub seeking: Option<bool>,
    pub buffering: Option<bool>,
    pub paused: Option<bool>,
    pub eof: Option<bool>,
}

#[derive(Default)]
pub struct SeekLossDetector {
    first: Option<(Duration, f64)>,
}

impl SeekLossDetector {
    /// Requires a continuous window of advancing video-only playback;
    /// caller must additionally confirm audio absence via a fresh reply.
    pub fn update(&mut self, elapsed: Duration, evidence: SeekEvidence) -> bool {
        let eligible = evidence.restarted && evidence.audio.is_none()
            && evidence.seeking == Some(false) && evidence.buffering == Some(false)
            && evidence.paused == Some(false) && evidence.eof == Some(false);
        let Some(position) = evidence.position.filter(|p| p.is_finite() && *p >= 0.0 && eligible) else {
            self.first = None;
            return false;
        };
        let (began, initial) = *self.first.get_or_insert((elapsed, position));
        if elapsed < began || position < initial {
            self.first = Some((elapsed, position));
            return false;
        }
        elapsed.saturating_sub(began) >= Duration::from_millis(750)
            && position - initial >= 0.25
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn e(position: f64) -> SeekEvidence {
        SeekEvidence { restarted: true, position: Some(position), audio: None, seeking: Some(false),
            buffering: Some(false), paused: Some(false), eof: Some(false) }
    }
    fn ms(value: u64) -> Duration { Duration::from_millis(value) }

    #[test]
    fn sustained_loss_qualifies_but_not_brief_reset() {
        let mut d = SeekLossDetector::default();
        assert!(!d.update(ms(0), e(52.25)));
        assert!(!d.update(ms(60), e(52.31)));
        assert!(!d.update(ms(749), e(52.99)));
        assert!(d.update(ms(750), e(53.0)));
    }

    #[test]
    fn legitimate_wait_or_restored_audio_resets_window() {
        for state in [
            SeekEvidence { restarted: false, ..e(52.5) },
            SeekEvidence { seeking: Some(true), ..e(52.5) },
            SeekEvidence { buffering: Some(true), ..e(52.5) },
            SeekEvidence { paused: Some(true), ..e(52.5) },
            SeekEvidence { eof: Some(true), ..e(52.5) },
            SeekEvidence { seeking: None, ..e(52.5) },
            SeekEvidence { audio: Some(52.5), ..e(52.5) },
        ] {
            let mut d = SeekLossDetector::default();
            assert!(!d.update(ms(0), e(52.25)));
            assert!(!d.update(ms(5000), state));
            assert!(!d.update(ms(5050), e(52.75)));
            assert!(d.update(ms(5800), e(53.5)));
        }
    }

    #[test]
    fn stationary_or_backward_video_does_not_trigger() {
        let mut d = SeekLossDetector::default();
        assert!(!d.update(ms(0), e(52.25)));
        assert!(!d.update(ms(1000), e(52.25)));
        assert!(!d.update(ms(2000), e(23.0)));
        assert!(!d.update(ms(2050), e(23.05)));
    }
}
