//! Form APIs - FormData, validation, constraint validation
//!
//! Implements:
//! - FormData constructor and methods
//! - ValidityState interface
//! - Constraint validation (checkValidity, reportValidity, setCustomValidity)

use boa_engine::{
    Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue,
    NativeFunction, js_string, object::ObjectInitializer, object::builtins::JsArray,
    object::FunctionObjectBuilder, property::Attribute,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    /// Storage for FormData instances
    static ref FORM_DATA_STORAGE: Arc<Mutex<HashMap<u32, FormDataStore>>> =
        Arc::new(Mutex::new(HashMap::new()));

    /// Counter for FormData IDs
    static ref FORM_DATA_COUNTER: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));

    /// Validity state storage
    static ref VALIDITY_STORAGE: Arc<Mutex<HashMap<String, ValidityState>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

/// FormData storage structure
#[derive(Debug, Clone)]
struct FormDataStore {
    entries: Vec<(String, FormDataValue)>,
}

/// FormData value (string or file-like)
#[derive(Debug, Clone)]
enum FormDataValue {
    String(String),
    File { name: String, content: Vec<u8>, mime_type: String },
}

impl FormDataStore {
    fn new() -> Self {
        Self { entries: Vec::new() }
    }

    fn append(&mut self, name: String, value: FormDataValue) {
        self.entries.push((name, value));
    }

    fn set(&mut self, name: String, value: FormDataValue) {
        // Remove all existing entries with this name
        self.entries.retain(|(n, _)| n != &name);
        self.entries.push((name, value));
    }

    fn get(&self, name: &str) -> Option<&FormDataValue> {
        self.entries.iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
    }

