// MutationObserver, IntersectionObserver, ResizeObserver, PerformanceObserver
// Full implementation with real callback support

use boa_engine::{
    Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction,
    object::{ObjectInitializer, FunctionObjectBuilder}, property::Attribute, JsObject,
    js_string,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// ============================================================================
// MutationObserver Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct MutationRecord {
    pub mutation_type: String,
    pub target: Option<JsObject>,
    pub added_nodes: Vec<JsObject>,
    pub removed_nodes: Vec<JsObject>,
    pub previous_sibling: Option<JsObject>,
    pub next_sibling: Option<JsObject>,
    pub attribute_name: Option<String>,
    pub attribute_namespace: Option<String>,
    pub old_value: Option<String>,
}

impl MutationRecord {
    pub fn to_js_object(&self, context: &mut Context) -> JsResult<JsObject> {
        let added_nodes = boa_engine::object::builtins::JsArray::new(context);
        for (i, node) in self.added_nodes.iter().enumerate() {
            added_nodes.set(i as u32, node.clone(), false, context)?;
        }

        let removed_nodes = boa_engine::object::builtins::JsArray::new(context);
        for (i, node) in self.removed_nodes.iter().enumerate() {
            removed_nodes.set(i as u32, node.clone(), false, context)?;
        }

        let obj = ObjectInitializer::new(context)
            .property(js_string!("type"), js_string!(self.mutation_type.clone()), Attribute::all())
            .property(
                js_string!("target"),
                self.target.clone().map(|o| JsValue::from(o)).unwrap_or(JsValue::null()),
                Attribute::all()
            )
            .property(js_string!("addedNodes"), added_nodes, Attribute::all())
            .property(js_string!("removedNodes"), removed_nodes, Attribute::all())
            .property(
                js_string!("previousSibling"),
                self.previous_sibling.clone().map(|o| JsValue::from(o)).unwrap_or(JsValue::null()),
                Attribute::all()
            )
            .property(
                js_string!("nextSibling"),
                self.next_sibling.clone().map(|o| JsValue::from(o)).unwrap_or(JsValue::null()),
                Attribute::all()
            )
            .property(
                js_string!("attributeName"),
                self.attribute_name.clone().map(|s| JsValue::from(js_string!(s))).unwrap_or(JsValue::null()),
                Attribute::all()
            )
            .property(
                js_string!("attributeNamespace"),
                self.attribute_namespace.clone().map(|s| JsValue::from(js_string!(s))).unwrap_or(JsValue::null()),
                Attribute::all()
            )
            .property(
                js_string!("oldValue"),
                self.old_value.clone().map(|s| JsValue::from(js_string!(s))).unwrap_or(JsValue::null()),
                Attribute::all()
            )
            .build();
        Ok(obj)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MutationObserverInit {
    pub child_list: bool,
    pub attributes: bool,
    pub character_data: bool,
    pub subtree: bool,
    pub attribute_old_value: bool,
    pub character_data_old_value: bool,
    pub attribute_filter: Option<Vec<String>>,
}

impl MutationObserverInit {
    pub fn from_js_object(obj: &JsObject, context: &mut Context) -> JsResult<Self> {
        let child_list = obj.get(js_string!("childList"), context)?
            .to_boolean();
        let attributes = obj.get(js_string!("attributes"), context)?
            .to_boolean();
        let character_data = obj.get(js_string!("characterData"), context)?
            .to_boolean();
        let subtree = obj.get(js_string!("subtree"), context)?
            .to_boolean();
        let attribute_old_value = obj.get(js_string!("attributeOldValue"), context)?
            .to_boolean();
        let character_data_old_value = obj.get(js_string!("characterDataOldValue"), context)?
            .to_boolean();

        let attribute_filter_val = obj.get(js_string!("attributeFilter"), context)?;
        let attribute_filter = if let Some(arr) = attribute_filter_val.as_object() {
            if arr.is_array() {
                let length = arr.get(js_string!("length"), context)?
                    .to_u32(context)?;
                let mut filter = Vec::new();
                for i in 0..length {
                    let val = arr.get(i, context)?;
                    if let Some(s) = val.as_string() {
                        filter.push(s.to_std_string_escaped());
                    }
                }
                Some(filter)
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            child_list,
            attributes,
            character_data,
            subtree,
            attribute_old_value,
            character_data_old_value,
            attribute_filter,
        })
    }
}

#[derive(Clone)]
struct ObserverTarget {
    target: JsObject,
    options: MutationObserverInit,
}

struct MutationObserverState {
    callback: JsObject,
    targets: Vec<ObserverTarget>,
    records: Vec<MutationRecord>,
    is_connected: bool,
}

// ============================================================================
// IntersectionObserver Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct IntersectionObserverEntry {
    pub time: f64,
    pub root_bounds: Option<DOMRectReadOnly>,
    pub bounding_client_rect: DOMRectReadOnly,
    pub intersection_rect: DOMRectReadOnly,
    pub is_intersecting: bool,
    pub intersection_ratio: f64,
    pub target: Option<JsObject>,
}

impl IntersectionObserverEntry {
    pub fn to_js_object(&self, context: &mut Context) -> JsResult<JsObject> {
        let root_bounds = self.root_bounds.as_ref()
            .map(|r| r.to_js_object(context))
            .transpose()?
            .map(|o| JsValue::from(o))
            .unwrap_or(JsValue::null());

        let bounding_rect = self.bounding_client_rect.to_js_object(context)?;
        let intersection_rect = self.intersection_rect.to_js_object(context)?;

        let obj = ObjectInitializer::new(context)
            .property(js_string!("time"), self.time, Attribute::all())
            .property(js_string!("rootBounds"), root_bounds, Attribute::all())
            .property(js_string!("boundingClientRect"), bounding_rect, Attribute::all())
            .property(js_string!("intersectionRect"), intersection_rect, Attribute::all())
            .property(js_string!("isIntersecting"), self.is_intersecting, Attribute::all())
            .property(js_string!("intersectionRatio"), self.intersection_ratio, Attribute::all())
            .property(
                js_string!("target"),
                self.target.clone().map(|o| JsValue::from(o)).unwrap_or(JsValue::null()),
                Attribute::all()
            )
            .build();
        Ok(obj)
    }
}

#[derive(Debug, Clone, Default)]
pub struct IntersectionObserverInit {
    pub root: Option<JsObject>,
    pub root_margin: String,
    pub threshold: Vec<f64>,
}

struct IntersectionObserverState {
    callback: JsObject,
    options: IntersectionObserverInit,
    targets: Vec<JsObject>,
    entries: Vec<IntersectionObserverEntry>,
}

// ============================================================================
// ResizeObserver Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct ResizeObserverEntry {
    pub target: Option<JsObject>,
    pub content_rect: DOMRectReadOnly,
    pub border_box_size: Vec<ResizeObserverSize>,
    pub content_box_size: Vec<ResizeObserverSize>,
    pub device_pixel_content_box_size: Vec<ResizeObserverSize>,
}

#[derive(Debug, Clone)]
pub struct ResizeObserverSize {
    pub inline_size: f64,
    pub block_size: f64,
}

impl ResizeObserverSize {
    pub fn to_js_object(&self, context: &mut Context) -> JsResult<JsObject> {
        let obj = ObjectInitializer::new(context)
            .property(js_string!("inlineSize"), self.inline_size, Attribute::all())
            .property(js_string!("blockSize"), self.block_size, Attribute::all())
            .build();
        Ok(obj)
    }
}

