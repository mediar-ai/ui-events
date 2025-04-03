/*
This file implements the `PlatformListener` trait for macOS.
It leverages Apple's Accessibility API (AXUIElement, AXObserver) via the `cidre` crate
to capture UI events such as application activation, window focus changes,
and UI element interactions (focus, value changes).

Events are captured using callbacks registered with `AXObserver` on a dedicated thread
running a `CFRunLoop`. Captured event data is then structured into a `UiEvent`
and sent asynchronously through an `mpsc::Sender` provided during initialization.

Key components:
- `cidre`: Rust bindings for Apple frameworks (Core Foundation, AppKit, Accessibility).
- `ax`: Accessibility API specific types within `cidre`.
- `cf`: Core Foundation types (RunLoop, String, etc.) within `cidre`.
- `ns`: AppKit types (Workspace, Application) within `cidre`.
- `tokio::sync::mpsc`: Used for sending events back to the main application logic.
- `thread_local!`: Used to store the sender and observer state within the C callback context.
*/

#![cfg(target_os = "macos")]

use super::PlatformListener;
use crate::event::{
    ApplicationInfo, ElementDetails, EventType, Position, Size, UiEvent, WindowInfo,
};
use anyhow::{Result, anyhow};
use chrono::Utc;
use cidre::arc::Retained;
use cidre::{ax, cf, ns, objc::ar_pool};
use std::cell::RefCell;
use std::ffi::c_void;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

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

        info!(%notification_name, "observer_callback received");

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
            } else if notification.equal(ax::notification::window_created()) {
                EventType::WindowCreated
            } else if notification.equal(ax::notification::window_moved()) {
                EventType::WindowMoved
            } else if notification.equal(ax::notification::window_resized()) {
                EventType::WindowResized
            } else if notification.equal(ax::notification::ui_element_destroyed()) {
                EventType::ElementDestroyed
            } else if notification.equal(ax::notification::menu_opened()) {
                EventType::MenuOpened
            } else if notification.equal(ax::notification::menu_closed()) {
                EventType::MenuClosed
            } else if notification.equal(ax::notification::menu_item_selected()) {
                EventType::MenuItemSelected
            } else if notification.equal(ax::notification::selected_text_changed()) {
                EventType::SelectedTextChanged
            } else if notification.equal(ax::notification::title_changed()) {
                EventType::TitleChanged
            } else {
                info!(%notification_name, "ignoring unhandled ax notification");
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

                    info!(%notification_name, "event sent");
                }
                Err(e) => {
                    error!(error = %e, "failed to extract event data in callback");
                }
            }
        });
    });
}

// Convert common CF types to serde_json::Value
fn cf_value_to_json(cf_value: &cf::Type) -> Option<serde_json::Value> {
    ar_pool(|| {
        let type_id = cf_value.get_type_id();
        // Remove Ok wrapping, return Option directly
        if type_id == cf::String::type_id() {
            let s_ptr = cf_value as *const cf::Type as *const cf::String;
            Some(serde_json::Value::String(unsafe { &*s_ptr }.to_string()))
        } else if type_id == cf::Number::type_id() {
            let n_ptr = cf_value as *const cf::Type as *const cf::Number;
            let n_number = unsafe { &*n_ptr };
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
            let b_ptr = cf_value as *const cf::Type as *const cf::Boolean;
            Some(serde_json::json!(unsafe { &*b_ptr }.value()))
        } else if type_id == cf::Date::type_id() {
            let d_ptr = cf_value as *const cf::Type as *const cf::Date;
            let d_date = unsafe { &*d_ptr };
            let abs_time = d_date.abs_time(); // f64 seconds since epoch
            let unix_timestamp_secs = CF_ABSOLUTE_TIME_EPOCH_OFFSET + abs_time as i64;
            let unix_timestamp_nanos = (abs_time.fract() * 1_000_000_000.0) as u32;
            // Use Option chaining instead of match
            chrono::DateTime::from_timestamp(unix_timestamp_secs, unix_timestamp_nanos)
                .map(|datetime| serde_json::json!(datetime.to_rfc3339()))
        } else {
            warn!(cf_type_id = type_id, description = ?cf_value.desc(), "unhandled cf type for element value");
            None
        }
    })
}

