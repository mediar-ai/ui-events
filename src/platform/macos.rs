#![cfg(target_os = "macos")]

use super::PlatformListener;
use crate::event::{ApplicationInfo, ElementDetails, EventType, UiEvent, WindowInfo};
use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use cidre::arc::Retained;
use cidre::{ax, cf, ns, objc::ar_pool};
use std::cell::RefCell;
use std::ffi::c_void;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

// Store sender and current observer for the CFRunLoop thread
thread_local! {
    static SENDER: RefCell<Option<mpsc::Sender<UiEvent>>> = RefCell::new(None);
    // Store the active AXObserver and the top-level element it observes (the app element)
    static CURRENT_AX_OBSERVER: RefCell<Option<(Retained<ax::Observer>, Retained<ax::UiElement>)>> = RefCell::new(None);
    // Store the NSWorkspace observer token to remove it on cleanup
    static WORKSPACE_OBSERVER_TOKEN: RefCell<Option<Retained<ns::Id>>> = RefCell::new(None);
}

// Define the reference date epoch seconds (Unix timestamp for 2001-01-01T00:00:00Z)
const CF_ABSOLUTE_TIME_EPOCH_OFFSET: i64 = 978307200;

// The C callback function for AXObserver notifications
extern "C" fn observer_callback(
    _observer: &mut ax::Observer,
    element: &mut ax::UiElement,
    notification: &ax::Notification,
    _user_info: *mut c_void,
) {
    ar_pool(|| {
        // Directly use the Ref, or retain if ownership/longer lifetime is needed
        let element = element.retained();
        let notification = notification.retained();
        let notification_name = notification.to_string();

        debug!(%notification_name, "observer_callback received");

        SENDER.with(|cell| {
            let r = cell.borrow();
            let sender = match r.as_ref() {
                Some(s) => s,
                None => {
                    error!("sender not available in observer callback");
                    return;
                }
            };

            // Map AX notifications (cf::String constants) to our event types
            let event_type = if notification.equal(ax::notification::focused_window_changed()) {
                EventType::WindowFocused
            } else if notification.equal(ax::notification::focused_ui_element_changed()) {
                EventType::ElementFocused
            } else if notification.equal(ax::notification::value_changed()) {
                EventType::ValueChanged
            } else {
                debug!(%notification_name, "ignoring unhandled ax notification");
                return;
            };

            // Extract contextual data from the element
            match extract_event_data(&element) {
                Ok((app_info, window_info, element_details)) => {
                    let event = UiEvent {
                        event_type,
                        timestamp: Utc::now(),
                        application: app_info,
                        window: window_info,
                        element: element_details,
                        event_specific_data: None, // Populate if needed
                    };

                    // Send the event (non-blocking)
                    if let Err(e) = sender.try_send(event) {
                        error!(error = %e, "failed to send event from callback");
                    }
                }
                Err(e) => {
                    error!(error = %e, "failed to extract event data in callback");
                }
            }
        });
    });
}

// Convert common CF types to serde_json::Value
fn cf_value_to_json(cf_value: &cf::Type) -> Result<Option<serde_json::Value>> {
    // TODO: Revisit CFType conversion logic
    warn!(
        cf_type_id = cf_value.get_type_id(),
        "cf_value_to_json conversion temporarily disabled"
    );
    Ok(None)
    /*
    ar_pool(|| {
        let type_id = cf_value.get_type_id();
        let retained_value: Retained<cf::Type> = cf_value.retained();
        let type_ptr = retained_value.0 as *const cf::Type; // Access inner pointer

        Ok(if type_id == cf::String::type_id() {
            let s_ptr = type_ptr as *const cf::String; // Cast pointer
            Some(serde_json::Value::String(unsafe { (*s_ptr).to_string() })) // Unsafe deref
        } else if type_id == cf::Number::type_id() {
            let n_ptr = type_ptr as *const cf::Number; // Cast pointer
            let n_number = unsafe { &*n_ptr }; // Unsafe deref
            if n_number.is_float_type() {
                n_number
                    .to_f64()
                    .and_then(|f| serde_json::Number::from_f64(f))
                    .map(serde_json::Value::Number)
            } else {
                n_number
                    .to_i64()
                    .map(|i| i.into())
                    .map(serde_json::Value::Number)
            }
        } else if type_id == cf::Boolean::type_id() {
            let b_ptr = type_ptr as *const cf::Boolean; // Cast pointer
            Some(serde_json::json!(unsafe { (*b_ptr).value() })) // Unsafe deref
        } else if type_id == cf::Date::type_id() {
            let d_ptr = type_ptr as *const cf::Date; // Cast pointer
            let d_date = unsafe { &*d_ptr }; // Unsafe deref
            let abs_time = d_date.abs_time(); // f64 seconds since epoch
            let unix_timestamp_secs = CF_ABSOLUTE_TIME_EPOCH_OFFSET + abs_time as i64;
            let unix_timestamp_nanos = (abs_time.fract() * 1_000_000_000.0) as u32;
            let datetime = chrono::DateTime::from_timestamp(unix_timestamp_secs, unix_timestamp_nanos)
                              .ok_or_else(|| anyhow!("Invalid date conversion from CFAbsoluteTime"))?;
            Some(serde_json::json!(datetime.to_rfc3339()))
        } else {
             warn!(cf_type_id = type_id, description = ?retained_value.desc(), "unhandled cf type for element value");
             None
        })
    })
    */
}