impl ResizeObserverEntry {
    pub fn to_js_object(&self, context: &mut Context) -> JsResult<JsObject> {
        let content_rect = self.content_rect.to_js_object(context)?;

        let border_box = boa_engine::object::builtins::JsArray::new(context);
        for (i, size) in self.border_box_size.iter().enumerate() {
            let size_obj = size.to_js_object(context)?;
            border_box.set(i as u32, size_obj, false, context)?;
        }

        let content_box = boa_engine::object::builtins::JsArray::new(context);
        for (i, size) in self.content_box_size.iter().enumerate() {
            let size_obj = size.to_js_object(context)?;
            content_box.set(i as u32, size_obj, false, context)?;
        }

        let device_pixel_box = boa_engine::object::builtins::JsArray::new(context);
        for (i, size) in self.device_pixel_content_box_size.iter().enumerate() {
            let size_obj = size.to_js_object(context)?;
            device_pixel_box.set(i as u32, size_obj, false, context)?;
        }

        let obj = ObjectInitializer::new(context)
            .property(
                js_string!("target"),
                self.target.clone().map(|o| JsValue::from(o)).unwrap_or(JsValue::null()),
                Attribute::all()
            )
            .property(js_string!("contentRect"), content_rect, Attribute::all())
            .property(js_string!("borderBoxSize"), border_box, Attribute::all())
            .property(js_string!("contentBoxSize"), content_box, Attribute::all())
            .property(js_string!("devicePixelContentBoxSize"), device_pixel_box, Attribute::all())
            .build();
        Ok(obj)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResizeObserverOptions {
    pub box_type: String, // "content-box", "border-box", "device-pixel-content-box"
}

struct ResizeObserverState {
    callback: JsObject,
    targets: Vec<(JsObject, ResizeObserverOptions)>,
    entries: Vec<ResizeObserverEntry>,
}

// ============================================================================
// PerformanceObserver Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct PerformanceEntry {
    pub name: String,
    pub entry_type: String,
    pub start_time: f64,
    pub duration: f64,
    pub detail: Option<JsValue>,
}

impl PerformanceEntry {
    pub fn to_js_object(&self, context: &mut Context) -> JsResult<JsObject> {
        let detail = self.detail.clone().unwrap_or(JsValue::null());

        let obj = ObjectInitializer::new(context)
            .property(js_string!("name"), js_string!(self.name.clone()), Attribute::all())
            .property(js_string!("entryType"), js_string!(self.entry_type.clone()), Attribute::all())
            .property(js_string!("startTime"), self.start_time, Attribute::all())
            .property(js_string!("duration"), self.duration, Attribute::all())
            .property(js_string!("detail"), detail, Attribute::all())
            .build();

        // Add toJSON method
        let to_json = |_: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
        };
        obj.set(
            js_string!("toJSON"),
            NativeFunction::from_copy_closure(to_json).to_js_function(context.realm()),
            false,
            context
        )?;

        Ok(obj)
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceEntryList {
    pub entries: Vec<PerformanceEntry>,
}

impl PerformanceEntryList {
    pub fn to_js_object(&self, context: &mut Context) -> JsResult<JsObject> {
        // Create array-like object with methods
        let arr = boa_engine::object::builtins::JsArray::new(context);
        for (i, entry) in self.entries.iter().enumerate() {
            let entry_obj = entry.to_js_object(context)?;
            arr.set(i as u32, entry_obj, false, context)?;
        }

        let entries_clone = self.entries.clone();
        let get_entries = move |_: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            let arr = boa_engine::object::builtins::JsArray::new(ctx);
            for (i, entry) in entries_clone.iter().enumerate() {
                let entry_obj = entry.to_js_object(ctx)?;
                arr.set(i as u32, entry_obj, false, ctx)?;
            }
            Ok(JsValue::from(arr))
        };

        let entries_clone2 = self.entries.clone();
        let get_entries_by_type = move |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            let entry_type = args.get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();

            let arr = boa_engine::object::builtins::JsArray::new(ctx);
            let mut idx = 0;
            for entry in entries_clone2.iter() {
                if entry.entry_type == entry_type {
                    let entry_obj = entry.to_js_object(ctx)?;
                    arr.set(idx, entry_obj, false, ctx)?;
                    idx += 1;
                }
            }
            Ok(JsValue::from(arr))
        };

        let entries_clone3 = self.entries.clone();
        let get_entries_by_name = move |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            let name = args.get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let entry_type = args.get(1)
                .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
                .transpose()?;

            let arr = boa_engine::object::builtins::JsArray::new(ctx);
            let mut idx = 0;
            for entry in entries_clone3.iter() {
                if entry.name == name {
                    if let Some(ref et) = entry_type {
                        if &entry.entry_type != et {
                            continue;
                        }
                    }
                    let entry_obj = entry.to_js_object(ctx)?;
                    arr.set(idx, entry_obj, false, ctx)?;
                    idx += 1;
                }
            }
            Ok(JsValue::from(arr))
        };

        let obj: JsObject = arr.into();
        obj.set(
            js_string!("getEntries"),
            unsafe { NativeFunction::from_closure(get_entries) }.to_js_function(context.realm()),
            false,
            context
        )?;
        obj.set(
            js_string!("getEntriesByType"),
            unsafe { NativeFunction::from_closure(get_entries_by_type) }.to_js_function(context.realm()),
            false,
            context
        )?;
        obj.set(
            js_string!("getEntriesByName"),
            unsafe { NativeFunction::from_closure(get_entries_by_name) }.to_js_function(context.realm()),
            false,
            context
        )?;

        Ok(obj)
    }
}

#[derive(Debug, Clone, Default)]
pub struct PerformanceObserverInit {
    pub entry_types: Option<Vec<String>>,
    pub type_: Option<String>,
    pub buffered: bool,
}

struct PerformanceObserverState {
    callback: JsObject,
    options: Option<PerformanceObserverInit>,
    entries: Vec<PerformanceEntry>,
}

// ============================================================================
// DOMRectReadOnly helper
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct DOMRectReadOnly {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl DOMRectReadOnly {
    pub fn to_js_object(&self, context: &mut Context) -> JsResult<JsObject> {
        let obj = ObjectInitializer::new(context)
            .property(js_string!("x"), self.x, Attribute::all())
            .property(js_string!("y"), self.y, Attribute::all())
            .property(js_string!("width"), self.width, Attribute::all())
            .property(js_string!("height"), self.height, Attribute::all())
            .property(js_string!("top"), self.y, Attribute::all())
            .property(js_string!("right"), self.x + self.width, Attribute::all())
            .property(js_string!("bottom"), self.y + self.height, Attribute::all())
            .property(js_string!("left"), self.x, Attribute::all())
            .build();

        let to_json = move |_: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
        };
        obj.set(
            js_string!("toJSON"),
            NativeFunction::from_copy_closure(to_json).to_js_function(context.realm()),
            false,
            context
        )?;

        Ok(obj)
    }
}

// ============================================================================
// Global Observer Registry
// ============================================================================

thread_local! {
    static MUTATION_OBSERVERS: RefCell<HashMap<u32, Rc<RefCell<MutationObserverState>>>> = RefCell::new(HashMap::new());
    static INTERSECTION_OBSERVERS: RefCell<HashMap<u32, Rc<RefCell<IntersectionObserverState>>>> = RefCell::new(HashMap::new());
    static RESIZE_OBSERVERS: RefCell<HashMap<u32, Rc<RefCell<ResizeObserverState>>>> = RefCell::new(HashMap::new());
    static PERFORMANCE_OBSERVERS: RefCell<HashMap<u32, Rc<RefCell<PerformanceObserverState>>>> = RefCell::new(HashMap::new());
    static OBSERVER_ID_COUNTER: RefCell<u32> = RefCell::new(1);
    static PERFORMANCE_ENTRIES: RefCell<Vec<PerformanceEntry>> = RefCell::new(Vec::new());
}

fn get_next_observer_id() -> u32 {
    OBSERVER_ID_COUNTER.with(|counter| {
        let mut c = counter.borrow_mut();
        let id = *c;
        *c += 1;
        id
    })
}

// ============================================================================
// MutationObserver Implementation
// ============================================================================

