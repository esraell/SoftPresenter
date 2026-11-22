//! Web pages on macOS: the system's web view (WKWebView), in a borderless window far off every
//! screen, its pictures taken with its own snapshot call and drawn into RGBA frames.
//!
//! Two of the web view's settings used here are not in Apple's published interface, and are
//! called only when the web view answers to them (so a macOS without them gets a slower page,
//! never a crash):
//!
//! - window occlusion detection, turned off: without it, WebKit treats a page whose window is
//!   off-screen as hidden and all but stops it (no animation frames, timers twice a second,
//!   pictures taking a second), which was measured;
//! - drawing its own background, turned off: so a page with no background of its own is
//!   see-through over the slide, as a caption layer must be.
//!
//! What a page may do is decided by [`Guard`], its navigation and interface delegate (see the
//! notes in `web.rs`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use block2::{DynBlock, RcBlock};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSBackingStoreType, NSColor, NSImage, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask};
use objc2_core_foundation::{CFRunLoop, CGPoint, CGRect, CGSize, kCFRunLoopDefaultMode};
use objc2_foundation::{NSError, NSNumber, NSObject, NSObjectProtocol, NSString, NSURL, NSURLRequest};
use objc2_web_kit::{
    WKAudiovisualMediaTypes, WKFrameInfo, WKMediaCaptureType, WKNavigation, WKNavigationAction,
    WKNavigationActionPolicy, WKNavigationDelegate, WKNavigationResponse, WKNavigationResponsePolicy,
    WKPermissionDecision, WKSecurityOrigin, WKSnapshotConfiguration, WKUIDelegate, WKWebView, WKWebViewConfiguration,
    WKWebsiteDataStore, WKWindowFeatures,
};

use crate::web::{Loading, WebFrames, may_go, on_this_computer};

/// A picture asked for and not back after this long is given up on, so a page that hangs
/// doesn't stop the pictures for good.
const GIVE_UP: Duration = Duration::from_secs(2);

/// How far the page has got, kept by its delegate: 0 loading, 1 loaded, 2 failed; and whether
/// it was opened at an address of this computer's own (a test's stand-in), the only kind that
/// may go on to plain http here.
#[derive(Default)]
struct State {
    loading: Cell<u8>,
    opened_here: Cell<bool>,
}