// Helper to safely get a string attribute from an AXUIElement
fn get_string_attribute(element: &ax::UiElement, attribute: &ax::Attr) -> Option<String> {
    ar_pool(|| {
        element.attr_value(attribute).ok().and_then(|val| {
            if val.get_type_id() == cf::String::type_id() {
                let s_ptr = &*val as *const cf::Type as *const cf::String;
                let string = unsafe { &*s_ptr }.to_string();
                if !string.is_empty() {
                    Some(string)
                } else {
                    None
                }
            } else {
                None
            }
        })
    })
}

// Helper to get position
fn get_element_position(element: &ax::UiElement) -> Option<Position> {
    ar_pool(|| {
        element.attr_value(ax::attr::pos()).ok().and_then(|val| {
            // Check if the value is an AXValue encoding a CGPoint
            if val.get_type_id() == ax::Value::type_id() {
                let value_ptr = &*val as *const cf::Type as *const ax::Value;
                let ax_value = unsafe { &*value_ptr };
                // Use cg_point() and rely on get_value's return
                if let Some(point) = ax_value.cg_point() {
                    Some(Position {
                        x: point.x,
                        y: point.y,
                    })
                } else {
                    // warn!("failed to extract cg_point or wrong type");
                    None
                }
            } else {
                // warn!("attribute was not ax_value, type_id: {}", val.get_type_id());
                None
            }
        })
    })
}

// Helper to get size
fn get_element_size(element: &ax::UiElement) -> Option<Size> {
    ar_pool(|| {
        element.attr_value(ax::attr::size()).ok().and_then(|val| {
            if val.get_type_id() == ax::Value::type_id() {
                let value_ptr = &*val as *const cf::Type as *const ax::Value;
                let ax_value = unsafe { &*value_ptr };
                // Use cg_size() and rely on get_value's return
                if let Some(size) = ax_value.cg_size() {
                    Some(Size {
                        width: size.width,
                        height: size.height,
                    })
                } else {
                    // warn!("failed to extract cg_size or wrong type");
                    None
                }
            } else {
                //    warn!("attribute was not ax_value, type_id: {}", val.get_type_id());
                None
            }
        })
    })
}