pub fn register_mutation_observer(context: &mut Context) -> JsResult<()> {
    let mutation_observer_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            let callback = args.get_or_undefined(0);
            if !callback.is_callable() {
                return Err(JsNativeError::typ()
                    .with_message("MutationObserver callback must be a function")
                    .into());
            }

            let callback_obj = callback.as_object().unwrap().clone();
            let observer_id = get_next_observer_id();

            let state = Rc::new(RefCell::new(MutationObserverState {
                callback: callback_obj,
                targets: Vec::new(),
                records: Vec::new(),
                is_connected: false,
            }));

            MUTATION_OBSERVERS.with(|observers| {
                observers.borrow_mut().insert(observer_id, state);
            });

            create_mutation_observer_object(observer_id, ctx)
        }
    );

    let ctor = FunctionObjectBuilder::new(context.realm(), mutation_observer_constructor)
        .name(js_string!("MutationObserver"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(
        js_string!("MutationObserver"),
        ctor,
        false,
        context
    )?;

    Ok(())
}

fn create_mutation_observer_object(observer_id: u32, context: &mut Context) -> JsResult<JsValue> {
    let obj = ObjectInitializer::new(context)
        .property(js_string!("_observerId"), observer_id, Attribute::empty())
        .build();

    // observe(target, options)
    let id = observer_id;
    let observe = move |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let target = args.get_or_undefined(0);
        if !target.is_object() {
            return Err(JsNativeError::typ()
                .with_message("MutationObserver.observe: target must be an object")
                .into());
        }

        let target_obj = target.as_object().unwrap().clone();
        let options_val = args.get_or_undefined(1);

        let options = if options_val.is_object() {
            MutationObserverInit::from_js_object(&options_val.as_object().unwrap(), ctx)?
        } else {
            MutationObserverInit::default()
        };

        // Validate options
        if !options.child_list && !options.attributes && !options.character_data {
            return Err(JsNativeError::typ()
                .with_message("MutationObserver.observe: at least one of childList, attributes, or characterData must be true")
                .into());
        }

        MUTATION_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id) {
                let mut state = state.borrow_mut();
                state.targets.push(ObserverTarget {
                    target: target_obj,
                    options,
                });
                state.is_connected = true;
            }
        });

        Ok(JsValue::undefined())
    };

    // disconnect()
    let id2 = observer_id;
    let disconnect = move |_: &JsValue, _: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
        MUTATION_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id2) {
                let mut state = state.borrow_mut();
                state.targets.clear();
                state.records.clear();
                state.is_connected = false;
            }
        });
        Ok(JsValue::undefined())
    };

    // takeRecords()
    let id3 = observer_id;
    let take_records = move |_: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let records = MUTATION_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id3) {
                let mut state = state.borrow_mut();
                let records = std::mem::take(&mut state.records);
                records
            } else {
                Vec::new()
            }
        });

        let arr = boa_engine::object::builtins::JsArray::new(ctx);
        for (i, record) in records.iter().enumerate() {
            let record_obj = record.to_js_object(ctx)?;
            arr.set(i as u32, record_obj, false, ctx)?;
        }

        Ok(JsValue::from(arr))
    };

    obj.set(
        js_string!("observe"),
        NativeFunction::from_copy_closure(observe).to_js_function(context.realm()),
        false,
        context
    )?;
    obj.set(
        js_string!("disconnect"),
        NativeFunction::from_copy_closure(disconnect).to_js_function(context.realm()),
        false,
        context
    )?;
    obj.set(
        js_string!("takeRecords"),
        NativeFunction::from_copy_closure(take_records).to_js_function(context.realm()),
        false,
        context
    )?;

    Ok(JsValue::from(obj))
}

// Function to queue a mutation record (for use by DOM mutation functions)
pub fn queue_mutation_record(record: MutationRecord) {
    MUTATION_OBSERVERS.with(|observers| {
        for state in observers.borrow().values() {
            let mut state = state.borrow_mut();
            if state.is_connected {
                // Check if any target matches
                for _target in &state.targets {
                    // In real implementation, would check if record.target is target or descendant
                    if state.records.len() < 10000 { // Prevent unbounded growth
                        state.records.push(record.clone());
                        break;
                    }
                }
            }
        }
    });
}

/// Check if there are any pending mutation records to deliver
pub fn has_pending_mutation_records() -> bool {
    MUTATION_OBSERVERS.with(|observers| {
        observers.borrow().values().any(|state| {
            !state.borrow().records.is_empty()
        })
    })
}

/// Deliver pending mutation records to observers
/// This should be called as part of the event loop microtask checkpoint
pub fn deliver_mutation_records(context: &mut Context) {
    // Collect observer IDs and their callbacks/records to avoid borrowing issues
    let pending_deliveries: Vec<(JsObject, Vec<MutationRecord>)> = MUTATION_OBSERVERS.with(|observers| {
        observers.borrow().values().filter_map(|state| {
            let mut state = state.borrow_mut();
            if !state.records.is_empty() {
                let records = std::mem::take(&mut state.records);
                Some((state.callback.clone(), records))
            } else {
                None
            }
        }).collect()
    });

    // Deliver records to each observer
    for (callback, records) in pending_deliveries {
        // Convert records to JS array
        let records_array = match boa_engine::object::builtins::JsArray::new(context) {
            arr => {
                for (i, record) in records.iter().enumerate() {
                    if let Ok(record_obj) = record.to_js_object(context) {
                        let _ = arr.set(i as u32, record_obj, false, context);
                    }
                }
                JsValue::from(arr)
            }
        };

        // Call the callback with the records
        let _ = callback.call(
            &JsValue::undefined(),
            &[records_array],
            context
        );
    }
}

// ============================================================================
// IntersectionObserver Implementation
// ============================================================================

pub fn register_intersection_observer(context: &mut Context) -> JsResult<()> {
    let intersection_observer_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            let callback = args.get_or_undefined(0);
            if !callback.is_callable() {
                return Err(JsNativeError::typ()
                    .with_message("IntersectionObserver callback must be a function")
                    .into());
            }

            let callback_obj = callback.as_object().unwrap().clone();
            let observer_id = get_next_observer_id();

            // Parse options
            let options_val = args.get_or_undefined(1);
            let mut options = IntersectionObserverInit {
                root: None,
                root_margin: "0px".to_string(),
                threshold: vec![0.0],
            };

            if options_val.is_object() {
                let opts = options_val.as_object().unwrap();

                let root_val = opts.get(js_string!("root"), ctx)?;
                if root_val.is_object() {
                    options.root = Some(root_val.as_object().unwrap().clone());
                }

                let margin_val = opts.get(js_string!("rootMargin"), ctx)?;
                if let Some(s) = margin_val.as_string() {
                    options.root_margin = s.to_std_string_escaped();
                }

                let threshold_val = opts.get(js_string!("threshold"), ctx)?;
                if let Some(arr) = threshold_val.as_object() {
                    if arr.is_array() {
                        let len = arr.get(js_string!("length"), ctx)?.to_u32(ctx)?;
                        options.threshold = Vec::new();
                        for i in 0..len {
                            let val = arr.get(i, ctx)?.to_number(ctx)?;
                            options.threshold.push(val);
                        }
                    } else if let Ok(num) = threshold_val.to_number(ctx) {
                        options.threshold = vec![num];
                    }
                } else if let Ok(num) = threshold_val.to_number(ctx) {
                    options.threshold = vec![num];
                }
            }

            let state = Rc::new(RefCell::new(IntersectionObserverState {
                callback: callback_obj,
                options,
                targets: Vec::new(),
                entries: Vec::new(),
            }));

            INTERSECTION_OBSERVERS.with(|observers| {
                observers.borrow_mut().insert(observer_id, state);
            });

            create_intersection_observer_object(observer_id, ctx)
        }
    );

    let ctor = FunctionObjectBuilder::new(context.realm(), intersection_observer_constructor)
        .name(js_string!("IntersectionObserver"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(
        js_string!("IntersectionObserver"),
        ctor,
        false,
        context
    )?;

    // IntersectionObserverEntry constructor (for completeness)
    let entry_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            let init = args.get_or_undefined(0);
            if !init.is_object() {
                return Err(JsNativeError::typ()
                    .with_message("IntersectionObserverEntry requires init object")
                    .into());
            }

            let init_obj = init.as_object().unwrap();
            let entry = IntersectionObserverEntry {
                time: init_obj.get(js_string!("time"), ctx)?.to_number(ctx).unwrap_or(0.0),
                root_bounds: None,
                bounding_client_rect: DOMRectReadOnly::default(),
                intersection_rect: DOMRectReadOnly::default(),
                is_intersecting: init_obj.get(js_string!("isIntersecting"), ctx)?.to_boolean(),
                intersection_ratio: init_obj.get(js_string!("intersectionRatio"), ctx)?.to_number(ctx).unwrap_or(0.0),
                target: None,
            };

            entry.to_js_object(ctx).map(|o| JsValue::from(o))
        }
    ).to_js_function(context.realm());

    context.global_object().set(
        js_string!("IntersectionObserverEntry"),
        entry_constructor,
        false,
        context
    )?;

    Ok(())
}