define_class!(
    // SAFETY:
    // - NSObject has no subclassing requirements.
    // - `Guard` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SoftPresenterWebGuard"]
    #[ivars = State]
    struct Guard;

    // SAFETY: NSObjectProtocol has no safety requirements.
    unsafe impl NSObjectProtocol for Guard {}

    // SAFETY: each method's selector, arguments and return match the protocol's.
    unsafe impl WKNavigationDelegate for Guard {
        /// A page or frame going somewhere: only to an allowed address, never a download, and
        /// never into a new window.
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide_action(
            &self,
            _web: &WKWebView,
            action: &WKNavigationAction,
            decision: &DynBlock<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            // SAFETY: plain getters on live objects WebKit hands the delegate.
            let (url, main, new_window, download) = unsafe {
                let target = action.targetFrame();
                (
                    action.request().URL(),
                    target.as_ref().is_some_and(|frame| frame.isMainFrame()),
                    target.is_none(),
                    action.shouldPerformDownload(),
                )
            };
            let here = self.ivars().opened_here.get();
            let go = !new_window && !download && url.is_some_and(|url| goes(&url, main, here));
            decision.call((if go { WKNavigationActionPolicy::Allow } else { WKNavigationActionPolicy::Cancel },));
        }

        /// What came back: shown if the web view can show it, otherwise (a download) not.
        #[unsafe(method(webView:decidePolicyForNavigationResponse:decisionHandler:))]
        fn decide_response(
            &self,
            _web: &WKWebView,
            response: &WKNavigationResponse,
            decision: &DynBlock<dyn Fn(WKNavigationResponsePolicy)>,
        ) {
            // SAFETY: a plain getter on a live object.
            let shown = unsafe { response.canShowMIMEType() };
            decision
                .call((if shown { WKNavigationResponsePolicy::Allow } else { WKNavigationResponsePolicy::Cancel },));
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        fn finished(&self, _web: &WKWebView, _navigation: Option<&WKNavigation>) {
            self.ivars().loading.set(1);
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn failed_to_start(&self, _web: &WKWebView, _navigation: Option<&WKNavigation>, _error: &NSError) {
            self.ivars().loading.set(2);
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn failed(&self, _web: &WKWebView, _navigation: Option<&WKNavigation>, _error: &NSError) {
            self.ivars().loading.set(2);
        }

        /// The page's own process stopped (it ran out of memory, say): load it again, so a
        /// screen doesn't stay blank.
        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        fn process_ended(&self, web: &WKWebView) {
            self.ivars().loading.set(0);
            // SAFETY: a plain call on a live web view, on the main thread.
            unsafe { web.reload() };
        }
    }

    // SAFETY: each method's selector, arguments and return match the protocol's.
    unsafe impl WKUIDelegate for Guard {
        /// A page asking for a new window gets none.
        #[unsafe(method_id(webView:createWebViewWithConfiguration:forNavigationAction:windowFeatures:))]
        fn new_window(
            &self,
            _web: &WKWebView,
            _configuration: &WKWebViewConfiguration,
            _action: &WKNavigationAction,
            _features: &WKWindowFeatures,
        ) -> Option<Retained<WKWebView>> {
            None
        }

        /// A page asking for the camera or microphone is refused (the app has no reason to
        /// give a slide either, and macOS would ask the operator in the middle of a service).
        #[unsafe(method(webView:requestMediaCapturePermissionForOrigin:initiatedByFrame:type:decisionHandler:))]
        fn camera(
            &self,
            _web: &WKWebView,
            _origin: &WKSecurityOrigin,
            _frame: &WKFrameInfo,
            _kind: WKMediaCaptureType,
            decision: &DynBlock<dyn Fn(WKPermissionDecision)>,
        ) {
            decision.call((WKPermissionDecision::Deny,));
        }
    }
);

impl Guard {
    fn new(mtm: MainThreadMarker) -> Retained<Guard> {
        let this = Guard::alloc(mtm).set_ivars(State::default());
        // SAFETY: NSObject's plain initialiser.
        unsafe { msg_send![super(this), init] }
    }
}

/// True for a place a frame may go (see `web::may_go`): https anywhere, plain http on this
/// computer only for a page opened here.
fn goes(url: &NSURL, main: bool, opened_here: bool) -> bool {
    let scheme = url.scheme().map(|scheme| scheme.to_string()).unwrap_or_default();
    let host = url.host().map(|host| host.to_string()).unwrap_or_default();
    may_go(&scheme, &host, main, opened_here)
}

/// A page in its off-screen window.
pub struct Page {
    window: Retained<NSWindow>,
    view: Retained<WKWebView>,
    guard: Retained<Guard>,
    frames: WebFrames,
    /// When the picture on its way was asked for.
    asked: Rc<Cell<Option<Instant>>>,
    size: (usize, usize),
}

impl Page {
    pub fn open(address: &str, width: u32, height: u32, frames: WebFrames) -> Result<Page, String> {
        let mtm = MainThreadMarker::new().ok_or("A web page is opened on the main thread.")?;
        let url = NSURL::URLWithString(&NSString::from_str(address))
            .ok_or_else(|| format!("{address} isn't a web address."))?;
        // The window: borderless, see-through, far off every screen, never in the window
        // cycle, Mission Control or the Window menu, and never taking a click.
        let place = CGRect::new(CGPoint::new(-40000.0, -40000.0), CGSize::new(f64::from(width), f64::from(height)));
        // SAFETY: a plain initialiser on the main thread.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                place,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: the window is kept by `Page` and closed in its `drop`, so AppKit must not
        // release it on close as well.
        unsafe { window.setReleasedWhenClosed(false) };
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setIgnoresMouseEvents(true);
        window.setHasShadow(false);
        window.setExcludedFromWindowsMenu(true);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::Transient
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenNone,
        );
        // The page is laid out `width` × `height` CSS pixels and drawn at that many pixels:
        // on a Retina screen (two pixels a point) its view is half the size in points, zoomed
        // to half, so WebKit doesn't draw four times the pixels only for them to be shrunk.
        let scale = window.backingScaleFactor().max(1.0);
        let points = CGSize::new(f64::from(width) / scale, f64::from(height) / scale);
        window.setContentSize(points);

        // SAFETY: plain configuration calls on new objects, on the main thread.
        let view = unsafe {
            let configuration = WKWebViewConfiguration::new(mtm);
            // Nothing is kept between pages, or on disk.
            configuration.setWebsiteDataStore(&WKWebsiteDataStore::nonPersistentDataStore(mtm));
            // A page's video or sound plays without a click: a slide is never clicked.
            configuration.setMediaTypesRequiringUserActionForPlayback(WKAudiovisualMediaTypes::None);
            configuration.preferences().setJavaScriptCanOpenWindowsAutomatically(false);
            WKWebView::initWithFrame_configuration(
                WKWebView::alloc(mtm),
                CGRect::new(CGPoint::new(0.0, 0.0), points),
                &configuration,
            )
        };
        let guard = Guard::new(mtm);
        guard.ivars().opened_here.set(on_this_computer(address));
        // SAFETY: the delegates are weak references; `guard` is kept by `Page` for as long as
        // the view.
        unsafe {
            view.setNavigationDelegate(Some(ProtocolObject::from_ref(&*guard)));
            view.setUIDelegate(Some(ProtocolObject::from_ref(&*guard)));
            view.setPageZoom(1.0 / scale);
        }
        // The two settings outside Apple's published interface (see the module's notes), each
        // only when the view answers to it.
        // SAFETY: `respondsToSelector:` is NSObject's; each setter takes one BOOL and returns
        // nothing, and is called only when the view says it has it.
        unsafe {
            let answers = |selector| -> bool { msg_send![&*view, respondsToSelector: selector] };
            if answers(sel!(_setWindowOcclusionDetectionEnabled:)) {
                let _: () = msg_send![&*view, _setWindowOcclusionDetectionEnabled: false];
            }
            if answers(sel!(_setDrawsBackground:)) {
                let _: () = msg_send![&*view, _setDrawsBackground: false];
            } else if answers(sel!(setDrawsBackground:)) {
                let _: () = msg_send![&*view, setDrawsBackground: false];
            }
        }
        window.setContentView(Some(&view));
        window.orderFrontRegardless();
        // SAFETY: a plain call on a live view with a request made here.
        unsafe { view.loadRequest(&NSURLRequest::requestWithURL(&url)) };
        Ok(Page {
            window,
            view,
            guard,
            frames,
            asked: Rc::new(Cell::new(None)),
            size: (width as usize, height as usize),
        })
    }

    /// Shows or hides the page's view in its window. Hidden, WebKit counts the page as not
    /// visible and rests it (no animation frames, slow timers); shown, it runs at once.
    pub fn set_shown(&self, shown: bool) {
        self.view.setHidden(!shown);
    }

    pub fn loading(&self) -> Loading {
        match self.guard.ivars().loading.get() {
            1 => Loading::Loaded,
            2 => Loading::Failed,
            _ => Loading::Loading,
        }
    }

    pub fn capture(&self) {
        // Nothing until the page has loaded, so a half-drawn page is never shown.
        if self.loading() != Loading::Loaded || self.asked.get().is_some_and(|at| at.elapsed() < GIVE_UP) {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else { return };
        self.asked.set(Some(Instant::now()));
        let (asked, frames, (width, height)) = (self.asked.clone(), self.frames.clone(), self.size);
        let done = RcBlock::new(move |image: *mut NSImage, _error: *mut NSError| {
            asked.set(None);
            // SAFETY: WebKit passes a live image, or null when there is none.
            let Some(image) = (unsafe { image.as_ref() }) else { return };
            // SAFETY: a null rectangle and no context or hints ask for the whole image.
            let picture = unsafe { image.CGImageForProposedRect_context_hints(std::ptr::null_mut(), None, None) };
            if let Some(frame) = picture.and_then(|picture| crate::mac::draw_at(&picture, width, height)) {
                frames.put(frame);
            }
        });
        // SAFETY: plain calls on the main thread; the block is copied by WebKit and called
        // once, on the main thread.
        unsafe {
            let configuration = WKSnapshotConfiguration::new(mtm);
            configuration.setAfterScreenUpdates(false);
            configuration.setSnapshotWidth(Some(&NSNumber::new_f64(self.view.frame().size.width)));
            self.view.takeSnapshotWithConfiguration_completionHandler(Some(&configuration), &done);
        }
    }

    pub fn ask(&self, script: &str, reply: impl FnOnce(Option<String>) + 'static) {
        let reply = RefCell::new(Some(reply));
        let done = RcBlock::new(move |result: *mut AnyObject, _error: *mut NSError| {
            // SAFETY: WebKit passes the script's result as a live object, or null.
            let text = unsafe { result.as_ref() }
                .and_then(|result| result.downcast_ref::<NSString>())
                .map(|text| text.to_string());
            if let Some(reply) = reply.borrow_mut().take() {
                reply(text);
            }
        });
        // SAFETY: a plain call on the main thread; the block is copied and called once.
        unsafe { self.view.evaluateJavaScript_completionHandler(&NSString::from_str(script), Some(&done)) };
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        // SAFETY: plain calls on live objects, on the main thread (where `Page` lives).
        unsafe {
            self.view.stopLoading();
            self.view.setNavigationDelegate(None);
            self.view.setUIDelegate(None);
        }
        self.window.setContentView(None);
        self.window.close();
    }
}

/// Runs the main thread's run loop for a while.
pub fn pump(seconds: f64) {
    // SAFETY: the default mode is a static string the framework defines; running the current
    // thread's run loop for a while has no other requirement.
    unsafe { CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, seconds, false) };
}
