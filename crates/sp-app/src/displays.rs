//! Which display the output goes on.

/// A connected display, as the window system reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayInfo {
    pub index: usize,
    /// Its name as the system shows it ("3DTV", "HP 27f"; on Windows the name the monitor gives
    /// itself, "Mi TV"), else `old_name`; a second display of the same name gets " (2)" after
    /// it.
    pub name: String,
    /// The window system's own name for it ("Monitor #0" on a Mac, `\\.\DISPLAY2` on Windows),
    /// which screens set up before SoftPresenter asked the system are saved under.
    pub old_name: String,
    /// The main display, with the menu bar, where the operator works. No screen fills it. On
    /// Wayland no display is the main one: the window system doesn't say.
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

/// The name a display is listed under: `own`, the name the system gives it, where it gives one
/// that isn't blank, else `old_name`, the window system's; with " (2)", " (3)"… after it when a
/// display already in `list` has that name.
pub fn listed_name(own: Option<&str>, old_name: &str, list: &[DisplayInfo]) -> String {
    let name = own.map(str::trim).filter(|own| !own.is_empty()).unwrap_or(old_name);
    let alike = list.iter().filter(|d| d.name == name || d.name.starts_with(&format!("{name} ("))).count();
    if alike > 0 { format!("{name} ({})", alike + 1) } else { name.to_owned() }
}

/// The system's own name for one display, from `names`: each display the system names, by the
/// key both it and the window system know the display by (on a Mac its display id, on Windows
/// its device name, `\\.\DISPLAY2`), with that name. A display shown on two monitors at once,
/// which Windows lists once for each, takes the first one's name.
pub fn own_name<'a, K: PartialEq>(names: &'a [(K, String)], key: &K) -> Option<&'a str> {
    names.iter().find(|(have, _)| have == key).map(|(_, name)| name.as_str())
}

/// The monitors' names as just asked, or, when the answer was empty, the last names that came
/// (`last` keeps them). Windows can refuse the question for a moment (the computer locked, a
/// permission prompt up), and a display listed by its device name for that moment would look
/// unplugged to a screen saved under the monitor's name, which would vanish mid-service.
// Only Windows is asked this way; a Mac and Linux read their names in ways that never fail
// like this.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn names_or_last(fresh: Vec<(String, String)>, last: &mut Vec<(String, String)>) -> Vec<(String, String)> {
    if fresh.is_empty() {
        return last.clone();
    }
    last.clone_from(&fresh);
    fresh
}

/// A monitor's own name from its EDID (the name it gives itself), or `None` when it gives none.
/// A name that is only a model number ("28E850") gets its maker's three-letter code in front
/// ("AOC 28E850"); a name with a maker in it ("HP 27f", "LG ULTRAGEAR") is kept as it is.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn edid_name(edid: &[u8]) -> Option<String> {
    if edid.len() < 128 || edid[..8] != [0, 255, 255, 255, 255, 255, 255, 0] {
        return None;
    }
    let name = [54usize, 72, 90, 108].iter().find_map(|&at| {
        let block = &edid[at..at + 18];
        (block[0] == 0 && block[1] == 0 && block[3] == 0xFC).then(|| {
            let text: String =
                block[5..].iter().take_while(|&&b| b != 0x0A && b != 0).map(|&b| char::from(b)).collect();
            text.trim().to_owned()
        })
    })?;
    if name.is_empty() {
        return None;
    }
    if !name.starts_with(|c: char| c.is_ascii_digit()) {
        return Some(name);
    }
    let code = u16::from_be_bytes([edid[8], edid[9]]);
    let maker: String = [(code >> 10) & 31, (code >> 5) & 31, code & 31]
        .iter()
        .filter(|&&letter| (1..=26).contains(&letter))
        .map(|&letter| char::from(b'A' - 1 + letter as u8))
        .collect();
    Some(if maker.len() == 3 { format!("{maker} {name}") } else { name })
}