fn create_intersection_observer_object(observer_id: u32, context: &mut Context) -> JsResult<JsValue> {
    // Get options for readonly properties
    let (root, root_margin, threshold) = INTERSECTION_OBSERVERS.with(|observers| {
        let observers = observers.borrow();
        if let Some(state) = observers.get(&observer_id) {
            let state = state.borrow();
            (
                state.options.root.clone(),
                state.options.root_margin.clone(),
                state.options.threshold.clone(),
            )
        } else {
            (None, "0px".to_string(), vec![0.0])
        }
    });

    let threshold_arr = boa_engine::object::builtins::JsArray::new(context);
    for (i, t) in threshold.iter().enumerate() {
        threshold_arr.set(i as u32, *t, false, context)?;
    }

    let obj = ObjectInitializer::new(context)
        .property(js_string!("_observerId"), observer_id, Attribute::empty())
        .property(
            js_string!("root"),
            root.map(|o| JsValue::from(o)).unwrap_or(JsValue::null()),
            Attribute::READONLY
        )
        .property(js_string!("rootMargin"), js_string!(root_margin), Attribute::READONLY)
        .property(js_string!("thresholds"), threshold_arr, Attribute::READONLY)
        .build();

    // observe(target)
    let id = observer_id;
    let observe = move |_: &JsValue, args: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
        let target = args.get_or_undefined(0);
        if !target.is_object() {
            return Err(JsNativeError::typ()
                .with_message("IntersectionObserver.observe: target must be an Element")
                .into());
        }

        let target_obj = target.as_object().unwrap().clone();

        INTERSECTION_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id) {
                let mut state = state.borrow_mut();
                // Check if not already observing
                let already_observing = state.targets.iter().any(|t| {
                    // Simple object identity check
                    std::ptr::eq(t.as_ref(), target_obj.as_ref())
                });
                if !already_observing {
                    state.targets.push(target_obj);
                }
            }
        });

        Ok(JsValue::undefined())
    };

    // unobserve(target)
    let id2 = observer_id;
    let unobserve = move |_: &JsValue, args: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
        let target = args.get_or_undefined(0);
        if target.is_object() {
            let target_obj = target.as_object().unwrap();

            INTERSECTION_OBSERVERS.with(|observers| {
                if let Some(state) = observers.borrow().get(&id2) {
                    let mut state = state.borrow_mut();
                    state.targets.retain(|t| !std::ptr::eq(t.as_ref(), target_obj.as_ref()));
                }
            });
        }
        Ok(JsValue::undefined())
    };

    // disconnect()
    let id3 = observer_id;
    let disconnect = move |_: &JsValue, _: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
        INTERSECTION_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id3) {
                let mut state = state.borrow_mut();
                state.targets.clear();
                state.entries.clear();
            }
        });
        Ok(JsValue::undefined())
    };

    // takeRecords()
    let id4 = observer_id;
    let take_records = move |_: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let entries = INTERSECTION_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id4) {
                let mut state = state.borrow_mut();
                std::mem::take(&mut state.entries)
            } else {
                Vec::new()
            }
        });

        let arr = boa_engine::object::builtins::JsArray::new(ctx);
        for (i, entry) in entries.iter().enumerate() {
            let entry_obj = entry.to_js_object(ctx)?;
            arr.set(i as u32, entry_obj, false, ctx)?;
        }

        Ok(JsValue::from(arr))
    };

    obj.set(
        js_string!("observe"),
        NativeFunction::from_copy_closure(observe).to_js_function(context.realm()),
        false,
        context
    )?;
    obj.set(
        js_string!("unobserve"),
        NativeFunction::from_copy_closure(unobserve).to_js_function(context.realm()),
        false,
        context
    )?;
    obj.set(
        js_string!("disconnect"),
        NativeFunction::from_copy_closure(disconnect).to_js_function(context.realm()),
        false,
        context
    )?;
    obj.set(
        js_string!("takeRecords"),
        NativeFunction::from_copy_closure(take_records).to_js_function(context.realm()),
        false,
        context
    )?;

    Ok(JsValue::from(obj))
}

// ============================================================================
// ResizeObserver Implementation
// ============================================================================

pub fn register_resize_observer(context: &mut Context) -> JsResult<()> {
    let resize_observer_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            let callback = args.get_or_undefined(0);
            if !callback.is_callable() {
                return Err(JsNativeError::typ()
                    .with_message("ResizeObserver callback must be a function")
                    .into());
            }

            let callback_obj = callback.as_object().unwrap().clone();
            let observer_id = get_next_observer_id();

            let state = Rc::new(RefCell::new(ResizeObserverState {
                callback: callback_obj,
                targets: Vec::new(),
                entries: Vec::new(),
            }));

            RESIZE_OBSERVERS.with(|observers| {
                observers.borrow_mut().insert(observer_id, state);
            });

            create_resize_observer_object(observer_id, ctx)
        }
    );

    let ctor = FunctionObjectBuilder::new(context.realm(), resize_observer_constructor)
        .name(js_string!("ResizeObserver"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(
        js_string!("ResizeObserver"),
        ctor,
        false,
        context
    )?;

    // ResizeObserverEntry constructor
    let entry_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            let target = args.get_or_undefined(0);

            let entry = ResizeObserverEntry {
                target: target.as_object().clone(),
                content_rect: DOMRectReadOnly::default(),
                border_box_size: vec![ResizeObserverSize { inline_size: 0.0, block_size: 0.0 }],
                content_box_size: vec![ResizeObserverSize { inline_size: 0.0, block_size: 0.0 }],
                device_pixel_content_box_size: vec![ResizeObserverSize { inline_size: 0.0, block_size: 0.0 }],
            };

            entry.to_js_object(ctx).map(|o| JsValue::from(o))
        }
    ).to_js_function(context.realm());

    context.global_object().set(
        js_string!("ResizeObserverEntry"),
        entry_constructor,
        false,
        context
    )?;

    // ResizeObserverSize constructor - creates size objects with inlineSize and blockSize
    let size_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            // Handle undefined/missing args - default to 0.0
            let inline_size = if args.len() > 0 && !args[0].is_undefined() && !args[0].is_null() {
                args[0].to_number(ctx).unwrap_or(0.0)
            } else {
                0.0
            };
            let block_size = if args.len() > 1 && !args[1].is_undefined() && !args[1].is_null() {
                args[1].to_number(ctx).unwrap_or(0.0)
            } else {
                0.0
            };

            let size = ResizeObserverSize { inline_size, block_size };
            size.to_js_object(ctx).map(|o| JsValue::from(o))
        }
    );

    let size_ctor = FunctionObjectBuilder::new(context.realm(), size_constructor)
        .name(js_string!("ResizeObserverSize"))
        .length(2)
        .constructor(true)
        .build();

    context.global_object().set(
        js_string!("ResizeObserverSize"),
        size_ctor,
        false,
        context
    )?;

    Ok(())
}

fn create_resize_observer_object(observer_id: u32, context: &mut Context) -> JsResult<JsValue> {
    let obj = ObjectInitializer::new(context)
        .property(js_string!("_observerId"), observer_id, Attribute::empty())
        .build();

    // observe(target, options?)
    let id = observer_id;
    let observe = move |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let target = args.get_or_undefined(0);
        if !target.is_object() {
            return Err(JsNativeError::typ()
                .with_message("ResizeObserver.observe: target must be an Element")
                .into());
        }

        let target_obj = target.as_object().unwrap().clone();

        let options_val = args.get_or_undefined(1);
        let options = if options_val.is_object() {
            let opts = options_val.as_object().unwrap();
            let box_val = opts.get(js_string!("box"), ctx)?;
            ResizeObserverOptions {
                box_type: box_val.as_string()
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|| "content-box".to_string()),
            }
        } else {
            ResizeObserverOptions::default()
        };

        RESIZE_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id) {
                let mut state = state.borrow_mut();
                // Check if not already observing
                let already_observing = state.targets.iter().any(|(t, _)| {
                    std::ptr::eq(t.as_ref(), target_obj.as_ref())
                });
                if !already_observing {
                    state.targets.push((target_obj, options));
                }
            }
        });

        Ok(JsValue::undefined())
    };

    // unobserve(target)
    let id2 = observer_id;
    let unobserve = move |_: &JsValue, args: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
        let target = args.get_or_undefined(0);
        if target.is_object() {
            let target_obj = target.as_object().unwrap();

            RESIZE_OBSERVERS.with(|observers| {
                if let Some(state) = observers.borrow().get(&id2) {
                    let mut state = state.borrow_mut();
                    state.targets.retain(|(t, _)| !std::ptr::eq(t.as_ref(), target_obj.as_ref()));
                }
            });
        }
        Ok(JsValue::undefined())
    };

    // disconnect()
    let id3 = observer_id;
    let disconnect = move |_: &JsValue, _: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
        RESIZE_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id3) {
                let mut state = state.borrow_mut();
                state.targets.clear();
                state.entries.clear();
            }
        });
        Ok(JsValue::undefined())
    };

    obj.set(
        js_string!("observe"),
        NativeFunction::from_copy_closure(observe).to_js_function(context.realm()),
        false,
        context
    )?;
    obj.set(
        js_string!("unobserve"),
        NativeFunction::from_copy_closure(unobserve).to_js_function(context.realm()),
        false,
        context
    )?;
    obj.set(
        js_string!("disconnect"),
        NativeFunction::from_copy_closure(disconnect).to_js_function(context.realm()),
        false,
        context
    )?;

    Ok(JsValue::from(obj))
}

// ============================================================================
// PerformanceObserver Implementation
// ============================================================================

