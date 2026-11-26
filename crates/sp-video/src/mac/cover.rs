//! A window that takes over a display at once, and the system telling us the displays changed.
//!
//! A window made full screen by the window library becomes a macOS Space of its own, which
//! slides in when it opens and slides out when it closes. A screen for the audience is plainer:
//! a borderless window put exactly on its display's frame, above the menu bar and the Dock on
//! that display, that appears and disappears without any animation. The app's presentation
//! options are never touched, so the operator's menu bar and Dock on the main display stay as
//! they are.

use std::ffi::c_void;
use std::ptr::NonNull;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_app_kit::{
    NSApplicationDidChangeScreenParametersNotification, NSScreen, NSView, NSWindowAnimationBehavior,
    NSWindowCollectionBehavior,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSNumber, NSObjectProtocol, NSString};

/// Testing aid (SOFTPRESENTER_TERMINATE_AFTER): asks the app to terminate the way the Dock's
/// Quit, a log-out, a restart and a shut-down do (`-[NSApplication terminate:]`), at the run
/// loop's next turn, so it comes as theirs does, between two events and not inside one being
/// handled. The system then tells the app it will terminate and ends the program itself; the
/// app's own Quit never goes this way. False off the main thread, where nothing is asked.
pub fn terminate_app() -> bool {
    let Some(mtm) = MainThreadMarker::new() else { return false };
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    let nobody: *const AnyObject = std::ptr::null();
    // SAFETY: `performSelector:withObject:afterDelay:` is NSObject's own, sent on the main
    // thread to the shared application, which lives as long as the program; `terminate:` takes
    // one object, its sender, which may be nil.
    unsafe {
        let _: () =
            objc2::msg_send![&*app, performSelector: objc2::sel!(terminate:), withObject: nobody, afterDelay: 0.0f64];
    }
    true
}

/// The window level of a screen's window: `kCGMainMenuWindowLevel` (24) plus one. That is above
/// the menu bar and the Dock (20) on the display it covers, so neither shows through, yet below
/// pop-up menus, the screen saver and the system's own security dialogs, which must always be
/// able to show. (`NSScreenSaverWindowLevel`, 1000, would cover those too.)
const COVER_LEVEL: isize = 25;

/// An ordinary window's level, for a test run that must stay behind everything.
const NORMAL_LEVEL: isize = 0;

/// How a screen's window behaves among Spaces and window switching: on every Space and not
/// moving when they change (so it stays put whichever Space the operator is in, with "Displays
/// have separate Spaces" on or off), never in Cmd-` window cycling, and allowed beside another
/// app's full-screen window. The whole value is set, never added to the library's.
fn behaviour() -> NSWindowCollectionBehavior {
    NSWindowCollectionBehavior::CanJoinAllSpaces
        | NSWindowCollectionBehavior::Stationary
        | NSWindowCollectionBehavior::IgnoresCycle
        | NSWindowCollectionBehavior::FullScreenAuxiliary
}

/// Puts the window whose view is `view` exactly on the display with this id (as the window
/// library names it: `MonitorHandleExtMacOS::native_id`) and shows it there, with no animation.
/// `above_menu_bar` is false only in a test that must stay behind other windows. The window is
/// shown without taking the keyboard from whoever has it.
///
/// Must be called on the main thread, with the view of a window that is alive. Fails, changing
/// nothing, when there is no such display now.
pub fn cover_display(view: NonNull<c_void>, display_id: u32, above_menu_bar: bool) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("A window is placed on the main thread.")?;
    let key = NSString::from_str("NSScreenNumber");
    let frame = NSScreen::screens(mtm)
        .iter()
        .find(|screen| {
            screen
                .deviceDescription()
                .objectForKey(&key)
                .and_then(|number| number.downcast::<NSNumber>().ok())
                .is_some_and(|number| number.unsignedIntValue() == display_id)
        })
        .map(|screen| screen.frame())
        .ok_or_else(|| format!("There is no display {display_id}."))?;
    // SAFETY: `view` is an NSView the window library made for a window the caller holds, and we
    // are on the main thread, where it may be used; it is only borrowed for these calls.
    let view: &NSView = unsafe { view.cast::<NSView>().as_ref() };
    let window = view.window().ok_or("The view is not in a window.")?;
    window.setLevel(if above_menu_bar { COVER_LEVEL } else { NORMAL_LEVEL });
    window.setCollectionBehavior(behaviour());
    // Not hidden when the app is hidden or loses the keyboard: the audience stays on screen.
    window.setCanHide(false);
    window.setHidesOnDeactivate(false);
    // Appears and goes without the system's fade or zoom.
    window.setAnimationBehavior(NSWindowAnimationBehavior::None);
    window.setFrame_display_animate(frame, true, false);
    window.orderFrontRegardless();
    Ok(())
}

/// Calls the closure whenever the displays change (one plugged in or taken away, the main one
/// changed, a size or arrangement changed). The closure runs on the main thread. Dropping the
/// watch stops it.
pub struct DisplayWatch {
    observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl DisplayWatch {
    pub fn start(on_change: impl Fn() + Send + Sync + 'static) -> DisplayWatch {
        let center = NSNotificationCenter::defaultCenter();
        let block = RcBlock::new(move |_note: NonNull<NSNotification>| on_change());
        // SAFETY: the name is a constant of AppKit's; the block takes the notification and
        // returns nothing, as the method wants; no queue is given, so it runs on the thread that
        // posts, the main thread.
        let observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(NSApplicationDidChangeScreenParametersNotification),
                None,
                None,
                &block,
            )
        };
        DisplayWatch { observer }
    }
}

impl Drop for DisplayWatch {
    fn drop(&mut self) {
        // SAFETY: the observer was added to this centre by `start` and is removed once.
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(AsRef::<AnyObject>::as_ref(&*self.observer)) };
    }
}