/// A display connector's kind and number, alike however a system writes them: Linux's own
/// ("HDMI-A-1", "Unknown-2", "DP-1") and the window systems' ("HDMI-1", "None-2", "HDMI-0").
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn connector_parts(name: &str) -> Option<(String, &str)> {
    let (kind, number) = name.rsplit_once('-')?;
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let kind = kind.to_ascii_lowercase();
    let kind = match kind.as_str() {
        "hdmi-a" | "hdmi-b" => "hdmi".to_owned(),
        "unknown" | "none" => "none".to_owned(),
        "displayport" => "dp".to_owned(),
        "dvi-i" | "dvi-d" | "dvi-a" => "dvi".to_owned(),
        _ => kind,
    };
    Some((kind, number))
}

/// Each display the window system names on Linux, with the name of the monitor connected to
/// the same connector (`connected`: Linux's connector names, each with the monitor's EDID name).
/// The same kind and number match; failing that, the only one of its kind on both sides (X11
/// counts from 0 where Linux counts from 1). Displays with no match keep the window system's
/// name.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn linux_names(window_names: &[String], connected: &[(String, String)]) -> Vec<(String, String)> {
    let mut names = Vec::new();
    for window_name in window_names {
        let Some((kind, number)) = connector_parts(window_name) else { continue };
        let same = connected.iter().find(|(connector, _)| connector_parts(connector) == Some((kind.clone(), number)));
        let only = || {
            let of_kind = |name: &&String| connector_parts(name).is_some_and(|(k, _)| k == kind);
            let theirs: Vec<&(String, String)> = connected.iter().filter(|(c, _)| of_kind(&c)).collect();
            (theirs.len() == 1 && window_names.iter().filter(of_kind).count() == 1).then(|| theirs[0])
        };
        if let Some((_, name)) = same.or_else(only) {
            names.push((window_name.clone(), name.clone()));
        }
    }
    names
}

/// The monitors Linux lists as connected, each by its connector ("HDMI-A-1") with the name
/// from its EDID.
#[cfg(target_os = "linux")]
pub fn connected_monitors() -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir("/sys/class/drm") else { return Vec::new() };
    let mut found: Vec<(String, String)> = entries
        .flatten()
        .filter_map(|entry| {
            let folder = entry.path();
            let file = entry.file_name().to_string_lossy().into_owned();
            // "card1-HDMI-A-1": the connector is what follows the card.
            let connector = file.strip_prefix("card")?.split_once('-')?.1.to_owned();
            let status = std::fs::read_to_string(folder.join("status")).ok()?;
            if status.trim() != "connected" {
                return None;
            }
            let name = edid_name(&std::fs::read(folder.join("edid")).ok()?)?;
            Some((connector, name))
        })
        .collect();
    found.sort();
    found
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// Borderless fullscreen on the display with this index.
    Fullscreen(usize),
    /// An ordinary window, for a computer with one display.
    Windowed,
}

/// Why no screen may fill a display: the operator works there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    /// It is the only display connected: a computer with one display has only the operator's.
    Only,
    /// It is the main display, with the menu bar (or the taskbar).
    Main,
    /// The window system doesn't say which display is the main one (Wayland), and SoftPresenter's
    /// own window is on this one.
    HomeWindow,
}

/// Whether the display at `index` is the operator's, so no screen fills it, and why. `operator`
/// are the displays the operator works on (the one holding the home window, and the main one),
/// by index, as far as the window system says; they count only where no display is known as
/// the main one, so a home window dragged onto a chosen display on a Mac or Windows never takes
/// the screen off it mid-service.
pub fn kept_for_operator(displays: &[DisplayInfo], operator: &[usize], index: usize) -> Option<Kept> {
    let display = displays.iter().find(|d| d.index == index)?;
    if displays.len() == 1 {
        Some(Kept::Only)
    } else if display.main {
        Some(Kept::Main)
    } else if !displays.iter().any(|d| d.main) && operator.contains(&index) {
        Some(Kept::HomeWindow)
    } else {
        None
    }
}