pub fn register_performance_observer(context: &mut Context) -> JsResult<()> {
    let performance_observer_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            let callback = args.get_or_undefined(0);
            if !callback.is_callable() {
                return Err(JsNativeError::typ()
                    .with_message("PerformanceObserver callback must be a function")
                    .into());
            }

            let callback_obj = callback.as_object().unwrap().clone();
            let observer_id = get_next_observer_id();

            let state = Rc::new(RefCell::new(PerformanceObserverState {
                callback: callback_obj,
                options: None,
                entries: Vec::new(),
            }));

            PERFORMANCE_OBSERVERS.with(|observers| {
                observers.borrow_mut().insert(observer_id, state);
            });

            create_performance_observer_object(observer_id, ctx)
        }
    );

    let ctor = FunctionObjectBuilder::new(context.realm(), performance_observer_constructor)
        .name(js_string!("PerformanceObserver"))
        .length(1)
        .constructor(true)
        .build();

    // Register constructor
    context.global_object().set(
        js_string!("PerformanceObserver"),
        ctor,
        false,
        context
    )?;

    // Static supportedEntryTypes property on the constructor
    let supported_types = boa_engine::object::builtins::JsArray::new(context);
    let entry_types = [
        "element", "event", "first-input", "largest-contentful-paint",
        "layout-shift", "longtask", "mark", "measure", "navigation",
        "paint", "resource", "visibility-state"
    ];
    for (i, t) in entry_types.iter().enumerate() {
        supported_types.set(i as u32, js_string!(*t), false, context)?;
    }

    // Get the constructor back and add the static property
    let constructor_val = context.global_object().get(js_string!("PerformanceObserver"), context)?;
    if let Some(constructor_obj) = constructor_val.as_object() {
        constructor_obj.set(
            js_string!("supportedEntryTypes"),
            supported_types,
            false,
            context
        )?;
    }

    // PerformanceEntry constructor
    let entry_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, _: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
            Err(JsNativeError::typ()
                .with_message("PerformanceEntry cannot be constructed directly")
                .into())
        }
    ).to_js_function(context.realm());

    context.global_object().set(
        js_string!("PerformanceEntry"),
        entry_constructor,
        false,
        context
    )?;

    // PerformanceObserverEntryList - not constructible, just for type checking
    let entry_list_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, _: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
            Err(JsNativeError::typ()
                .with_message("PerformanceObserverEntryList cannot be constructed directly")
                .into())
        }
    ).to_js_function(context.realm());

    context.global_object().set(
        js_string!("PerformanceObserverEntryList"),
        entry_list_constructor,
        false,
        context
    )?;

    Ok(())
}

fn create_performance_observer_object(observer_id: u32, context: &mut Context) -> JsResult<JsValue> {
    let obj = ObjectInitializer::new(context)
        .property(js_string!("_observerId"), observer_id, Attribute::empty())
        .build();

    // observe(options)
    let id = observer_id;
    let observe = move |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let options_val = args.get_or_undefined(0);
        if !options_val.is_object() {
            return Err(JsNativeError::typ()
                .with_message("PerformanceObserver.observe requires options object")
                .into());
        }

        let opts = options_val.as_object().unwrap();

        let entry_types_val = opts.get(js_string!("entryTypes"), ctx)?;
        let type_val = opts.get(js_string!("type"), ctx)?;
        let buffered = opts.get(js_string!("buffered"), ctx)?.to_boolean();

        let entry_types = if let Some(arr) = entry_types_val.as_object() {
            if arr.is_array() {
                let len = arr.get(js_string!("length"), ctx)?.to_u32(ctx)?;
                let mut types = Vec::new();
                for i in 0..len {
                    let val = arr.get(i, ctx)?;
                    if let Some(s) = val.as_string() {
                        types.push(s.to_std_string_escaped());
                    }
                }
                Some(types)
            } else {
                None
            }
        } else {
            None
        };

        let type_single = type_val.as_string().map(|s| s.to_std_string_escaped());

        if entry_types.is_none() && type_single.is_none() {
            return Err(JsNativeError::typ()
                .with_message("PerformanceObserver.observe requires either entryTypes or type")
                .into());
        }

        let options = PerformanceObserverInit {
            entry_types,
            type_: type_single,
            buffered,
        };

        PERFORMANCE_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id) {
                let mut state = state.borrow_mut();
                state.options = Some(options);
            }
        });

        Ok(JsValue::undefined())
    };

    // disconnect()
    let id2 = observer_id;
    let disconnect = move |_: &JsValue, _: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
        PERFORMANCE_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id2) {
                let mut state = state.borrow_mut();
                state.options = None;
                state.entries.clear();
            }
        });
        Ok(JsValue::undefined())
    };

    // takeRecords()
    let id3 = observer_id;
    let take_records = move |_: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let entries = PERFORMANCE_OBSERVERS.with(|observers| {
            if let Some(state) = observers.borrow().get(&id3) {
                let mut state = state.borrow_mut();
                std::mem::take(&mut state.entries)
            } else {
                Vec::new()
            }
        });

        let entry_list = PerformanceEntryList { entries };
        entry_list.to_js_object(ctx).map(|o| JsValue::from(o))
    };

    obj.set(
        js_string!("observe"),
        NativeFunction::from_copy_closure(observe).to_js_function(context.realm()),
        false,
        context
    )?;
    obj.set(
        js_string!("disconnect"),
        NativeFunction::from_copy_closure(disconnect).to_js_function(context.realm()),
        false,
        context
    )?;
    obj.set(
        js_string!("takeRecords"),
        NativeFunction::from_copy_closure(take_records).to_js_function(context.realm()),
        false,
        context
    )?;

    Ok(JsValue::from(obj))
}

// ============================================================================
// Performance Entry Type Constructors
// ============================================================================

/// Register PerformanceNavigationTiming constructor
fn register_performance_navigation_timing(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let elapsed = crate::PERFORMANCE_ORIGIN.elapsed();
        let start_time = elapsed.as_secs_f64() * 1000.0;

        let entry = ObjectInitializer::new(ctx)
            // PerformanceEntry properties
            .property(js_string!("name"), js_string!(""), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("navigation"), Attribute::READONLY)
            .property(js_string!("startTime"), 0.0, Attribute::READONLY)
            .property(js_string!("duration"), start_time, Attribute::READONLY)
            // PerformanceResourceTiming properties
            .property(js_string!("initiatorType"), js_string!("navigation"), Attribute::READONLY)
            .property(js_string!("nextHopProtocol"), js_string!("h2"), Attribute::READONLY)
            .property(js_string!("workerStart"), 0.0, Attribute::READONLY)
            .property(js_string!("redirectStart"), 0.0, Attribute::READONLY)
            .property(js_string!("redirectEnd"), 0.0, Attribute::READONLY)
            .property(js_string!("fetchStart"), 0.0, Attribute::READONLY)
            .property(js_string!("domainLookupStart"), 0.0, Attribute::READONLY)
            .property(js_string!("domainLookupEnd"), 0.0, Attribute::READONLY)
            .property(js_string!("connectStart"), 0.0, Attribute::READONLY)
            .property(js_string!("connectEnd"), 0.0, Attribute::READONLY)
            .property(js_string!("secureConnectionStart"), 0.0, Attribute::READONLY)
            .property(js_string!("requestStart"), 0.0, Attribute::READONLY)
            .property(js_string!("responseStart"), 0.0, Attribute::READONLY)
            .property(js_string!("responseEnd"), start_time * 0.1, Attribute::READONLY)
            .property(js_string!("transferSize"), 0, Attribute::READONLY)
            .property(js_string!("encodedBodySize"), 0, Attribute::READONLY)
            .property(js_string!("decodedBodySize"), 0, Attribute::READONLY)
            // PerformanceNavigationTiming specific
            .property(js_string!("type"), js_string!("navigate"), Attribute::READONLY)
            .property(js_string!("redirectCount"), 0, Attribute::READONLY)
            .property(js_string!("unloadEventStart"), 0.0, Attribute::READONLY)
            .property(js_string!("unloadEventEnd"), 0.0, Attribute::READONLY)
            .property(js_string!("domInteractive"), start_time * 0.3, Attribute::READONLY)
            .property(js_string!("domContentLoadedEventStart"), start_time * 0.4, Attribute::READONLY)
            .property(js_string!("domContentLoadedEventEnd"), start_time * 0.5, Attribute::READONLY)
            .property(js_string!("domComplete"), start_time * 0.8, Attribute::READONLY)
            .property(js_string!("loadEventStart"), start_time * 0.9, Attribute::READONLY)
            .property(js_string!("loadEventEnd"), start_time, Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("PerformanceNavigationTiming"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("PerformanceNavigationTiming"), ctor, false, context)?;
    Ok(())
}

/// Register PerformanceResourceTiming constructor
fn register_performance_resource_timing(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let elapsed = crate::PERFORMANCE_ORIGIN.elapsed();
        let start_time = elapsed.as_secs_f64() * 1000.0;

        // Create serverTiming array before ObjectInitializer
        let server_timing = boa_engine::object::builtins::JsArray::new(ctx);

        let entry = ObjectInitializer::new(ctx)
            // PerformanceEntry properties
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("resource"), Attribute::READONLY)
            .property(js_string!("startTime"), start_time, Attribute::READONLY)
            .property(js_string!("duration"), 0.0, Attribute::READONLY)
            // PerformanceResourceTiming specific
            .property(js_string!("initiatorType"), js_string!("fetch"), Attribute::READONLY)
            .property(js_string!("nextHopProtocol"), js_string!("h2"), Attribute::READONLY)
            .property(js_string!("workerStart"), 0.0, Attribute::READONLY)
            .property(js_string!("redirectStart"), 0.0, Attribute::READONLY)
            .property(js_string!("redirectEnd"), 0.0, Attribute::READONLY)
            .property(js_string!("fetchStart"), start_time, Attribute::READONLY)
            .property(js_string!("domainLookupStart"), start_time, Attribute::READONLY)
            .property(js_string!("domainLookupEnd"), start_time, Attribute::READONLY)
            .property(js_string!("connectStart"), start_time, Attribute::READONLY)
            .property(js_string!("connectEnd"), start_time, Attribute::READONLY)
            .property(js_string!("secureConnectionStart"), start_time, Attribute::READONLY)
            .property(js_string!("requestStart"), start_time, Attribute::READONLY)
            .property(js_string!("responseStart"), start_time, Attribute::READONLY)
            .property(js_string!("responseEnd"), start_time, Attribute::READONLY)
            .property(js_string!("transferSize"), 0, Attribute::READONLY)
            .property(js_string!("encodedBodySize"), 0, Attribute::READONLY)
            .property(js_string!("decodedBodySize"), 0, Attribute::READONLY)
            .property(js_string!("serverTiming"), JsValue::from(server_timing), Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("PerformanceResourceTiming"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("PerformanceResourceTiming"), ctor, false, context)?;
    Ok(())
}

/// Register PerformanceMark constructor
fn register_performance_mark(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "mark".to_string());

        let options = args.get_or_undefined(1);
        let detail = if let Some(obj) = options.as_object() {
            obj.get(js_string!("detail"), ctx).unwrap_or(JsValue::null())
        } else {
            JsValue::null()
        };

        let start_time = if let Some(obj) = options.as_object() {
            obj.get(js_string!("startTime"), ctx)
                .ok()
                .and_then(|v| v.as_number())
                .unwrap_or_else(|| {
                    crate::PERFORMANCE_ORIGIN.elapsed().as_secs_f64() * 1000.0
                })
        } else {
            crate::PERFORMANCE_ORIGIN.elapsed().as_secs_f64() * 1000.0
        };

        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name.clone()), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("mark"), Attribute::READONLY)
            .property(js_string!("startTime"), start_time, Attribute::READONLY)
            .property(js_string!("duration"), 0.0, Attribute::READONLY)
            .property(js_string!("detail"), detail, Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        // Also add to performance entries
        PERFORMANCE_ENTRIES.with(|e| {
            e.borrow_mut().push(PerformanceEntry {
                name,
                entry_type: "mark".to_string(),
                start_time,
                duration: 0.0,
                detail: None,
            });
        });

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("PerformanceMark"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("PerformanceMark"), ctor, false, context)?;
    Ok(())
}