    fn get_all(&self, name: &str) -> Vec<&FormDataValue> {
        self.entries.iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v)
            .collect()
    }

    fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|(n, _)| n == name)
    }

    fn delete(&mut self, name: &str) {
        self.entries.retain(|(n, _)| n != name);
    }

    fn keys(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        self.entries.iter()
            .filter_map(|(n, _)| {
                if seen.insert(n.clone()) {
                    Some(n.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn values(&self) -> Vec<&FormDataValue> {
        self.entries.iter().map(|(_, v)| v).collect()
    }
}

/// ValidityState structure
#[derive(Debug, Clone, Default)]
struct ValidityState {
    value_missing: bool,
    type_mismatch: bool,
    pattern_mismatch: bool,
    too_long: bool,
    too_short: bool,
    range_underflow: bool,
    range_overflow: bool,
    step_mismatch: bool,
    bad_input: bool,
    custom_error: bool,
    custom_message: String,
}

impl ValidityState {
    fn is_valid(&self) -> bool {
        !self.value_missing && !self.type_mismatch && !self.pattern_mismatch &&
        !self.too_long && !self.too_short && !self.range_underflow &&
        !self.range_overflow && !self.step_mismatch && !self.bad_input && !self.custom_error
    }
}

/// Register all form-related APIs
pub fn register_all_form_apis(context: &mut Context) -> JsResult<()> {
    register_form_data(context)?;
    register_validity_state(context)?;
    register_url_search_params_form_data(context)?;
    Ok(())
}

/// Internal property key for storing FormData ID
const FORM_DATA_ID_KEY: &str = "__formDataId__";

/// Helper to get FormData ID from this object
fn get_form_data_id(this: &JsValue, ctx: &mut Context) -> Option<u32> {
    this.as_object()
        .and_then(|obj| obj.get(js_string!(FORM_DATA_ID_KEY), ctx).ok())
        .and_then(|v| v.to_u32(ctx).ok())
}

/// Register FormData constructor and prototype
fn register_form_data(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        let mut id = FORM_DATA_COUNTER.lock().unwrap();
        *id += 1;
        let form_data_id = *id;
        drop(id);

        let mut store = FormDataStore::new();

        // If form element passed, extract form field values
        if let Some(form) = args.get_or_undefined(0).as_object() {
            // Try to extract form elements
            if let Ok(elements) = form.get(js_string!("elements"), ctx) {
                if let Some(els) = elements.as_object() {
                    if let Ok(length) = els.get(js_string!("length"), ctx) {
                        let len = length.to_u32(ctx).unwrap_or(0);
                        for i in 0..len {
                            if let Ok(el) = els.get(i, ctx) {
                                if let Some(element) = el.as_object() {
                                    // Skip disabled elements
                                    let disabled = element.get(js_string!("disabled"), ctx)
                                        .ok()
                                        .map(|v| v.to_boolean())
                                        .unwrap_or(false);
                                    if disabled {
                                        continue;
                                    }

                                    let name = element.get(js_string!("name"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_string(ctx).ok())
                                        .map(|s| s.to_std_string_escaped())
                                        .unwrap_or_default();

                                    // Skip elements without name
                                    if name.is_empty() {
                                        continue;
                                    }

                                    // Get element type
                                    let element_type = element.get(js_string!("type"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_string(ctx).ok())
                                        .map(|s| s.to_std_string_escaped().to_lowercase())
                                        .unwrap_or_default();

                                    let tag_name = element.get(js_string!("tagName"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_string(ctx).ok())
                                        .map(|s| s.to_std_string_escaped().to_uppercase())
                                        .unwrap_or_default();

                                    // Handle checkboxes and radio buttons
                                    if element_type == "checkbox" || element_type == "radio" {
                                        let checked = element.get(js_string!("checked"), ctx)
                                            .ok()
                                            .map(|v| v.to_boolean())
                                            .unwrap_or(false);
                                        if !checked {
                                            continue; // Skip unchecked checkboxes/radios
                                        }
                                    }

                                    // Skip file inputs (handle separately if needed)
                                    if element_type == "file" {
                                        continue;
                                    }

                                    // Skip submit and reset buttons
                                    if element_type == "submit" || element_type == "reset" || element_type == "button" {
                                        continue;
                                    }

                                    // Handle select elements
                                    if tag_name == "SELECT" {
                                        // Get selected option(s)
                                        let multiple = element.get(js_string!("multiple"), ctx)
                                            .ok()
                                            .map(|v| v.to_boolean())
                                            .unwrap_or(false);

                                        if let Ok(options) = element.get(js_string!("options"), ctx) {
                                            if let Some(opts) = options.as_object() {
                                                if let Ok(opt_len) = opts.get(js_string!("length"), ctx) {
                                                    let opt_len = opt_len.to_u32(ctx).unwrap_or(0);
                                                    for j in 0..opt_len {
                                                        if let Ok(opt) = opts.get(j, ctx) {
                                                            if let Some(opt_obj) = opt.as_object() {
                                                                let selected = opt_obj.get(js_string!("selected"), ctx)
                                                                    .ok()
                                                                    .map(|v| v.to_boolean())
                                                                    .unwrap_or(false);
                                                                if selected {
                                                                    let value = opt_obj.get(js_string!("value"), ctx)
                                                                        .ok()
                                                                        .and_then(|v| v.to_string(ctx).ok())
                                                                        .map(|s| s.to_std_string_escaped())
                                                                        .unwrap_or_default();
                                                                    store.append(name.clone(), FormDataValue::String(value));
                                                                    if !multiple {
                                                                        break;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        continue;
                                    }

                                    // Regular input/textarea - get value
                                    let value = element.get(js_string!("value"), ctx)
                                        .ok()
                                        .and_then(|v| v.to_string(ctx).ok())
                                        .map(|s| s.to_std_string_escaped())
                                        .unwrap_or_default();

                                    store.append(name, FormDataValue::String(value));
                                }
                            }
                        }
                    }
                }
            }
        }

        FORM_DATA_STORAGE.lock().unwrap().insert(form_data_id, store);

        // Get the this object (created by new) or create a new object
        let result_obj = if let Some(this_obj) = this.as_object() {
            // Use the this object created by new
            this_obj.set(js_string!(FORM_DATA_ID_KEY), JsValue::from(form_data_id), false, ctx)?;
            setup_form_data_methods_with_id(&this_obj, ctx, form_data_id)?;
            JsValue::from(this_obj.clone())
        } else {
            // Create a new object if this is not set (fallback)
            let new_obj = JsObject::with_null_proto();
            new_obj.set(js_string!(FORM_DATA_ID_KEY), JsValue::from(form_data_id), false, ctx)?;
            setup_form_data_methods_with_id(&new_obj, ctx, form_data_id)?;
            JsValue::from(new_obj)
        };

        Ok(result_obj)
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("FormData"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("FormData"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register FormData: {}", e)))?;

    Ok(())
}

/// Setup FormData methods on an existing object with embedded ID
fn setup_form_data_methods_with_id(obj: &JsObject, context: &mut Context, id: u32) -> JsResult<()> {
    // append(name, value, filename?) - ID embedded in closure
    let id_append = id;
    let append = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let value = args.get_or_undefined(1);

        let form_value = if let Some(obj) = value.as_object() {
            if let Ok(name_val) = obj.get(js_string!("name"), ctx) {
                let filename = args.get(2)
                    .map(|v| v.to_string(ctx).ok())
                    .flatten()
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|| name_val.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default());
                FormDataValue::File {
                    name: filename,
                    content: Vec::new(),
                    mime_type: "application/octet-stream".to_string(),
                }
            } else {
                FormDataValue::String(value.to_string(ctx)?.to_std_string_escaped())
            }
        } else {
            FormDataValue::String(value.to_string(ctx)?.to_std_string_escaped())
        };

        if let Ok(mut storage) = FORM_DATA_STORAGE.lock() {
            if let Some(store) = storage.get_mut(&id_append) {
                store.append(name, form_value);
            }
        }
        Ok(JsValue::undefined())
    });

    // delete(name) - ID embedded in closure
    let id_delete = id;
    let delete = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        if let Ok(mut storage) = FORM_DATA_STORAGE.lock() {
            if let Some(store) = storage.get_mut(&id_delete) {
                store.delete(&name);
            }
        }
        Ok(JsValue::undefined())
    });

    // get(name) - ID embedded in closure
    let id_get = id;
    let get = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        if let Ok(storage) = FORM_DATA_STORAGE.lock() {
            if let Some(store) = storage.get(&id_get) {
                if let Some(value) = store.get(&name) {
                    return match value {
                        FormDataValue::String(s) => Ok(JsValue::from(js_string!(s.as_str()))),
                        FormDataValue::File { name, .. } => Ok(JsValue::from(js_string!(name.as_str()))),
                    };
                }
            }
        }
        Ok(JsValue::null())
    });

    // getAll(name) - ID embedded in closure
    let id_get_all = id;
    let get_all = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let array = JsArray::new(ctx);
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        if let Ok(storage) = FORM_DATA_STORAGE.lock() {
            if let Some(store) = storage.get(&id_get_all) {
                for value in store.get_all(&name) {
                    let js_val = match value {
                        FormDataValue::String(s) => JsValue::from(js_string!(s.as_str())),
                        FormDataValue::File { name, .. } => JsValue::from(js_string!(name.as_str())),
                    };
                    array.push(js_val, ctx)?;
                }
            }
        }
        Ok(JsValue::from(array))
    });

    // has(name) - ID embedded in closure
    let id_has = id;
    let has = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        if let Ok(storage) = FORM_DATA_STORAGE.lock() {
            if let Some(store) = storage.get(&id_has) {
                return Ok(JsValue::from(store.has(&name)));
            }
        }
        Ok(JsValue::from(false))
    });

    // set(name, value, filename?) - ID embedded in closure
    let id_set = id;
    let set = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let value = args.get_or_undefined(1);

        let form_value = if let Some(obj) = value.as_object() {
            if let Ok(name_val) = obj.get(js_string!("name"), ctx) {
                let filename = args.get(2)
                    .map(|v| v.to_string(ctx).ok())
                    .flatten()
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|| name_val.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default());
                FormDataValue::File {
                    name: filename,
                    content: Vec::new(),
                    mime_type: "application/octet-stream".to_string(),
                }
            } else {
                FormDataValue::String(value.to_string(ctx)?.to_std_string_escaped())
            }
        } else {
            FormDataValue::String(value.to_string(ctx)?.to_std_string_escaped())
        };

        if let Ok(mut storage) = FORM_DATA_STORAGE.lock() {
            if let Some(store) = storage.get_mut(&id_set) {
                store.set(name, form_value);
            }
        }
        Ok(JsValue::undefined())
    });

    // keys() - ID embedded in closure
    let id_keys = id;
    let keys = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let array = JsArray::new(ctx);
        if let Ok(storage) = FORM_DATA_STORAGE.lock() {
            if let Some(store) = storage.get(&id_keys) {
                for key in store.keys() {
                    array.push(JsValue::from(js_string!(key.as_str())), ctx)?;
                }
            }
        }
        Ok(JsValue::from(array))
    });

    // values() - ID embedded in closure
    let id_values = id;
    let values = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let array = JsArray::new(ctx);
        if let Ok(storage) = FORM_DATA_STORAGE.lock() {
            if let Some(store) = storage.get(&id_values) {
                for value in store.values() {
                    let js_val = match value {
                        FormDataValue::String(s) => JsValue::from(js_string!(s.as_str())),
                        FormDataValue::File { name, .. } => JsValue::from(js_string!(name.as_str())),
                    };
                    array.push(js_val, ctx)?;
                }
            }
        }
        Ok(JsValue::from(array))
    });

    // entries() - ID embedded in closure
    let id_entries = id;
    let entries = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let array = JsArray::new(ctx);
        if let Ok(storage) = FORM_DATA_STORAGE.lock() {
            if let Some(store) = storage.get(&id_entries) {
                for (name, value) in &store.entries {
                    let entry = JsArray::new(ctx);
                    entry.push(JsValue::from(js_string!(name.as_str())), ctx)?;
                    let js_val = match value {
                        FormDataValue::String(s) => JsValue::from(js_string!(s.as_str())),
                        FormDataValue::File { name, .. } => JsValue::from(js_string!(name.as_str())),
                    };
                    entry.push(js_val, ctx)?;
                    array.push(JsValue::from(entry), ctx)?;
                }
            }
        }
        Ok(JsValue::from(array))
    });

    // forEach(callback) - ID embedded in closure
    let id_foreach = id;
    let foreach = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if let Some(cb) = callback.as_callable() {
            // Clone entries to avoid holding lock during callback
            let entries_clone: Vec<(String, FormDataValue)> = {
                if let Ok(storage) = FORM_DATA_STORAGE.lock() {
                    if let Some(store) = storage.get(&id_foreach) {
                        store.entries.clone()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            };
            for (name, value) in entries_clone {
                let js_val = match &value {
                    FormDataValue::String(s) => JsValue::from(js_string!(s.as_str())),
                    FormDataValue::File { name, .. } => JsValue::from(js_string!(name.as_str())),
                };
                let _ = cb.call(&JsValue::undefined(), &[
                    js_val,
                    JsValue::from(js_string!(name.as_str())),
                ], ctx);
            }
        }
        Ok(JsValue::undefined())
    });

    // Set methods on the object
    let append_fn = append.to_js_function(context.realm());
    let delete_fn = delete.to_js_function(context.realm());
    let get_fn = get.to_js_function(context.realm());
    let get_all_fn = get_all.to_js_function(context.realm());
    let has_fn = has.to_js_function(context.realm());
    let set_fn = set.to_js_function(context.realm());
    let keys_fn = keys.to_js_function(context.realm());
    let values_fn = values.to_js_function(context.realm());
    let entries_fn = entries.to_js_function(context.realm());
    let foreach_fn = foreach.to_js_function(context.realm());

    obj.set(js_string!("append"), JsValue::from(append_fn), false, context)?;
    obj.set(js_string!("delete"), JsValue::from(delete_fn), false, context)?;
    obj.set(js_string!("get"), JsValue::from(get_fn), false, context)?;
    obj.set(js_string!("getAll"), JsValue::from(get_all_fn), false, context)?;
    obj.set(js_string!("has"), JsValue::from(has_fn), false, context)?;
    obj.set(js_string!("set"), JsValue::from(set_fn), false, context)?;
    obj.set(js_string!("keys"), JsValue::from(keys_fn), false, context)?;
    obj.set(js_string!("values"), JsValue::from(values_fn), false, context)?;
    obj.set(js_string!("entries"), JsValue::from(entries_fn), false, context)?;
    obj.set(js_string!("forEach"), JsValue::from(foreach_fn), false, context)?;

    Ok(())
}

