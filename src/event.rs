use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// TODO: Define more specific event types and details based on AXObserver/UIA/AT-SPI capabilities

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventType {
    ApplicationActivated,
    ApplicationDeactivated,
    WindowFocused,
    WindowCreated,
    WindowMoved,
    WindowResized,
    // WindowClosed,  // Maybe useful?
    ElementFocused,
    ValueChanged,
    ElementDestroyed,
    MenuOpened,
    MenuClosed,
    MenuItemSelected,
    SelectionChanged,
    SelectedTextChanged,
    TitleChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationInfo {
    pub name: Option<String>,
    pub pid: Option<i32>, // Or appropriate type
                          // pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub title: Option<String>,
    pub id: Option<String>, // Platform-specific ID
                            // pub position: Option<Position>,
                            // pub size: Option<Size>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementDetails {
    pub role: Option<String>,             // Standardized role if possible
    pub identifier: Option<String>,       // Accessibility Label/Name
    pub value: Option<serde_json::Value>, // Current value (flexible type)
    pub position: Option<Position>,
    pub size: Option<Size>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiEvent {
    pub event_type: EventType,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub timestamp: DateTime<Utc>,
    pub application: Option<ApplicationInfo>,
    pub window: Option<WindowInfo>,
    pub element: Option<ElementDetails>,
    // Specific data not fitting above, use sparingly
    pub event_specific_data: Option<serde_json::Value>,
}