/// Register PerformanceMeasure constructor
fn register_performance_measure(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "measure".to_string());

        let options = args.get_or_undefined(1);
        let (start_time, duration, detail) = if let Some(obj) = options.as_object() {
            let start = obj.get(js_string!("start"), ctx)
                .ok()
                .and_then(|v| v.as_number())
                .unwrap_or(0.0);
            let end = obj.get(js_string!("end"), ctx)
                .ok()
                .and_then(|v| v.as_number())
                .unwrap_or_else(|| crate::PERFORMANCE_ORIGIN.elapsed().as_secs_f64() * 1000.0);
            let dur = obj.get(js_string!("duration"), ctx)
                .ok()
                .and_then(|v| v.as_number())
                .unwrap_or(end - start);
            let det = obj.get(js_string!("detail"), ctx).unwrap_or(JsValue::null());
            (start, dur, det)
        } else {
            (0.0, 0.0, JsValue::null())
        };

        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name.clone()), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("measure"), Attribute::READONLY)
            .property(js_string!("startTime"), start_time, Attribute::READONLY)
            .property(js_string!("duration"), duration, Attribute::READONLY)
            .property(js_string!("detail"), detail, Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        // Also add to performance entries
        PERFORMANCE_ENTRIES.with(|e| {
            e.borrow_mut().push(PerformanceEntry {
                name,
                entry_type: "measure".to_string(),
                start_time,
                duration,
                detail: None,
            });
        });

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("PerformanceMeasure"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("PerformanceMeasure"), ctor, false, context)?;
    Ok(())
}

/// Register PerformancePaintTiming constructor
fn register_performance_paint_timing(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "first-paint".to_string());

        let start_time = args.get_or_undefined(1)
            .as_number()
            .unwrap_or_else(|| crate::PERFORMANCE_ORIGIN.elapsed().as_secs_f64() * 1000.0);

        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("paint"), Attribute::READONLY)
            .property(js_string!("startTime"), start_time, Attribute::READONLY)
            .property(js_string!("duration"), 0.0, Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("PerformancePaintTiming"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("PerformancePaintTiming"), ctor, false, context)?;
    Ok(())
}

/// Register LargestContentfulPaint constructor
fn register_largest_contentful_paint(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let start_time = crate::PERFORMANCE_ORIGIN.elapsed().as_secs_f64() * 1000.0;

        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(""), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("largest-contentful-paint"), Attribute::READONLY)
            .property(js_string!("startTime"), start_time, Attribute::READONLY)
            .property(js_string!("duration"), 0.0, Attribute::READONLY)
            .property(js_string!("renderTime"), start_time, Attribute::READONLY)
            .property(js_string!("loadTime"), start_time, Attribute::READONLY)
            .property(js_string!("size"), 0, Attribute::READONLY)
            .property(js_string!("id"), js_string!(""), Attribute::READONLY)
            .property(js_string!("url"), js_string!(""), Attribute::READONLY)
            .property(js_string!("element"), JsValue::null(), Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("LargestContentfulPaint"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("LargestContentfulPaint"), ctor, false, context)?;
    Ok(())
}

/// Register PerformanceEventTiming constructor
fn register_performance_event_timing(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "click".to_string());

        let start_time = crate::PERFORMANCE_ORIGIN.elapsed().as_secs_f64() * 1000.0;

        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("event"), Attribute::READONLY)
            .property(js_string!("startTime"), start_time, Attribute::READONLY)
            .property(js_string!("duration"), 0.0, Attribute::READONLY)
            .property(js_string!("processingStart"), start_time, Attribute::READONLY)
            .property(js_string!("processingEnd"), start_time, Attribute::READONLY)
            .property(js_string!("cancelable"), true, Attribute::READONLY)
            .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("interactionId"), 0, Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("PerformanceEventTiming"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("PerformanceEventTiming"), ctor, false, context)?;
    Ok(())
}

/// Register PerformanceLongTaskTiming constructor
fn register_performance_long_task_timing(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let start_time = crate::PERFORMANCE_ORIGIN.elapsed().as_secs_f64() * 1000.0;

        // Create attribution array
        let attribution = boa_engine::object::builtins::JsArray::new(ctx);
        let task_attribution = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!("unknown"), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("taskattribution"), Attribute::READONLY)
            .property(js_string!("startTime"), 0.0, Attribute::READONLY)
            .property(js_string!("duration"), 0.0, Attribute::READONLY)
            .property(js_string!("containerType"), js_string!("window"), Attribute::READONLY)
            .property(js_string!("containerSrc"), js_string!(""), Attribute::READONLY)
            .property(js_string!("containerId"), js_string!(""), Attribute::READONLY)
            .property(js_string!("containerName"), js_string!(""), Attribute::READONLY)
            .build();
        attribution.push(JsValue::from(task_attribution), ctx)?;

        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!("self"), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("longtask"), Attribute::READONLY)
            .property(js_string!("startTime"), start_time, Attribute::READONLY)
            .property(js_string!("duration"), 50.0, Attribute::READONLY)
            .property(js_string!("attribution"), JsValue::from(attribution), Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("PerformanceLongTaskTiming"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("PerformanceLongTaskTiming"), ctor, false, context)?;
    Ok(())
}