/// Register ValidityState and constraint validation APIs
fn register_validity_state(context: &mut Context) -> JsResult<()> {
    // ValidityState constructor (usually created internally)
    let validity_state_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let validity = create_validity_state_object(ctx, ValidityState::default())?;
        Ok(JsValue::from(validity))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), validity_state_fn)
        .name(js_string!("ValidityState"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("ValidityState"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register ValidityState: {}", e)))?;

    Ok(())
}

/// Create ValidityState object
fn create_validity_state_object(context: &mut Context, state: ValidityState) -> JsResult<JsObject> {
    let valid = state.is_valid();

    let obj = ObjectInitializer::new(context)
        .property(js_string!("valid"), JsValue::from(valid), Attribute::all())
        .property(js_string!("valueMissing"), JsValue::from(state.value_missing), Attribute::all())
        .property(js_string!("typeMismatch"), JsValue::from(state.type_mismatch), Attribute::all())
        .property(js_string!("patternMismatch"), JsValue::from(state.pattern_mismatch), Attribute::all())
        .property(js_string!("tooLong"), JsValue::from(state.too_long), Attribute::all())
        .property(js_string!("tooShort"), JsValue::from(state.too_short), Attribute::all())
        .property(js_string!("rangeUnderflow"), JsValue::from(state.range_underflow), Attribute::all())
        .property(js_string!("rangeOverflow"), JsValue::from(state.range_overflow), Attribute::all())
        .property(js_string!("stepMismatch"), JsValue::from(state.step_mismatch), Attribute::all())
        .property(js_string!("badInput"), JsValue::from(state.bad_input), Attribute::all())
        .property(js_string!("customError"), JsValue::from(state.custom_error), Attribute::all())
        .build();

    Ok(obj)
}

/// Register URLSearchParams FormData integration
fn register_url_search_params_form_data(context: &mut Context) -> JsResult<()> {
    // Already have URLSearchParams, but add FormData constructor support
    Ok(())
}

/// Create validation methods for form elements
pub fn create_form_element_validation_methods(context: &mut Context) -> JsResult<(JsValue, JsValue, JsValue, JsValue)> {
    // checkValidity()
    let check_validity = NativeFunction::from_copy_closure(|this, _args, ctx| {
        // Get required attribute
        let required = this.as_object()
            .and_then(|obj| obj.get(js_string!("required"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);

        // Get value
        let value = this.as_object()
            .and_then(|obj| obj.get(js_string!("value"), ctx).ok())
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        // Basic validation: if required and empty, invalid
        let valid = !(required && value.is_empty());

        Ok(JsValue::from(valid))
    }).to_js_function(context.realm());

    // reportValidity()
    let report_validity = NativeFunction::from_copy_closure(|this, _args, ctx| {
        let required = this.as_object()
            .and_then(|obj| obj.get(js_string!("required"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);

        let value = this.as_object()
            .and_then(|obj| obj.get(js_string!("value"), ctx).ok())
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let valid = !(required && value.is_empty());

        // In a real browser, this would show a validation message
        // We just return the validity
        Ok(JsValue::from(valid))
    }).to_js_function(context.realm());

    // setCustomValidity(message)
    let set_custom_validity = NativeFunction::from_copy_closure(|this, args, ctx| {
        let message = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        if let Some(obj) = this.as_object() {
            let _ = obj.set(js_string!("validationMessage"), JsValue::from(js_string!(message.as_str())), false, ctx);
        }

        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    // validity getter
    let validity = NativeFunction::from_copy_closure(|this, _args, ctx| {
        let required = this.as_object()
            .and_then(|obj| obj.get(js_string!("required"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);

        let value = this.as_object()
            .and_then(|obj| obj.get(js_string!("value"), ctx).ok())
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let state = ValidityState {
            value_missing: required && value.is_empty(),
            ..Default::default()
        };

        let validity_obj = create_validity_state_object(ctx, state)?;
        Ok(JsValue::from(validity_obj))
    }).to_js_function(context.realm());

    Ok((
        JsValue::from(check_validity),
        JsValue::from(report_validity),
        JsValue::from(set_custom_validity),
        JsValue::from(validity),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;

    fn create_test_context() -> Context {
        let mut ctx = Context::default();
        register_all_form_apis(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn test_form_data_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof FormData === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_form_data_append_get() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            var fd = new FormData();
            fd.append('name', 'value');
            fd.get('name') === 'value'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_form_data_has() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            var fd = new FormData();
            fd.append('key', 'val');
            fd.has('key') && !fd.has('missing')
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_form_data_delete() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            var fd = new FormData();
            fd.append('key', 'val');
            fd.delete('key');
            !fd.has('key')
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_validity_state_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof ValidityState === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }
}
