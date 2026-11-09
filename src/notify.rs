//! System notifications: a session's run ended (done, needs you, blocked, failed).
//!
//! Inside an app bundle (scope.app, or a test instance's) they are scope's own, posted through
//! the User Notifications framework: the banner carries scope's icon, groups under scope in
//! Notification Center, shows even while scope is in front, and clicking it opens the session.
//! macOS asks once whether scope may notify (System Settings → Notifications → scope). It only
//! allows that to apps signed with an Apple-issued certificate (Developer ID): an ad-hoc or
//! self-signed scope.app, a test instance or a bare `cargo run` binary falls back to
//! `osascript`, whose banners carry Script Editor's icon.

use std::sync::Mutex;

/// Sessions whose notification was clicked, for the app to open (`take_clicked`).
static CLICKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Sets up notifications: the delegate that shows them while scope is in front and hears
/// clicks, and (the first time) macOS's question whether scope may notify. `ctx` is repainted
/// when one is clicked.
pub fn init(ctx: &eframe::egui::Context) {
    #[cfg(target_os = "macos")]
    mac::init(ctx.clone());
    #[cfg(not(target_os = "macos"))]
    let _ = ctx;
}

/// Posts a notification for session `id`.
pub fn post(id: &str, title: &str, subtitle: &str, body: &str) {
    // Tests (src/e2e) end runs by the dozen: none of them reach the desktop.
    if cfg!(test) {
        return;
    }
    #[cfg(target_os = "macos")]
    if mac::post(id, title, subtitle, body) {
        return;
    }
    fallback(title, subtitle, body);
}

/// Through `osascript`: Script Editor's icon, no click to open.
fn fallback(title: &str, subtitle: &str, body: &str) {
    crate::runner::notify(title, subtitle, body);
}

/// The sessions whose notification was clicked since the last call.
pub fn take_clicked() -> Vec<String> {
    std::mem::take(&mut *CLICKED.lock().unwrap_or_else(|e| e.into_inner()))
}

#[cfg(target_os = "macos")]
mod mac {
    use super::CLICKED;
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{Bool, NSObject, ProtocolObject};
    use objc2::{AnyThread, DefinedClass, define_class, msg_send};
    use objc2_foundation::{NSBundle, NSDictionary, NSError, NSObjectProtocol, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions, UNNotificationRequest,
        UNNotificationResponse, UNNotificationSound, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };
    use std::sync::OnceLock;

    const SESSION_KEY: &str = "session";

    struct Ivars {
        ctx: eframe::egui::Context,
    }

    define_class!(
        #[unsafe(super(NSObject))]
        #[name = "ScopeNotificationDelegate"]
        #[ivars = Ivars]
        struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl UNUserNotificationCenterDelegate for Delegate {
            // Shown as a banner even while scope is the app in front.
            #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
            fn will_present(&self, _center: &UNUserNotificationCenter, _n: &UNNotification, handler: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>) {
                handler.call((UNNotificationPresentationOptions::Banner | UNNotificationPresentationOptions::List | UNNotificationPresentationOptions::Sound,));
            }

            // Clicked: the app opens the session.
            #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
            fn did_receive(&self, _center: &UNUserNotificationCenter, response: &UNNotificationResponse, handler: &block2::DynBlock<dyn Fn()>) {
                let info = response.notification().request().content().userInfo();
                let key = NSString::from_str(SESSION_KEY);
                if let Some(id) = info.objectForKey(&key).and_then(|v| v.downcast::<NSString>().ok()) {
                    CLICKED.lock().unwrap_or_else(|e| e.into_inner()).push(id.to_string());
                    self.ivars().ctx.request_repaint();
                }
                handler.call(());
            }
        }
    );

    /// The notification center, when scope runs from an app bundle (without one,
    /// `currentNotificationCenter` raises an exception).
    fn center() -> Option<Retained<UNUserNotificationCenter>> {
        NSBundle::mainBundle().bundleIdentifier()?;
        Some(UNUserNotificationCenter::currentNotificationCenter())
    }

    /// Set when macOS refuses scope outright (an error, not the developer's answer), e.g. for
    /// an app bundle under /tmp as test instances are: notifications fall back to osascript.
    /// Turned off by the developer, they are not sent at all.
    static REFUSED: OnceLock<()> = OnceLock::new();

    pub fn init(ctx: eframe::egui::Context) {
        let Some(center) = center() else { return };
        // The center holds its delegate weakly: this one lives as long as scope.
        let delegate: Retained<Delegate> = {
            let this = Delegate::alloc().set_ivars(Ivars { ctx });
            unsafe { msg_send![super(this), init] }
        };
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        std::mem::forget(delegate);
        let done = RcBlock::new(|granted: Bool, err: *mut NSError| {
            if let Some(e) = unsafe { err.as_ref() }.filter(|_| !granted.as_bool()) {
                eprintln!("notifications: {} (falling back to osascript)", e.localizedDescription());
                let _ = REFUSED.set(());
            }
        });
        center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound, &done);
    }

    /// Posts it as scope's own; false when it can't (no bundle, or refused), for the caller to
    /// fall back.
    pub fn post(id: &str, title: &str, subtitle: &str, body: &str) -> bool {
        let Some(center) = center() else { return false };
        if REFUSED.get().is_some() {
            return false;
        }
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(title));
        content.setSubtitle(&NSString::from_str(subtitle));
        content.setBody(&NSString::from_str(body));
        content.setSound(Some(&UNNotificationSound::defaultSound()));
        // One thread per session in Notification Center.
        content.setThreadIdentifier(&NSString::from_str(id));
        let info: Retained<NSDictionary<NSString, NSString>> = NSDictionary::from_retained_objects(&[&*NSString::from_str(SESSION_KEY)], &[NSString::from_str(id)]);
        // Untyped, as userInfo is declared; the keys and values are strings.
        let info: Retained<NSDictionary> = unsafe { Retained::cast_unchecked(info) };
        unsafe { content.setUserInfo(&info) };
        let key = NSString::from_str(&format!("{id}-{}", crate::runner::new_session_id()));
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&key, &content, None);
        center.addNotificationRequest_withCompletionHandler(&request, None);
        true
    }
}