/// Register LayoutShift constructor
fn register_layout_shift(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let start_time = crate::PERFORMANCE_ORIGIN.elapsed().as_secs_f64() * 1000.0;

        // Create sources array before ObjectInitializer
        let sources = boa_engine::object::builtins::JsArray::new(ctx);

        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(""), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("layout-shift"), Attribute::READONLY)
            .property(js_string!("startTime"), start_time, Attribute::READONLY)
            .property(js_string!("duration"), 0.0, Attribute::READONLY)
            .property(js_string!("value"), 0.0, Attribute::READONLY)
            .property(js_string!("hadRecentInput"), false, Attribute::READONLY)
            .property(js_string!("lastInputTime"), 0.0, Attribute::READONLY)
            .property(js_string!("sources"), JsValue::from(sources), Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("LayoutShift"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("LayoutShift"), ctor, false, context)?;
    Ok(())
}

/// Register VisibilityStateEntry constructor
fn register_visibility_state_entry(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "visible".to_string());

        let start_time = crate::PERFORMANCE_ORIGIN.elapsed().as_secs_f64() * 1000.0;

        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("visibility-state"), Attribute::READONLY)
            .property(js_string!("startTime"), start_time, Attribute::READONLY)
            .property(js_string!("duration"), 0.0, Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("VisibilityStateEntry"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("VisibilityStateEntry"), ctor, false, context)?;
    Ok(())
}

/// Register TaskAttributionTiming constructor
fn register_task_attribution_timing(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!("unknown"), Attribute::READONLY)
            .property(js_string!("entryType"), js_string!("taskattribution"), Attribute::READONLY)
            .property(js_string!("startTime"), 0.0, Attribute::READONLY)
            .property(js_string!("duration"), 0.0, Attribute::READONLY)
            .property(js_string!("containerType"), js_string!("window"), Attribute::READONLY)
            .property(js_string!("containerSrc"), js_string!(""), Attribute::READONLY)
            .property(js_string!("containerId"), js_string!(""), Attribute::READONLY)
            .property(js_string!("containerName"), js_string!(""), Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("TaskAttributionTiming"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("TaskAttributionTiming"), ctor, false, context)?;
    Ok(())
}

/// Register PerformanceServerTiming constructor
fn register_performance_server_timing(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|_| "server".to_string());

        let entry = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("duration"), 0.0, Attribute::READONLY)
            .property(js_string!("description"), js_string!(""), Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| {
                    Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
                }),
                js_string!("toJSON"),
                0,
            )
            .build();

        Ok(JsValue::from(entry))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("PerformanceServerTiming"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(js_string!("PerformanceServerTiming"), ctor, false, context)?;
    Ok(())
}

/// Register all performance entry type constructors
fn register_performance_entry_types(context: &mut Context) -> JsResult<()> {
    register_performance_navigation_timing(context)?;
    register_performance_resource_timing(context)?;
    register_performance_mark(context)?;
    register_performance_measure(context)?;
    register_performance_paint_timing(context)?;
    register_largest_contentful_paint(context)?;
    register_performance_event_timing(context)?;
    register_performance_long_task_timing(context)?;
    register_layout_shift(context)?;
    register_visibility_state_entry(context)?;
    register_task_attribution_timing(context)?;
    register_performance_server_timing(context)?;
    Ok(())
}

// ============================================================================
// ReportingObserver Implementation
// ============================================================================

pub fn register_reporting_observer(context: &mut Context) -> JsResult<()> {
    let reporting_observer_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
            let callback = args.get_or_undefined(0);
            if !callback.is_callable() {
                return Err(JsNativeError::typ()
                    .with_message("ReportingObserver callback must be a function")
                    .into());
            }

            let obj = ObjectInitializer::new(ctx)
                .property(js_string!("_callback"), callback.clone(), Attribute::empty())
                .property(js_string!("_observing"), false, Attribute::WRITABLE)
                .build();

            // observe()
            let observe = |this: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
                if let Some(obj) = this.as_object() {
                    obj.set(js_string!("_observing"), true, false, ctx)?;
                }
                Ok(JsValue::undefined())
            };

            // disconnect()
            let disconnect = |this: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
                if let Some(obj) = this.as_object() {
                    obj.set(js_string!("_observing"), false, false, ctx)?;
                }
                Ok(JsValue::undefined())
            };

            // takeRecords()
            let take_records = |_: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
                Ok(JsValue::from(boa_engine::object::builtins::JsArray::new(ctx)))
            };

            obj.set(
                js_string!("observe"),
                NativeFunction::from_copy_closure(observe).to_js_function(ctx.realm()),
                false,
                ctx
            )?;
            obj.set(
                js_string!("disconnect"),
                NativeFunction::from_copy_closure(disconnect).to_js_function(ctx.realm()),
                false,
                ctx
            )?;
            obj.set(
                js_string!("takeRecords"),
                NativeFunction::from_copy_closure(take_records).to_js_function(ctx.realm()),
                false,
                ctx
            )?;

            Ok(JsValue::from(obj))
        }
    );

    let ctor = FunctionObjectBuilder::new(context.realm(), reporting_observer_constructor)
        .name(js_string!("ReportingObserver"))
        .length(1)
        .constructor(true)
        .build();

    context.global_object().set(
        js_string!("ReportingObserver"),
        ctor,
        false,
        context
    )?;

    Ok(())
}

// ============================================================================
// Performance API Extensions
// ============================================================================