/// The displays a screen left to choose for itself (Automatic) may fill, in order: each that
/// isn't the operator's (see `kept_for_operator`) and doesn't hold the home window. None when it
/// isn't known which display the operator is on (`operator` is empty): a guess that was wrong
/// would cover the operator's screen and leave them nothing to work with.
pub fn for_automatic<'a>(displays: &'a [DisplayInfo], operator: &'a [usize]) -> impl Iterator<Item = &'a DisplayInfo> {
    displays.iter().filter(move |d| {
        !operator.is_empty() && !operator.contains(&d.index) && kept_for_operator(displays, operator, d.index).is_none()
    })
}

/// Picks where the output goes. `operator` are the displays the operator works on: the one
/// holding the home window and the one with the menu bar (they are usually the same).
///
/// A requested display is used if it exists. Otherwise the output takes the first display
/// `for_automatic` gives, and falls back to a window when there is none: with one display, or
/// when it isn't known which display is the operator's.
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
    Ok(for_automatic(displays, operator).map(|d| d.index).min().map_or(Placement::Windowed, Placement::Fullscreen))
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

    /// The displays as `display_list` names them on Windows: `devices` are the window library's
    /// names for them, in its order (the first is the main display), and `names` what
    /// `sp_winapi::monitor_names` gives.
    fn listed_on_windows(devices: &[&str], names: &[(&str, &str)]) -> Vec<DisplayInfo> {
        let names: Vec<(String, String)> = names.iter().map(|(d, n)| ((*d).to_owned(), (*n).to_owned())).collect();
        let mut list: Vec<DisplayInfo> = Vec::new();
        for (index, device) in devices.iter().enumerate() {
            let old_name = (*device).to_owned();
            let name = listed_name(own_name(&names, &old_name), &old_name, &list);
            list.push(DisplayInfo { index, name, old_name, main: index == 0, ..Default::default() });
        }
        list
    }

    fn names_of(list: &[DisplayInfo]) -> Vec<&str> {
        list.iter().map(|d| d.name.as_str()).collect()
    }

    /// An EDID's first block with a name in its last descriptor, from maker code `maker`.
    fn edid(maker: [u8; 2], name: &[u8]) -> Vec<u8> {
        let mut block = vec![0u8; 128];
        block[..8].copy_from_slice(&[0, 255, 255, 255, 255, 255, 255, 0]);
        block[8..10].copy_from_slice(&maker);
        block[108..113].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
        let mut text = name.to_vec();
        text.push(0x0A);
        text.resize(13, b' ');
        block[113..126].copy_from_slice(&text);
        block
    }

    #[test]
    fn a_monitors_name_is_read_from_its_edid_with_its_maker_before_a_bare_model() {
        // AOC is 0x05E3, HP's HPN is 0x2223: the codes' letters, five bits each.
        assert_eq!(edid_name(&edid([0x05, 0xE3], b"28E850")).as_deref(), Some("AOC 28E850"));
        assert_eq!(edid_name(&edid([0x22, 0x23], b"HP 27f")).as_deref(), Some("HP 27f"));
        assert_eq!(edid_name(&edid([0x1E, 0x6D], b"LG ULTRAGEAR")).as_deref(), Some("LG ULTRAGEAR"));
        assert_eq!(edid_name(&edid([0x05, 0xE3], b"   ")), None);
        assert_eq!(edid_name(&[0u8; 128]), None, "not an EDID");
        assert_eq!(edid_name(&[0u8; 10]), None);
    }

    #[test]
    fn linux_displays_take_the_name_of_the_monitor_on_their_connector() {
        let connected = vec![
            ("DP-1".to_owned(), "HP 27f".to_owned()),
            ("Unknown-2".to_owned(), "AOC 28E850".to_owned()),
            ("HDMI-A-1".to_owned(), "Lobby TV".to_owned()),
        ];
        // Wayland on GNOME: "None" for a connector the driver calls Unknown, "HDMI" for HDMI-A.
        let window = vec!["DP-1".to_owned(), "None-2".to_owned(), "HDMI-1".to_owned(), "DP-3".to_owned()];
        let names = linux_names(&window, &connected);
        assert_eq!(
            names,
            [
                ("DP-1".to_owned(), "HP 27f".to_owned()),
                ("None-2".to_owned(), "AOC 28E850".to_owned()),
                ("HDMI-1".to_owned(), "Lobby TV".to_owned()),
            ],
            "DP-3 has no monitor of its own: it keeps its name"
        );
        // X11 with NVIDIA's driver counts from 0: the only HDMI on both sides still matches,
        // but not when there are two of a kind.
        let x11 = vec!["DP-0".to_owned(), "HDMI-0".to_owned()];
        let names = linux_names(&x11, &connected);
        assert!(names.contains(&("HDMI-0".to_owned(), "Lobby TV".to_owned())), "{names:?}");
        assert!(names.contains(&("DP-0".to_owned(), "HP 27f".to_owned())), "{names:?}");
        let two = vec![("DP-1".to_owned(), "A".to_owned()), ("DP-2".to_owned(), "B".to_owned())];
        assert!(linux_names(&["DP-0".to_owned()], &two).is_empty());
    }

    #[test]
    fn a_moment_without_monitor_names_keeps_the_last_ones() {
        let named = vec![(r"\\.\DISPLAY2".to_owned(), "Mi TV".to_owned())];
        let mut last = Vec::new();
        assert_eq!(names_or_last(named.clone(), &mut last), named);
        // Windows didn't answer: the names stay, so a screen saved as "Mi TV" stays up.
        assert_eq!(names_or_last(Vec::new(), &mut last), named);
        // A new answer replaces them.
        let other = vec![(r"\\.\DISPLAY3".to_owned(), "Projector".to_owned())];
        assert_eq!(names_or_last(other.clone(), &mut last), other);
        assert_eq!(last, other);
    }

    #[test]
    fn windows_lists_displays_by_the_monitors_own_names_and_keeps_the_device_names() {
        let list = listed_on_windows(
            &[r"\\.\DISPLAY1", r"\\.\DISPLAY2"],
            &[(r"\\.\DISPLAY2", "Mi TV"), (r"\\.\DISPLAY1", "Philips FTV")],
        );
        assert_eq!(names_of(&list), ["Philips FTV", "Mi TV"]);
        assert_eq!(list[1].old_name, r"\\.\DISPLAY2");
        // A screen saved under either name means the display; never the other display's.
        assert!(list[1].is_named("Mi TV") && list[1].is_named(r"\\.\DISPLAY2"));
        assert!(!list[1].is_named(r"\\.\DISPLAY1") && !list[1].is_named("Philips FTV"));
    }

    #[test]
    fn a_monitor_that_gives_no_name_is_listed_by_its_device_name() {
        let list = listed_on_windows(&[r"\\.\DISPLAY1", r"\\.\DISPLAY2"], &[(r"\\.\DISPLAY2", "Mi TV")]);
        assert_eq!(names_of(&list), [r"\\.\DISPLAY1", "Mi TV"]);
        assert!(list[0].is_named(r"\\.\DISPLAY1"));
        // A blank name is no name; one with spaces round it is the name.
        let list = listed_on_windows(
            &[r"\\.\DISPLAY1", r"\\.\DISPLAY2"],
            &[(r"\\.\DISPLAY1", "   "), (r"\\.\DISPLAY2", " Mi TV ")],
        );
        assert_eq!(names_of(&list), [r"\\.\DISPLAY1", "Mi TV"]);
        // Where Windows can't be asked (and on Linux) every display keeps its own name.
        let list = listed_on_windows(&[r"\\.\DISPLAY1", r"\\.\DISPLAY2"], &[]);
        assert_eq!(names_of(&list), [r"\\.\DISPLAY1", r"\\.\DISPLAY2"]);
    }

    #[test]
    fn monitors_of_the_same_name_are_told_apart_in_order() {
        let list = listed_on_windows(
            &[r"\\.\DISPLAY1", r"\\.\DISPLAY2", r"\\.\DISPLAY3"],
            &[(r"\\.\DISPLAY1", "Philips FTV"), (r"\\.\DISPLAY2", "Philips FTV"), (r"\\.\DISPLAY3", "Philips FTV")],
        );
        assert_eq!(names_of(&list), ["Philips FTV", "Philips FTV (2)", "Philips FTV (3)"]);
        assert!(list[2].is_named(r"\\.\DISPLAY3"));
    }

    #[test]
    fn a_display_shown_on_two_monitors_takes_the_first_ones_name() {
        let list = listed_on_windows(
            &[r"\\.\DISPLAY1", r"\\.\DISPLAY2"],
            &[(r"\\.\DISPLAY1", "Philips FTV"), (r"\\.\DISPLAY1", "Mi TV"), (r"\\.\DISPLAY2", "Lobby TV")],
        );
        assert_eq!(names_of(&list), ["Philips FTV", "Lobby TV"]);
    }

    #[test]
    fn a_macs_displays_are_found_by_display_id() {
        let names = vec![(5_u32, "3DTV".to_owned()), (1, "HP 27f".to_owned())];
        assert_eq!(own_name(&names, &1), Some("HP 27f"));
        assert_eq!(own_name(&names, &7), None);
        assert_eq!(listed_name(own_name(&names, &5), "Monitor #0", &[]), "3DTV");
    }

    #[test]
    fn a_lone_display_is_the_operators_whatever_the_system_says() {
        // Wayland: nothing is the main display, and the home window's display may not be known.
        let one = displays(1);
        for operator in [&[][..], &[0]] {
            assert_eq!(kept_for_operator(&one, operator, 0), Some(Kept::Only));
            assert_eq!(for_automatic(&one, operator).count(), 0);
            assert_eq!(choose_output(&one, operator, None), Ok(Placement::Windowed));
        }
        // A Mac or Windows: the one display is the main one too; it is still the only one.
        let mut main = displays(1);
        main[0].main = true;
        assert_eq!(kept_for_operator(&main, &[0], 0), Some(Kept::Only));
    }

    #[test]
    fn with_a_known_main_display_only_it_is_kept() {
        let mut three = displays(3);
        three[0].main = true;
        assert_eq!(kept_for_operator(&three, &[0], 0), Some(Kept::Main));
        // The home window moved to display 1: a screen chosen for that display keeps it, so an
        // operator dragging the window across never drops the audience screen mid-service;
        // Automatic doesn't take it.
        assert_eq!(kept_for_operator(&three, &[1, 0], 1), None);
        assert_eq!(for_automatic(&three, &[1, 0]).map(|d| d.index).collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn with_no_known_main_display_the_home_windows_stands_for_it() {
        let two = displays(2);
        assert_eq!(kept_for_operator(&two, &[0], 0), Some(Kept::HomeWindow));
        assert_eq!(kept_for_operator(&two, &[0], 1), None);
        assert_eq!(for_automatic(&two, &[0]).map(|d| d.index).collect::<Vec<_>>(), [1]);
        assert_eq!(choose_output(&two, &[0], None), Ok(Placement::Fullscreen(1)));
        // Not known where the home window is: nothing is kept, and Automatic takes nothing.
        assert_eq!(kept_for_operator(&two, &[], 0), None);
        assert_eq!(for_automatic(&two, &[]).count(), 0);
    }
}