// Helper to extract application, window, and element info from an AXUIElement
fn extract_event_data(
    element: &ax::UiElement,
) -> Result<(
    Option<ApplicationInfo>,
    Option<WindowInfo>,
    Option<ElementDetails>,
)> {
    let mut app_info: Option<ApplicationInfo> = None;
    let mut window_info: Option<WindowInfo> = None;

    let pid = element.pid().ok();

    if let Some(p) = pid {
        ar_pool(|| {
            // Requires 'appkit' feature
            let app = ns::running_application::RunningApp::with_pid(p);
            let app_name = app.and_then(|a| a.localized_name()).map(|s| s.to_string());
            app_info = Some(ApplicationInfo {
                name: app_name,
                pid: Some(p),
            });
        });
    }

    // Find the containing window by traversing up the accessibility hierarchy
    let window_element = std::iter::successors(Some(element.retained()), |el| el.parent().ok())
        .find(|el| {
            el.role()
                .map(|r| r.equal(ax::role::window()))
                .unwrap_or(false)
        });

    if let Some(win) = &window_element {
        // Use copy_attribute_value for attributes
        let title = win
            .attr_value(ax::attr::title())
            .ok()
            .and_then(|_val: Retained<cf::Type>| None::<String>);
        window_info = Some(WindowInfo { title, id: None });
    }

    // Get details of the specific element that received the notification
    let role = element.role().map(|r| r.to_string()).ok(); // role() returns Result
    let identifier = element
        .attr_value(ax::attr::title())
        .ok()
        .and_then(|_val: Retained<cf::Type>| None::<String>);

    let value = element
        .attr_value(ax::attr::value())
        .ok()
        .and_then(|_cf_val| None::<serde_json::Value>); // Temporarily set value to None

    let element_details = ElementDetails {
        role,
        identifier,
        value,
        position: None, // TODO
        size: None,     // TODO
    };

    Ok((app_info, window_info, Some(element_details)))
}

// Function called by NSWorkspace notification observer when an app activates
// Changed signature to accept &ns::running_application::RunningApp
fn handle_activation(app: &ns::running_application::RunningApp, sender: &mpsc::Sender<UiEvent>) {
    ar_pool(|| {
        let pid = app.pid();
        let app_name = app.localized_name().map(|s| s.to_string());
        info!(app_name = ?app_name, pid, "activated app");

        // --- Send ApplicationActivated Event ---
        let event = UiEvent {
            event_type: EventType::ApplicationActivated,
            timestamp: Utc::now(),
            application: Some(ApplicationInfo {
                name: app_name.clone(),
                pid: Some(pid),
            }),
            window: None,
            element: None,
            event_specific_data: None,
        };
        if let Err(e) = sender.try_send(event) {
            error!(error = %e, "failed to send activation event");
        }

        CURRENT_AX_OBSERVER.with(|cell| {
            if cell.borrow().is_some() {
                debug!(pid = pid, "dropping old axobserver");
                *cell.borrow_mut() = None;
            }

            // Get app element using pid
            let app_element = ax::UiElement::with_app_pid(pid);

            // Retry Observer::new_for_app
            match ax::Observer::with_cb(pid, observer_callback) {
                Ok(mut observer) => { // observer should be Retained<ax::Observer>
                    info!(pid, "created new axobserver");

                    let notifications_to_add = [
                        ax::notification::focused_window_changed(),
                        ax::notification::focused_ui_element_changed(),
                        ax::notification::value_changed(),
                    ];

                    for notif_name in notifications_to_add {
                        // Observer expects &cf::String for notification name
                        // Call add_notification on the observer instance
                        match observer.add_notification(&app_element, notif_name, std::ptr::null_mut()) {
                            Ok(_) => info!(pid, notification = %notif_name.to_string(), "added notification"),
                            Err(e) => error!(pid, notification = %notif_name.to_string(), error = ?e, "failed to add notification"),
                        }
                    }

                    // Call run_loop_source on the observer instance
                    let source = observer.run_loop_src(); // Should be Retained<cf::RunLoopSource>
                    // Use add_source with as_ref()
                    cf::RunLoop::current().add_src(source, cf::RunLoopMode::default());
                    info!(pid, "added run loop source for observer");

                    // Store the observer
                    *cell.borrow_mut() = Some((observer, app_element));
                }
                Err(e) => {
                    error!(pid, error = ?e, "failed to create axobserver for pid");
                }
            }
        });
    });
}