pub fn register_performance_api(context: &mut Context) -> JsResult<()> {
    // Create performance object
    let start_time = std::time::Instant::now();

    let now = move |_: &JsValue, _: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
        let elapsed = start_time.elapsed();
        let ms = elapsed.as_secs_f64() * 1000.0;
        Ok(JsValue::from(ms))
    };

    let time_origin = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64;

    let get_entries = |_: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let entries = PERFORMANCE_ENTRIES.with(|e| e.borrow().clone());
        let arr = boa_engine::object::builtins::JsArray::new(ctx);
        for (i, entry) in entries.iter().enumerate() {
            let entry_obj = entry.to_js_object(ctx)?;
            arr.set(i as u32, entry_obj, false, ctx)?;
        }
        Ok(JsValue::from(arr))
    };

    let get_entries_by_type = |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let entry_type = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let entries = PERFORMANCE_ENTRIES.with(|e| {
            e.borrow()
                .iter()
                .filter(|entry| entry.entry_type == entry_type)
                .cloned()
                .collect::<Vec<_>>()
        });

        let arr = boa_engine::object::builtins::JsArray::new(ctx);
        for (i, entry) in entries.iter().enumerate() {
            let entry_obj = entry.to_js_object(ctx)?;
            arr.set(i as u32, entry_obj, false, ctx)?;
        }
        Ok(JsValue::from(arr))
    };

    let get_entries_by_name = |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let name = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let entry_type = args.get(1)
            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
            .transpose()?;

        let entries = PERFORMANCE_ENTRIES.with(|e| {
            e.borrow()
                .iter()
                .filter(|entry| {
                    entry.name == name && entry_type.as_ref().map_or(true, |t| &entry.entry_type == t)
                })
                .cloned()
                .collect::<Vec<_>>()
        });

        let arr = boa_engine::object::builtins::JsArray::new(ctx);
        for (i, entry) in entries.iter().enumerate() {
            let entry_obj = entry.to_js_object(ctx)?;
            arr.set(i as u32, entry_obj, false, ctx)?;
        }
        Ok(JsValue::from(arr))
    };

    let mark_start = start_time;
    let mark = move |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let name = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let options = args.get(1);
        let detail = options.and_then(|o| {
            if o.is_object() {
                o.as_object().and_then(|obj| {
                    obj.get(js_string!("detail"), ctx).ok()
                })
            } else {
                None
            }
        });

        let elapsed = mark_start.elapsed();
        let start_time_ms = elapsed.as_secs_f64() * 1000.0;

        let entry = PerformanceEntry {
            name: name.clone(),
            entry_type: "mark".to_string(),
            start_time: start_time_ms,
            duration: 0.0,
            detail,
        };

        PERFORMANCE_ENTRIES.with(|e| {
            e.borrow_mut().push(entry.clone());
        });

        entry.to_js_object(ctx).map(|o| JsValue::from(o))
    };

    let measure_start = start_time;
    let measure = move |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let name = args.get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();

        let elapsed = measure_start.elapsed();
        let current_time = elapsed.as_secs_f64() * 1000.0;

        // Parse start and end marks or options
        let (start_time_ms, end_time_ms, detail) = if args.len() > 1 {
            let arg1 = args.get_or_undefined(1);
            if arg1.is_object() && !arg1.as_object().unwrap().is_callable() {
                // MeasureOptions
                let opts = arg1.as_object().unwrap();
                let start_val = opts.get(js_string!("start"), ctx)?;
                let end_val = opts.get(js_string!("end"), ctx)?;
                let duration_val = opts.get(js_string!("duration"), ctx)?;
                let detail_val = opts.get(js_string!("detail"), ctx).ok();

                let start = if start_val.is_string() {
                    // Look up mark
                    let mark_name = start_val.to_string(ctx)?.to_std_string_escaped();
                    PERFORMANCE_ENTRIES.with(|e| {
                        e.borrow().iter()
                            .filter(|e| e.name == mark_name && e.entry_type == "mark")
                            .last()
                            .map(|e| e.start_time)
                            .unwrap_or(0.0)
                    })
                } else if let Ok(n) = start_val.to_number(ctx) {
                    n
                } else {
                    0.0
                };

                let end = if end_val.is_string() {
                    let mark_name = end_val.to_string(ctx)?.to_std_string_escaped();
                    PERFORMANCE_ENTRIES.with(|e| {
                        e.borrow().iter()
                            .filter(|e| e.name == mark_name && e.entry_type == "mark")
                            .last()
                            .map(|e| e.start_time)
                            .unwrap_or(current_time)
                    })
                } else if let Ok(n) = end_val.to_number(ctx) {
                    n
                } else if let Ok(d) = duration_val.to_number(ctx) {
                    start + d
                } else {
                    current_time
                };

                (start, end, detail_val)
            } else {
                // Legacy: measure(name, startMark, endMark)
                let start_mark = arg1.to_string(ctx)?.to_std_string_escaped();
                let start = PERFORMANCE_ENTRIES.with(|e| {
                    e.borrow().iter()
                        .filter(|e| e.name == start_mark && e.entry_type == "mark")
                        .last()
                        .map(|e| e.start_time)
                        .unwrap_or(0.0)
                });

                let end = if let Some(end_arg) = args.get(2) {
                    let end_mark = end_arg.to_string(ctx)?.to_std_string_escaped();
                    PERFORMANCE_ENTRIES.with(|e| {
                        e.borrow().iter()
                            .filter(|e| e.name == end_mark && e.entry_type == "mark")
                            .last()
                            .map(|e| e.start_time)
                            .unwrap_or(current_time)
                    })
                } else {
                    current_time
                };

                (start, end, None)
            }
        } else {
            (0.0, current_time, None)
        };

        let entry = PerformanceEntry {
            name: name.clone(),
            entry_type: "measure".to_string(),
            start_time: start_time_ms,
            duration: end_time_ms - start_time_ms,
            detail,
        };

        PERFORMANCE_ENTRIES.with(|e| {
            e.borrow_mut().push(entry.clone());
        });

        entry.to_js_object(ctx).map(|o| JsValue::from(o))
    };

    let clear_marks = |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let name = args.get(0)
            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
            .transpose()?;

        PERFORMANCE_ENTRIES.with(|e| {
            let mut entries = e.borrow_mut();
            if let Some(name) = name {
                entries.retain(|entry| !(entry.entry_type == "mark" && entry.name == name));
            } else {
                entries.retain(|entry| entry.entry_type != "mark");
            }
        });

        Ok(JsValue::undefined())
    };

    let clear_measures = |_: &JsValue, args: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let name = args.get(0)
            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
            .transpose()?;

        PERFORMANCE_ENTRIES.with(|e| {
            let mut entries = e.borrow_mut();
            if let Some(name) = name {
                entries.retain(|entry| !(entry.entry_type == "measure" && entry.name == name));
            } else {
                entries.retain(|entry| entry.entry_type != "measure");
            }
        });

        Ok(JsValue::undefined())
    };

    let clear_resource_timings = |_: &JsValue, _: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
        PERFORMANCE_ENTRIES.with(|e| {
            e.borrow_mut().retain(|entry| entry.entry_type != "resource");
        });
        Ok(JsValue::undefined())
    };

    let set_resource_timing_buffer_size = |_: &JsValue, _: &[JsValue], _: &mut Context| -> JsResult<JsValue> {
        // No-op for now
        Ok(JsValue::undefined())
    };

    let to_json = move |_: &JsValue, _: &[JsValue], ctx: &mut Context| -> JsResult<JsValue> {
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("timeOrigin"), time_origin, Attribute::all())
            .build();
        Ok(JsValue::from(obj))
    };

    let performance = ObjectInitializer::new(context)
        .property(js_string!("timeOrigin"), time_origin, Attribute::READONLY)
        .function(
            NativeFunction::from_copy_closure(now),
            js_string!("now"),
            0
        )
        .function(
            NativeFunction::from_copy_closure(get_entries),
            js_string!("getEntries"),
            0
        )
        .function(
            NativeFunction::from_copy_closure(get_entries_by_type),
            js_string!("getEntriesByType"),
            1
        )
        .function(
            NativeFunction::from_copy_closure(get_entries_by_name),
            js_string!("getEntriesByName"),
            1
        )
        .function(
            NativeFunction::from_copy_closure(mark),
            js_string!("mark"),
            1
        )
        .function(
            NativeFunction::from_copy_closure(measure),
            js_string!("measure"),
            1
        )
        .function(
            NativeFunction::from_copy_closure(clear_marks),
            js_string!("clearMarks"),
            0
        )
        .function(
            NativeFunction::from_copy_closure(clear_measures),
            js_string!("clearMeasures"),
            0
        )
        .function(
            NativeFunction::from_copy_closure(clear_resource_timings),
            js_string!("clearResourceTimings"),
            0
        )
        .function(
            NativeFunction::from_copy_closure(set_resource_timing_buffer_size),
            js_string!("setResourceTimingBufferSize"),
            1
        )
        .function(
            NativeFunction::from_copy_closure(to_json),
            js_string!("toJSON"),
            0
        )
        .build();

    // Add navigation timing (PerformanceNavigationTiming)
    let navigation = ObjectInitializer::new(context)
        .property(js_string!("type"), js_string!("navigate"), Attribute::READONLY)
        .property(js_string!("redirectCount"), 0, Attribute::READONLY)
        .build();

    performance.set(js_string!("navigation"), navigation, false, context)?;

    // Add memory info (non-standard but widely used)
    let memory = ObjectInitializer::new(context)
        .property(js_string!("jsHeapSizeLimit"), 2147483648_i64, Attribute::READONLY)
        .property(js_string!("totalJSHeapSize"), 50000000_i64, Attribute::READONLY)
        .property(js_string!("usedJSHeapSize"), 25000000_i64, Attribute::READONLY)
        .build();

    performance.set(js_string!("memory"), memory, false, context)?;

    // Add timing (deprecated but still used)
    let timing_origin = time_origin as i64;
    let timing = ObjectInitializer::new(context)
        .property(js_string!("navigationStart"), timing_origin, Attribute::READONLY)
        .property(js_string!("unloadEventStart"), 0, Attribute::READONLY)
        .property(js_string!("unloadEventEnd"), 0, Attribute::READONLY)
        .property(js_string!("redirectStart"), 0, Attribute::READONLY)
        .property(js_string!("redirectEnd"), 0, Attribute::READONLY)
        .property(js_string!("fetchStart"), timing_origin, Attribute::READONLY)
        .property(js_string!("domainLookupStart"), timing_origin, Attribute::READONLY)
        .property(js_string!("domainLookupEnd"), timing_origin, Attribute::READONLY)
        .property(js_string!("connectStart"), timing_origin, Attribute::READONLY)
        .property(js_string!("connectEnd"), timing_origin, Attribute::READONLY)
        .property(js_string!("secureConnectionStart"), timing_origin, Attribute::READONLY)
        .property(js_string!("requestStart"), timing_origin, Attribute::READONLY)
        .property(js_string!("responseStart"), timing_origin, Attribute::READONLY)
        .property(js_string!("responseEnd"), timing_origin, Attribute::READONLY)
        .property(js_string!("domLoading"), timing_origin, Attribute::READONLY)
        .property(js_string!("domInteractive"), timing_origin, Attribute::READONLY)
        .property(js_string!("domContentLoadedEventStart"), timing_origin, Attribute::READONLY)
        .property(js_string!("domContentLoadedEventEnd"), timing_origin, Attribute::READONLY)
        .property(js_string!("domComplete"), timing_origin, Attribute::READONLY)
        .property(js_string!("loadEventStart"), timing_origin, Attribute::READONLY)
        .property(js_string!("loadEventEnd"), timing_origin, Attribute::READONLY)
        .build();

    performance.set(js_string!("timing"), timing, false, context)?;

    context.global_object().set(
        js_string!("performance"),
        performance,
        false,
        context
    )?;

    Ok(())
}

// ============================================================================
// Main Registration Function
// ============================================================================

pub fn register_all_observers(context: &mut Context) -> JsResult<()> {
    register_mutation_observer(context)?;
    register_intersection_observer(context)?;
    register_resize_observer(context)?;
    register_performance_observer(context)?;
    register_reporting_observer(context)?;
    register_performance_api(context)?;
    register_performance_entry_types(context)?;

    Ok(())
}
