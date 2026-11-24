//! Which display the output goes on.

/// A connected display, as the window system reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayInfo {
    pub index: usize,
    /// Its name as the system shows it ("3DTV", "HP 27f"); a second display of the same name
    /// gets " (2)" after it.
    pub name: String,
    /// The name the window system gave it before SoftPresenter asked macOS ("Monitor #0"),
    /// which screens set up then are saved under.
    pub old_name: String,
    /// The main display, with the menu bar, where the operator works. No screen fills it.
    pub main: bool,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl DisplayInfo {
    /// True when a screen saved with this display name means this display.
    pub fn is_named(&self, name: &str) -> bool {
        self.name == name || (!self.old_name.is_empty() && self.old_name == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Borderless fullscreen on the display with this index.
    Fullscreen(usize),
    /// An ordinary window, for a computer with one display.
    Windowed,
}

/// Picks where the output goes. `operator` are the displays the operator works on: the one
/// holding the home window and the one with the menu bar (they are usually the same).
///
/// A requested display is used if it exists. Otherwise the output takes the first display that
/// is none of the operator's, and falls back to a window when there is no other display. When
/// it isn't known which display is the operator's (`operator` is empty) and there are several,
/// it uses a window too: an output that guessed wrong would cover the operator's screen and
/// leave them nothing to work with.
pub fn choose_output(
    displays: &[DisplayInfo],
    operator: &[usize],
    requested: Option<usize>,
) -> Result<Placement, String> {
    if let Some(wanted) = requested {
        return if displays.iter().any(|d| d.index == wanted) {
            Ok(Placement::Fullscreen(wanted))
        } else {
            Err(format!("There is no display {wanted}; run with --list-displays."))
        };
    }
    if operator.is_empty() {
        return Ok(Placement::Windowed);
    }
    Ok(displays
        .iter()
        .map(|d| d.index)
        .filter(|index| !operator.contains(index) && !displays.iter().any(|d| d.index == *index && d.main))
        .min()
        .map_or(Placement::Windowed, Placement::Fullscreen))
}

/// A rectangle of pixels: where a window or a display is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    fn right(&self) -> i64 {
        self.x as i64 + self.w as i64
    }
    fn bottom(&self) -> i64 {
        self.y as i64 + self.h as i64
    }
    fn contains(&self, inner: &Rect) -> bool {
        inner.x >= self.x && inner.y >= self.y && inner.right() <= self.right() && inner.bottom() <= self.bottom()
    }
}

/// The room left round the operator's window on the screen it opens on.
pub const HOME_MARGIN: u32 = 24;

/// Where the home window opens, as its outer rectangle (frame included): no larger than the
/// work area less a margin all round, and in it. At `saved` (a window position kept from the
/// last run) only if the whole window is still on one of the `work_areas`, otherwise centred in
/// `first`, the work area of the display it opens on.
pub fn place_home_window(wanted: (u32, u32), saved: Option<(i32, i32)>, first: Rect, work_areas: &[Rect]) -> Rect {
    let w = wanted.0.min(first.w.saturating_sub(2 * HOME_MARGIN)).max(1);
    let h = wanted.1.min(first.h.saturating_sub(2 * HOME_MARGIN)).max(1);
    if let Some((x, y)) = saved {
        let at = Rect { x, y, w, h };
        if work_areas.iter().any(|area| area.contains(&at)) {
            return at;
        }
    }
    Rect {
        x: first.x + (first.w.saturating_sub(w) / 2) as i32,
        y: first.y + (first.h.saturating_sub(h) / 2) as i32,
        w,
        h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK_1080: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };

    #[test]
    fn a_big_window_shrinks_to_fit_the_work_area_and_is_centred() {
        // 1456x939 with its frame fits a 1080p screen less its taskbar: centred, not cascaded.
        let put = place_home_window((1456, 939), None, WORK_1080, &[WORK_1080]);
        assert_eq!(put, Rect { x: 232, y: 50, w: 1456, h: 939 });
        // A small laptop screen: smaller than asked, a margin all round.
        let laptop = Rect { x: 0, y: 0, w: 1366, h: 728 };
        let put = place_home_window((1456, 939), None, laptop, &[laptop]);
        assert_eq!((put.w, put.h), (1366 - 48, 728 - 48));
        assert_eq!((put.x, put.y), (24, 24));
    }

    #[test]
    fn the_work_area_may_start_away_from_the_corner() {
        // A taskbar on the left, and a second display to the left at a negative place.
        let work = Rect { x: -1920 + 60, y: 0, w: 1860, h: 1080 };
        let put = place_home_window((1000, 700), None, work, &[work]);
        assert_eq!((put.x, put.y), (-1860 + 430, 190));
    }

    #[test]
    fn a_saved_place_is_kept_only_while_all_of_the_window_is_on_a_display() {
        let right = Rect { x: 1920, y: 0, w: 1920, h: 1080 };
        let areas = [WORK_1080, right];
        assert_eq!(
            place_home_window((1000, 700), Some((100, 80)), WORK_1080, &areas),
            Rect { x: 100, y: 80, w: 1000, h: 700 }
        );
        // That display was unplugged: the window is back, centred.
        let put = place_home_window((1000, 700), Some((2100, 80)), WORK_1080, &[WORK_1080]);
        assert_eq!((put.x, put.y), (460, 170));
        // Half under the taskbar is not on the work area.
        let put = place_home_window((1000, 700), Some((100, 600)), WORK_1080, &[WORK_1080]);
        assert_eq!((put.x, put.y), (460, 170));
    }

    fn displays(n: usize) -> Vec<DisplayInfo> {
        (0..n)
            .map(|index| DisplayInfo { index, name: format!("d{index}"), w: 1920, h: 1080, ..Default::default() })
            .collect()
    }

    #[test]
    fn one_display_is_windowed() {
        assert_eq!(choose_output(&displays(1), &[0], None), Ok(Placement::Windowed));
    }

    #[test]
    fn two_displays_picks_the_other() {
        assert_eq!(choose_output(&displays(2), &[0], None), Ok(Placement::Fullscreen(1)));
        assert_eq!(choose_output(&displays(2), &[1], None), Ok(Placement::Fullscreen(0)));
    }

    #[test]
    fn three_displays_picks_lowest_other() {
        assert_eq!(choose_output(&displays(3), &[1], None), Ok(Placement::Fullscreen(0)));
        assert_eq!(choose_output(&displays(3), &[0], None), Ok(Placement::Fullscreen(1)));
    }

    #[test]
    fn request_is_honoured_even_if_home() {
        assert_eq!(choose_output(&displays(2), &[0], Some(0)), Ok(Placement::Fullscreen(0)));
    }

    #[test]
    fn bad_request_is_an_error() {
        let err = choose_output(&displays(2), &[0], Some(4)).unwrap_err();
        assert!(err.contains("no display 4"), "{err}");
    }

    #[test]
    fn never_the_display_with_the_menu_bar_nor_the_home_windows() {
        // The home window on display 1, the menu bar on display 0: the output takes display 2.
        assert_eq!(choose_output(&displays(3), &[1, 0], None), Ok(Placement::Fullscreen(2)));
        // With only those two, a window, never one of the operator's displays.
        assert_eq!(choose_output(&displays(2), &[1, 0], None), Ok(Placement::Windowed));
    }

    #[test]
    fn not_knowing_the_operators_display_means_a_window_not_a_guess() {
        assert_eq!(choose_output(&displays(2), &[], None), Ok(Placement::Windowed));
    }

    #[test]
    fn no_displays_is_windowed() {
        assert_eq!(choose_output(&[], &[0], None), Ok(Placement::Windowed));
    }
}
