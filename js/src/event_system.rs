//! Event System - DOM Event bubbling and listener management
//!
//! Implements proper DOM event propagation:
//! 1. Capture phase (root -> target)
//! 2. Target phase
//! 3. Bubble phase (target -> root)

use std::cell::RefCell;
use std::collections::HashMap;

use boa_engine::{
    js_string, Context, JsObject, JsValue,
};

/// Event listener options
#[derive(Debug, Clone)]
pub struct ListenerOptions {
    pub capture: bool,
    pub once: bool,
    pub passive: bool,
}

impl Default for ListenerOptions {
    fn default() -> Self {
        Self {
            capture: false,
            once: false,
            passive: false,
        }
    }
}

/// A registered event listener
#[derive(Clone)]
pub struct EventListener {
    pub event_type: String,
    pub callback: JsObject,
    pub options: ListenerOptions,
    pub id: u64,
}

/// Thread-local event listener registry (JS is single-threaded)
thread_local! {
    static EVENT_REGISTRY: RefCell<EventRegistry> = RefCell::new(EventRegistry::new());
    static LISTENER_ID_COUNTER: RefCell<u64> = const { RefCell::new(0) };
}

/// Registry that maps element IDs to their event listeners
#[derive(Default)]
pub struct EventRegistry {
    /// element_id -> event_type -> listeners
    listeners: HashMap<u64, HashMap<String, Vec<EventListener>>>,
    /// Currently focused element ID
    active_element_id: Option<u64>,
}

impl EventRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an event listener to an element
    pub fn add_listener(&mut self, element_id: u64, listener: EventListener) {
        self.listeners
            .entry(element_id)
            .or_insert_with(HashMap::new)
            .entry(listener.event_type.clone())
            .or_insert_with(Vec::new)
            .push(listener);
    }

    /// Remove an event listener from an element
    pub fn remove_listener(&mut self, element_id: u64, event_type: &str, callback: &JsObject, capture: bool) {
        if let Some(type_map) = self.listeners.get_mut(&element_id) {
            if let Some(listeners) = type_map.get_mut(event_type) {
                listeners.retain(|l| {
                    // Compare by capture phase and callback identity
                    !(l.options.capture == capture && std::ptr::eq(l.callback.as_ref(), callback.as_ref()))
                });
            }
        }
    }

    /// Get listeners for an element and event type
    pub fn get_listeners(&self, element_id: u64, event_type: &str) -> Vec<EventListener> {
        self.listeners
            .get(&element_id)
            .and_then(|type_map| type_map.get(event_type))
            .cloned()
            .unwrap_or_default()
    }

    /// Set the active element
    pub fn set_active_element(&mut self, element_id: Option<u64>) {
        self.active_element_id = element_id;
    }

    /// Get the active element ID
    pub fn get_active_element(&self) -> Option<u64> {
        self.active_element_id
    }

    /// Clear all listeners for an element
    pub fn clear_element(&mut self, element_id: u64) {
        self.listeners.remove(&element_id);
    }
}

/// Generate a unique listener ID
fn generate_listener_id() -> u64 {
    LISTENER_ID_COUNTER.with(|counter| {
        let mut c = counter.borrow_mut();
        *c += 1;
        *c
    })
}

// ============ PUBLIC API ============

/// Add an event listener
pub fn add_event_listener(
    element_id: u64,
    event_type: &str,
    callback: JsObject,
    options: ListenerOptions,
) {
    let listener = EventListener {
        event_type: event_type.to_string(),
        callback,
        options,
        id: generate_listener_id(),
    };

    EVENT_REGISTRY.with(|registry| {
        registry.borrow_mut().add_listener(element_id, listener);
    });
}

/// Remove an event listener
pub fn remove_event_listener(
    element_id: u64,
    event_type: &str,
    callback: &JsObject,
    capture: bool,
) {
    EVENT_REGISTRY.with(|registry| {
        registry.borrow_mut().remove_listener(element_id, event_type, callback, capture);
    });
}

/// Get listeners for an element
pub fn get_listeners(element_id: u64, event_type: &str) -> Vec<EventListener> {
    EVENT_REGISTRY.with(|registry| {
        registry.borrow().get_listeners(element_id, event_type)
    })
}

