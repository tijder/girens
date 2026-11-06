//! Subtitles that are pictures (Blu-ray, DVD), for the player to draw.

/// One picture: on screen from `start` to `end` (milliseconds into the
/// file), at a place on the canvas, cut out of one of the sheets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cue {
    pub start: u64,
    pub end: u64,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Which of [`PictureSubtitles::sheets`] holds it, and where.
    pub sheet: usize,
    pub sheet_x: f32,
    pub sheet_y: f32,
}

/// A disc shows at most a few pictures at once, so looking this far back
/// from the last that has started finds all that are on screen.
const LOOK_BACK: usize = 4;

/// All pictures of one subtitle stream.
#[derive(Debug, Clone, PartialEq)]
pub struct PictureSubtitles {
    /// The size of the frame the positions of the cues refer to.
    pub width: f32,
    pub height: f32,
    /// Where to get the images the cues are cut out of.
    pub sheets: Vec<String>,
    cues: Vec<Cue>,
}

impl PictureSubtitles {
    pub fn new(width: f32, height: f32, sheets: Vec<String>, mut cues: Vec<Cue>) -> Self {
        cues.sort_by_key(|cue| cue.start);
        Self {
            width,
            height,
            sheets,
            cues,
        }
    }

    /// The pictures on screen at `position`, in milliseconds.
    pub fn active_at(&self, position: u64) -> Vec<Cue> {
        let started = self.cues.partition_point(|cue| cue.start <= position);
        self.cues[started.saturating_sub(LOOK_BACK)..started]
            .iter()
            .copied()
            .filter(|cue| cue.end > position)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(start: u64, end: u64) -> Cue {
        Cue {
            start,
            end,
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            sheet: 0,
            sheet_x: 0.0,
            sheet_y: 0.0,
        }
    }

    #[test]
    fn the_pictures_on_screen_are_found_by_time() {
        // Given out of order, and with two at once around 5 seconds.
        let subtitles = PictureSubtitles::new(
            720.0,
            576.0,
            vec![],
            vec![cue(9_000, 12_000), cue(1_000, 3_000), cue(5_000, 7_000), cue(5_500, 6_000)],
        );
        let starts = |position| -> Vec<u64> {
            subtitles
                .active_at(position)
                .iter()
                .map(|cue| cue.start)
                .collect()
        };
        assert!(starts(500).is_empty());
        assert_eq!(starts(1_000), [1_000]);
        assert!(starts(3_000).is_empty());
        assert_eq!(starts(5_700), [5_000, 5_500]);
        assert_eq!(starts(6_500), [5_000]);
        assert_eq!(starts(11_999), [9_000]);
        assert!(starts(60_000).is_empty());
    }
}