// --- MacosListener Implementation ---

pub struct MacosListener {}

impl MacosListener {
    pub fn new() -> Result<Self> {
        info!("checking accessibility permissions...");
        if !ax::is_process_trusted_with_prompt(true) {
            error!("accessibility permissions not granted");
            return Err(anyhow!("accessibility permissions not granted by user"));
        }
        info!("accessibility permissions granted");
        Ok(Self {})
    }
}

impl PlatformListener for MacosListener {
    fn run(&self, sender: mpsc::Sender<UiEvent>) -> Result<()> {
        if !ax::is_process_trusted_with_prompt(true) {
            error!("accessibility permissions not granted");
            return Err(anyhow!("accessibility permissions not granted by user"));
        }
        info!(
            "macos listener starting run() on thread {:?}...",
            std::thread::current().id()
        );

        SENDER.with(|cell| {
            *cell.borrow_mut() = Some(sender.clone());
        });
        info!("sender stored in thread_local");

        extern "C" fn observer_cb(
            _observer: &mut ax::Observer,
            _elem: &mut ax::UiElement,
            notification: &ax::Notification,
            _context: *mut std::ffi::c_void,
        ) {
            // Access sender via thread_local
            SENDER.with(|cell| {
                let r = cell.borrow();
                if let Some(sender_ref) = r.as_ref() {
                    println!("{:?}", notification); // Consider using debug! or info!
                    // TODO: hack hardcoded cursor
                    let apps = ns::Workspace::shared().running_apps();
                    let current_app = apps
                        .iter()
                        .find(|app| {
                            app.localized_name()
                                .map(|name| name.to_string())
                                .unwrap_or_default()
                                .to_lowercase()
                                == "cursor"
                        })
                        .unwrap();

                    handle_activation(&current_app, sender_ref);
                } else {
                    error!("sender not available in observer_cb");
                }
            });
        }

        let pid = ns::Workspace::shared()
            .running_apps()
            .iter()
            .find(|app| {
                debug!(
                    "app {:?} is_active: {:?}",
                    app.localized_name(),
                    app.is_active()
                );
                app.localized_name()
                    .map(|name| name.to_string())
                    .unwrap_or_default()
                    .to_lowercase()
                    == "cursor"
            })
            .map(|app| app.pid())
            .unwrap_or_default();
        let app = ax::UiElement::with_app_pid(pid as i32);
        info!("app: {:?}", app.desc());
        // Assuming with_cb expects context, but it's not used here because we use thread_local
        let mut observer = ax::Observer::with_cb(pid as i32, observer_cb)
            .context("failed to create observer")
            .unwrap();
        info!("observer: {:?}", observer);

        // let observer = ns::Workspace::shared().notification_center().add_observer(
        //     &ns::NotificationName::with_str("nope"),
        //     None,
        //     None,
        //     |n| {
        //         println!("app activated {:?}", n);
        //     },
        // );

        observer
            .add_notification(
                &app,
                ax::notification::app_activated(),
                std::ptr::null_mut(), // Pass null context
            )
            .unwrap();

        observer
            .add_notification(
                &app,
                ax::notification::app_deactivated(),
                std::ptr::null_mut(), // Pass null context
            )
            .unwrap();

        cf::RunLoop::current().add_src(observer.run_loop_src(), cf::RunLoopMode::default());

        info!("added application activation/deactivation observer");
        // Note: The previous code for NSWorkspace observer seemed disabled/commented out.
        // This setup only observes the current app's activation/deactivation.
        // If you need system-wide app activation, the NSWorkspace approach might be needed.

        info!("starting cf run loop (blocking current thread)... Awaiting UI events.");
        cf::RunLoop::run(); // This blocks the thread

        warn!("cf run loop finished! Performing cleanup (this is unexpected).");
        // Cleanup for NSWorkspace observer (if it were active)
        WORKSPACE_OBSERVER_TOKEN.with(|cell| {
            if let Some(token) = cell.borrow_mut().take() {
                ar_pool(|| {
                    ns::Workspace::shared()
                        .notification_center()
                        .remove_observer(&token);
                });
                info!("removed ns workspace observer");
            }
        });
        // Cleanup for AXObserver (if it were active via CURRENT_AX_OBSERVER)
        CURRENT_AX_OBSERVER.with(|cell| {
            if cell.borrow().is_some() {
                debug!("dropping final axobserver during cleanup");
                *cell.borrow_mut() = None;
            }
        });
        // Cleanup sender
        SENDER.with(|cell| *cell.borrow_mut() = None);

        Ok(()) // Should technically not be reached if RunLoop runs forever
    }
}