// Enhanced helper - NOT wrapped entirely in ar_pool anymore
fn extract_event_data(
    element: &ax::UiElement,
) -> Result<(
    Option<ApplicationInfo>,
    Option<WindowInfo>,
    Option<ElementDetails>,
)> {
    let pid = element.pid().ok(); // Use ok() to handle potential error

    // --- Application Info ---
    let app_info: Option<ApplicationInfo> = pid.and_then(|p| {
        ar_pool(|| {
            // Pool for NS object access
            let app = ns::running_application::RunningApp::with_pid(p);
            let app_name = app.and_then(|a| a.localized_name()).map(|s| s.to_string());
            Some(ApplicationInfo {
                name: app_name,
                pid: Some(p),
            })
        })
    });

    // --- Window Info ---
    // ar_pool might be needed here due to element access & retention
    let window_info: Option<WindowInfo> = ar_pool(|| {
        // 1. Check if the element itself is the window
        let mut window_element = element
            .role()
            .ok()
            .filter(|r| r.equal(ax::role::window()))
            .map(|_| element.retained()); // Retain if it's a window

        // 2. If not, traverse parents
        if window_element.is_none() {
            window_element = std::iter::successors(Some(element.retained()), |el| el.parent().ok())
                .find(|el| {
                    el.role()
                        .map(|r| r.equal(ax::role::window()))
                        .unwrap_or(false) // Default to false if role fetch fails
                });
        }

        // 3. If still no window, try getting the app's focused window
        if window_element.is_none() {
            if let Some(p) = pid {
                // Requires access to AX API inside pool
                let app_element = ax::UiElement::with_app_pid(p);
                window_element = app_element
                    .attr_value(ax::attr::focused_window())
                    .ok()
                    .and_then(|val| {
                        // The attribute value should be an AXUIElement
                        if val.get_type_id() == ax::UiElement::type_id() {
                            let win_ptr = &*val as *const cf::Type as *const ax::UiElement;
                            Some(unsafe { &*win_ptr }.retained()) // Retain the window element
                        } else {
                            None
                        }
                    });
            }
        }

        // Extract title if we found a window element
        window_element.and_then(|win| {
            let title = get_string_attribute(&win, ax::attr::title());
            Some(WindowInfo { title, id: None })
            // Note: win (Retained<UiElement>) goes out of scope here, pool handles release
        })
    });

    // --- Element Details ---
    // These helpers use ar_pool internally
    let role = ar_pool(|| element.role().map(|r| r.to_string()).ok());
    let identifier = get_string_attribute(element, ax::attr::title())
        .or_else(|| get_string_attribute(element, ax::attr::desc()))
        .or_else(|| get_string_attribute(element, ax::attr::help()));
    let value = ar_pool(|| {
        element
            .attr_value(ax::attr::value())
            .ok()
            .and_then(|cf_val| cf_value_to_json(&*cf_val))
    });
    let position = get_element_position(element);
    let size = get_element_size(element);

    let element_details = ElementDetails {
        role,
        identifier,
        value,
        position,
        size,
    };

    Ok((app_info, window_info, Some(element_details))) // Final Result constructed outside ar_pool
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
                info!(pid = pid, "dropping old axobserver");
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
                        // Added notifications:
                        ax::notification::window_created(),
                        ax::notification::window_moved(),
                        ax::notification::window_resized(),
                        ax::notification::ui_element_destroyed(),
                        ax::notification::menu_opened(),
                        ax::notification::menu_closed(),
                        ax::notification::menu_item_selected(),
                        ax::notification::selected_text_changed(),
                        ax::notification::title_changed(),
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

        // Store the sender for the callbacks
        SENDER.with(|cell| {
            *cell.borrow_mut() = Some(sender.clone());
        });
        info!("sender stored in thread_local");

        // --- Setup NSWorkspace Observer for App Activation ---
        let mut center = ns::Workspace::shared().notification_center();
        let sender_callback = sender.clone();

        // Define the callback closure for NSWorkspace notifications
        let workspace_callback = move |notification: &ns::Notification| {
            ar_pool(|| {
                info!(notification_name = ?notification.name(), "received workspace notification");

                let apps = ns::Workspace::shared().running_apps();
                let active_app = apps.iter().find(|app| app.is_active()).unwrap();

                handle_activation(active_app, &sender_callback);
            });
        }; // Copy the block to the heap

        // Add the observer to the notification center
        // TODO this does not work rn - so only recording the active app at startup
        let token = center.add_observer(
            // Use actual static method name for notification
            &ns::NotificationName::with_str("NSWorkspaceDidActivateApplicationNotification"),
            None, // Observe notifications from any object
            None, // Pass None for OperationQueue
            workspace_callback,
        );
        let retained_token = token.retained(); // Retain the token

        // Store the token for cleanup
        WORKSPACE_OBSERVER_TOKEN.with(|cell| {
            *cell.borrow_mut() = Some(retained_token);
        });
        info!("added ns workspace observer for app activation");

        // --- Initial Activation Handling ---
        // Handle the currently active application immediately
        let apps = ns::Workspace::shared().running_apps();
        let active_app = apps.iter().find(|app| app.is_active()).unwrap();

        handle_activation(&active_app, &sender);

        // --- Start Run Loop ---
        info!("starting cf run loop (blocking current thread)... Awaiting UI events.");
        cf::RunLoop::run(); // This blocks the thread

        warn!("cf run loop finished! Performing cleanup (this is unexpected).");
        // Cleanup for NSWorkspace observer
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
        // Cleanup for AXObserver (managed by handle_activation, but clear it finally)
        CURRENT_AX_OBSERVER.with(|cell| {
            if cell.borrow().is_some() {
                info!("dropping final axobserver during cleanup");
                *cell.borrow_mut() = None;
            }
        });
        // Cleanup sender
        SENDER.with(|cell| *cell.borrow_mut() = None);

        Ok(()) // Should technically not be reached if RunLoop runs forever
    }
}