/// Dispatch an event with proper bubbling
/// Returns true if the event was not cancelled
pub fn dispatch_event(
    target_element_id: u64,
    event: &JsObject,
    ancestor_ids: Vec<u64>, // From immediate parent to root (document)
    context: &mut Context,
) -> bool {
    let event_type = get_event_type(event, context);
    let bubbles = get_event_bubbles(event, context);
    let cancelable = get_event_cancelable(event, context);

    let mut propagation_stopped = false;
    let mut default_prevented = false;

    // Set target
    // Note: We can't easily set the target to the actual element object here
    // because we don't have access to it. The caller should set it.

    // Phase 1: Capture phase (root -> target)
    let _ = event.set(js_string!("eventPhase"), JsValue::from(1), false, context);
    for &ancestor_id in ancestor_ids.iter().rev() {
        if propagation_stopped {
            break;
        }
        let listeners = get_listeners(ancestor_id, &event_type);
        for listener in listeners.iter().filter(|l| l.options.capture) {
            if propagation_stopped {
                break;
            }
            if let Err(e) = call_listener(&listener.callback, event, context) {
                log::warn!("Event listener error: {:?}", e);
            }
            propagation_stopped = check_propagation_stopped(event, context);

            // Handle once option
            if listener.options.once {
                remove_event_listener(ancestor_id, &event_type, &listener.callback, true);
            }
        }
    }

    // Phase 2: Target phase
    if !propagation_stopped {
        let _ = event.set(js_string!("eventPhase"), JsValue::from(2), false, context);
        let listeners = get_listeners(target_element_id, &event_type);
        for listener in listeners.iter() {
            if propagation_stopped {
                break;
            }
            if let Err(e) = call_listener(&listener.callback, event, context) {
                log::warn!("Event listener error: {:?}", e);
            }
            propagation_stopped = check_propagation_stopped(event, context);

            if listener.options.once {
                remove_event_listener(target_element_id, &event_type, &listener.callback, listener.options.capture);
            }
        }
    }

    // Phase 3: Bubble phase (target -> root)
    if bubbles && !propagation_stopped {
        let _ = event.set(js_string!("eventPhase"), JsValue::from(3), false, context);
        for &ancestor_id in ancestor_ids.iter() {
            if propagation_stopped {
                break;
            }
            let listeners = get_listeners(ancestor_id, &event_type);
            for listener in listeners.iter().filter(|l| !l.options.capture) {
                if propagation_stopped {
                    break;
                }
                if let Err(e) = call_listener(&listener.callback, event, context) {
                    log::warn!("Event listener error: {:?}", e);
                }
                propagation_stopped = check_propagation_stopped(event, context);

                if listener.options.once {
                    remove_event_listener(ancestor_id, &event_type, &listener.callback, false);
                }
            }
        }
    }

    // Check if default was prevented
    if cancelable {
        default_prevented = check_default_prevented(event, context);
    }

    !default_prevented
}

/// Set the active element
pub fn set_active_element(element_id: Option<u64>) {
    EVENT_REGISTRY.with(|registry| {
        registry.borrow_mut().set_active_element(element_id);
    });
}

/// Get the active element ID
pub fn get_active_element() -> Option<u64> {
    EVENT_REGISTRY.with(|registry| {
        registry.borrow().get_active_element()
    })
}

/// Clear the event registry (for testing)
pub fn clear_registry() {
    EVENT_REGISTRY.with(|registry| {
        let mut r = registry.borrow_mut();
        r.listeners.clear();
        r.active_element_id = None;
    });
}

// ============ HELPER FUNCTIONS ============

fn get_event_type(event: &JsObject, context: &mut Context) -> String {
    event
        .get(js_string!("type"), context)
        .ok()
        .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
        .unwrap_or_default()
}

fn get_event_bubbles(event: &JsObject, context: &mut Context) -> bool {
    event
        .get(js_string!("bubbles"), context)
        .ok()
        .and_then(|v| v.as_boolean())
        .unwrap_or(false)
}

fn get_event_cancelable(event: &JsObject, context: &mut Context) -> bool {
    event
        .get(js_string!("cancelable"), context)
        .ok()
        .and_then(|v| v.as_boolean())
        .unwrap_or(false)
}

fn check_propagation_stopped(event: &JsObject, context: &mut Context) -> bool {
    event
        .get(js_string!("cancelBubble"), context)
        .ok()
        .and_then(|v| v.as_boolean())
        .unwrap_or(false)
}

fn check_default_prevented(event: &JsObject, context: &mut Context) -> bool {
    event
        .get(js_string!("defaultPrevented"), context)
        .ok()
        .and_then(|v| v.as_boolean())
        .unwrap_or(false)
}

fn call_listener(
    callback: &JsObject,
    event: &JsObject,
    context: &mut Context,
) -> Result<(), String> {
    callback
        .call(&JsValue::undefined(), &[JsValue::from(event.clone())], context)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_listener_registry() {
        clear_registry();

        // This is a basic test - full testing requires a JS context
        let listeners = get_listeners(1, "click");
        assert!(listeners.is_empty());
    }

    #[test]
    fn test_active_element() {
        clear_registry();

        assert!(get_active_element().is_none());
        set_active_element(Some(42));
        assert_eq!(get_active_element(), Some(42));
        set_active_element(None);
        assert!(get_active_element().is_none());
    }
}
