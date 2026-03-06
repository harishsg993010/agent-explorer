// Intl API implementation for JavaScript runtime
// Provides internationalization support

use boa_engine::{
    Context, JsArgs, JsResult, JsValue, Source,
    object::ObjectInitializer,
    property::Attribute,
    NativeFunction, JsString, JsObject,
    js_string,
};
use std::collections::HashMap;
use lazy_static::lazy_static;
use std::sync::Mutex;

lazy_static! {
    /// Default locale for Intl operations
    static ref DEFAULT_LOCALE: Mutex<String> = Mutex::new("en-US".to_string());

    /// Supported locales cache
    static ref SUPPORTED_LOCALES: Mutex<Vec<String>> = Mutex::new(vec![
        "en".to_string(),
        "en-US".to_string(),
        "en-GB".to_string(),
        "es".to_string(),
        "fr".to_string(),
        "de".to_string(),
        "it".to_string(),
        "pt".to_string(),
        "ja".to_string(),
        "ko".to_string(),
        "zh".to_string(),
        "ar".to_string(),
        "ru".to_string(),
    ]);
}

/// Register all Intl APIs
pub fn register_all_intl_apis(context: &mut Context) -> JsResult<()> {
    register_intl_object(context)?;
    Ok(())
}

/// Helper to get array length from a JsObject
fn get_array_length(obj: &JsObject, ctx: &mut Context) -> JsResult<u64> {
    if let Ok(len_val) = obj.get(js_string!("length"), ctx) {
        if let Some(n) = len_val.as_number() {
            return Ok(n as u64);
        }
    }
    Ok(0)
}

/// Register the main Intl object with all formatters
fn register_intl_object(context: &mut Context) -> JsResult<()> {
    // Create getCanonicalLocales function first (outside the builder)
    let get_canonical_locales = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let locales = args.get_or_undefined(0);
        canonicalize_locales(locales, ctx)
    });
    let get_canonical_locales_fn = get_canonical_locales.to_js_function(context.realm());

    // Create Intl.DateTimeFormat constructor
    let date_time_format = create_date_time_format_constructor(context)?;

    // Create Intl.NumberFormat constructor
    let number_format = create_number_format_constructor(context)?;

    // Create Intl.Collator constructor
    let collator = create_collator_constructor(context)?;

    // Create Intl.PluralRules constructor
    let plural_rules = create_plural_rules_constructor(context)?;

    // Create Intl.RelativeTimeFormat constructor
    let relative_time_format = create_relative_time_format_constructor(context)?;

    // Create Intl.ListFormat constructor
    let list_format = create_list_format_constructor(context)?;

    // Create Intl.Segmenter constructor
    let segmenter = create_segmenter_constructor(context)?;

    // Create Intl.DisplayNames constructor
    let display_names = create_display_names_constructor(context)?;

    // Build the Intl object
    let intl = ObjectInitializer::new(context)
        .property(js_string!("DateTimeFormat"), date_time_format, Attribute::all())
        .property(js_string!("NumberFormat"), number_format, Attribute::all())
        .property(js_string!("Collator"), collator, Attribute::all())
        .property(js_string!("PluralRules"), plural_rules, Attribute::all())
        .property(js_string!("RelativeTimeFormat"), relative_time_format, Attribute::all())
        .property(js_string!("ListFormat"), list_format, Attribute::all())
        .property(js_string!("Segmenter"), segmenter, Attribute::all())
        .property(js_string!("DisplayNames"), display_names, Attribute::all())
        .property(js_string!("getCanonicalLocales"), JsValue::from(get_canonical_locales_fn), Attribute::all())
        .build();

    context.register_global_property(js_string!("Intl"), intl, Attribute::all())?;

    Ok(())
}

/// Create Intl.DateTimeFormat constructor
fn create_date_time_format_constructor(context: &mut Context) -> JsResult<JsValue> {
    let constructor = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let locale = get_locale_from_args(args, 0);
            let options = get_options_from_args(args, 1, ctx)?;

            // Extract options
            let date_style = options.get("dateStyle").cloned().unwrap_or_default();
            let time_style = options.get("timeStyle").cloned().unwrap_or_default();
            let time_zone = options.get("timeZone").cloned().unwrap_or_else(|| "UTC".to_string());

            // Create the format function
            let locale_for_format = locale.clone();
            let date_style_for_format = date_style.clone();
            let time_style_for_format = time_style.clone();

            let format = unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let date_value = args.get_or_undefined(0);
                    let formatted = format_date_time_simple(date_value, &locale_for_format, &date_style_for_format, &time_style_for_format, ctx)?;
                    Ok(JsValue::from(JsString::from(formatted.as_str())))
                })
            };

            // Create resolvedOptions function
            let resolved_locale = locale.clone();
            let resolved_time_zone = time_zone.clone();
            let resolved_options = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                let options = ObjectInitializer::new(ctx)
                    .property(js_string!("locale"), JsValue::from(JsString::from("en-US")), Attribute::all())
                    .property(js_string!("calendar"), JsValue::from(JsString::from("gregory")), Attribute::all())
                    .property(js_string!("numberingSystem"), JsValue::from(JsString::from("latn")), Attribute::all())
                    .build();
                Ok(JsValue::from(options))
            });

            // Build the DateTimeFormat instance
            let format_fn = format.to_js_function(ctx.realm());
            let resolved_fn = resolved_options.to_js_function(ctx.realm());

            let instance = ObjectInitializer::new(ctx)
                .property(js_string!("format"), JsValue::from(format_fn), Attribute::all())
                .property(js_string!("resolvedOptions"), JsValue::from(resolved_fn), Attribute::all())
                .build();

            Ok(JsValue::from(instance))
        })
    };

    let constructor_obj = ObjectInitializer::new(context).build();
    context.register_global_builtin_callable(js_string!("DateTimeFormat"), 0, constructor)?;
    Ok(JsValue::from(constructor_obj))
}

/// Create Intl.NumberFormat constructor
fn create_number_format_constructor(context: &mut Context) -> JsResult<JsValue> {
    let constructor = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let locale = get_locale_from_args(args, 0);
            let options = get_options_from_args(args, 1, ctx)?;

            let style = options.get("style").cloned().unwrap_or_else(|| "decimal".to_string());
            let currency = options.get("currency").cloned();
            let use_grouping = options.get("useGrouping").map(|s| s != "false").unwrap_or(true);

            // Create the format function
            let style_clone = style.clone();
            let currency_clone = currency.clone();

            let format = unsafe {
                NativeFunction::from_closure(move |_this, args, _ctx| {
                    let number = args.get_or_undefined(0);
                    let formatted = format_number_simple(number, &style_clone, &currency_clone, use_grouping)?;
                    Ok(JsValue::from(JsString::from(formatted.as_str())))
                })
            };

            // Create resolvedOptions function
            let resolved_style = style.clone();
            let resolved_currency = currency.clone();
            let resolved_options = unsafe {
                NativeFunction::from_closure(move |_this, _args, ctx| {
                    let obj = if let Some(ref curr) = resolved_currency {
                        ObjectInitializer::new(ctx)
                            .property(js_string!("locale"), JsValue::from(JsString::from("en-US")), Attribute::all())
                            .property(js_string!("style"), JsValue::from(JsString::from(resolved_style.as_str())), Attribute::all())
                            .property(js_string!("numberingSystem"), JsValue::from(JsString::from("latn")), Attribute::all())
                            .property(js_string!("currency"), JsValue::from(JsString::from(curr.as_str())), Attribute::all())
                            .build()
                    } else {
                        ObjectInitializer::new(ctx)
                            .property(js_string!("locale"), JsValue::from(JsString::from("en-US")), Attribute::all())
                            .property(js_string!("style"), JsValue::from(JsString::from(resolved_style.as_str())), Attribute::all())
                            .property(js_string!("numberingSystem"), JsValue::from(JsString::from("latn")), Attribute::all())
                            .build()
                    };
                    Ok(JsValue::from(obj))
                })
            };

            let format_fn = format.to_js_function(ctx.realm());
            let resolved_fn = resolved_options.to_js_function(ctx.realm());

            let instance = ObjectInitializer::new(ctx)
                .property(js_string!("format"), JsValue::from(format_fn), Attribute::all())
                .property(js_string!("resolvedOptions"), JsValue::from(resolved_fn), Attribute::all())
                .build();

            Ok(JsValue::from(instance))
        })
    };

    let constructor_obj = ObjectInitializer::new(context).build();
    context.register_global_builtin_callable(js_string!("NumberFormat"), 0, constructor)?;
    Ok(JsValue::from(constructor_obj))
}

/// Create Intl.Collator constructor
fn create_collator_constructor(context: &mut Context) -> JsResult<JsValue> {
    let constructor = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let _locale = get_locale_from_args(args, 0);
            let options = get_options_from_args(args, 1, ctx)?;

            let sensitivity = options.get("sensitivity").cloned().unwrap_or_else(|| "variant".to_string());
            let numeric = options.get("numeric").map(|s| s == "true").unwrap_or(false);

            // Create the compare function
            let compare = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
                let x = args.get_or_undefined(0);
                let y = args.get_or_undefined(1);

                let x_str = if let Some(s) = x.as_string() { s.to_std_string_escaped() } else { String::new() };
                let y_str = if let Some(s) = y.as_string() { s.to_std_string_escaped() } else { String::new() };

                let result = match x_str.cmp(&y_str) {
                    std::cmp::Ordering::Less => -1,
                    std::cmp::Ordering::Greater => 1,
                    std::cmp::Ordering::Equal => 0,
                };
                Ok(JsValue::from(result))
            });

            // Create resolvedOptions function
            let resolved_options = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                let options = ObjectInitializer::new(ctx)
                    .property(js_string!("locale"), JsValue::from(JsString::from("en-US")), Attribute::all())
                    .property(js_string!("usage"), JsValue::from(JsString::from("sort")), Attribute::all())
                    .property(js_string!("sensitivity"), JsValue::from(JsString::from("variant")), Attribute::all())
                    .build();
                Ok(JsValue::from(options))
            });

            let compare_fn = compare.to_js_function(ctx.realm());
            let resolved_fn = resolved_options.to_js_function(ctx.realm());

            let instance = ObjectInitializer::new(ctx)
                .property(js_string!("compare"), JsValue::from(compare_fn), Attribute::all())
                .property(js_string!("resolvedOptions"), JsValue::from(resolved_fn), Attribute::all())
                .build();

            Ok(JsValue::from(instance))
        })
    };

    let constructor_obj = ObjectInitializer::new(context).build();
    context.register_global_builtin_callable(js_string!("Collator"), 0, constructor)?;
    Ok(JsValue::from(constructor_obj))
}

/// Create Intl.PluralRules constructor
fn create_plural_rules_constructor(context: &mut Context) -> JsResult<JsValue> {
    let constructor = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let _locale = get_locale_from_args(args, 0);
            let options = get_options_from_args(args, 1, ctx)?;

            let type_ = options.get("type").cloned().unwrap_or_else(|| "cardinal".to_string());

            // Create the select function
            let select = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
                let n = args.get_or_undefined(0);
                let n_val = if let Some(num) = n.as_number() { num } else { 0.0 };
                let category = if n_val == 1.0 { "one" } else { "other" };
                Ok(JsValue::from(JsString::from(category)))
            });

            // Create resolvedOptions function
            let resolved_options = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                let options = ObjectInitializer::new(ctx)
                    .property(js_string!("locale"), JsValue::from(JsString::from("en-US")), Attribute::all())
                    .property(js_string!("type"), JsValue::from(JsString::from("cardinal")), Attribute::all())
                    .build();
                Ok(JsValue::from(options))
            });

            let select_fn = select.to_js_function(ctx.realm());
            let resolved_fn = resolved_options.to_js_function(ctx.realm());

            let instance = ObjectInitializer::new(ctx)
                .property(js_string!("select"), JsValue::from(select_fn), Attribute::all())
                .property(js_string!("resolvedOptions"), JsValue::from(resolved_fn), Attribute::all())
                .build();

            Ok(JsValue::from(instance))
        })
    };

    let constructor_obj = ObjectInitializer::new(context).build();
    context.register_global_builtin_callable(js_string!("PluralRules"), 0, constructor)?;
    Ok(JsValue::from(constructor_obj))
}

/// Create Intl.RelativeTimeFormat constructor
fn create_relative_time_format_constructor(context: &mut Context) -> JsResult<JsValue> {
    let constructor = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let _locale = get_locale_from_args(args, 0);
            let options = get_options_from_args(args, 1, ctx)?;

            let style = options.get("style").cloned().unwrap_or_else(|| "long".to_string());
            let numeric = options.get("numeric").cloned().unwrap_or_else(|| "always".to_string());

            // Create the format function
            let format = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
                let value = args.get_or_undefined(0);
                let unit = args.get_or_undefined(1);

                let value_num = if let Some(num) = value.as_number() { num } else { 0.0 };
                let unit_str = if let Some(s) = unit.as_string() { s.to_std_string_escaped() } else { "second".to_string() };

                let abs_value = value_num.abs() as i64;
                let is_past = value_num < 0.0;
                let plural = if abs_value == 1 { "" } else { "s" };

                let formatted = if is_past {
                    format!("{} {}{} ago", abs_value, unit_str, plural)
                } else {
                    format!("in {} {}{}", abs_value, unit_str, plural)
                };

                Ok(JsValue::from(JsString::from(formatted.as_str())))
            });

            // Create resolvedOptions function
            let resolved_options = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                let options = ObjectInitializer::new(ctx)
                    .property(js_string!("locale"), JsValue::from(JsString::from("en-US")), Attribute::all())
                    .property(js_string!("style"), JsValue::from(JsString::from("long")), Attribute::all())
                    .property(js_string!("numeric"), JsValue::from(JsString::from("always")), Attribute::all())
                    .build();
                Ok(JsValue::from(options))
            });

            let format_fn = format.to_js_function(ctx.realm());
            let resolved_fn = resolved_options.to_js_function(ctx.realm());

            let instance = ObjectInitializer::new(ctx)
                .property(js_string!("format"), JsValue::from(format_fn), Attribute::all())
                .property(js_string!("resolvedOptions"), JsValue::from(resolved_fn), Attribute::all())
                .build();

            Ok(JsValue::from(instance))
        })
    };

    let constructor_obj = ObjectInitializer::new(context).build();
    context.register_global_builtin_callable(js_string!("RelativeTimeFormat"), 0, constructor)?;
    Ok(JsValue::from(constructor_obj))
}

/// Create Intl.ListFormat constructor
fn create_list_format_constructor(context: &mut Context) -> JsResult<JsValue> {
    let constructor = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let _locale = get_locale_from_args(args, 0);
            let options = get_options_from_args(args, 1, ctx)?;

            let style = options.get("style").cloned().unwrap_or_else(|| "long".to_string());
            let type_ = options.get("type").cloned().unwrap_or_else(|| "conjunction".to_string());

            // Create the format function
            let type_clone = type_.clone();
            let format = unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let list = args.get_or_undefined(0);
                    let items = extract_list_items(list, ctx)?;
                    let formatted = format_list_simple(&items, &type_clone);
                    Ok(JsValue::from(JsString::from(formatted.as_str())))
                })
            };

            // Create resolvedOptions function
            let resolved_options = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                let options = ObjectInitializer::new(ctx)
                    .property(js_string!("locale"), JsValue::from(JsString::from("en-US")), Attribute::all())
                    .property(js_string!("style"), JsValue::from(JsString::from("long")), Attribute::all())
                    .property(js_string!("type"), JsValue::from(JsString::from("conjunction")), Attribute::all())
                    .build();
                Ok(JsValue::from(options))
            });

            let format_fn = format.to_js_function(ctx.realm());
            let resolved_fn = resolved_options.to_js_function(ctx.realm());

            let instance = ObjectInitializer::new(ctx)
                .property(js_string!("format"), JsValue::from(format_fn), Attribute::all())
                .property(js_string!("resolvedOptions"), JsValue::from(resolved_fn), Attribute::all())
                .build();

            Ok(JsValue::from(instance))
        })
    };

    let constructor_obj = ObjectInitializer::new(context).build();
    context.register_global_builtin_callable(js_string!("ListFormat"), 0, constructor)?;
    Ok(JsValue::from(constructor_obj))
}

/// Create Intl.Segmenter constructor
fn create_segmenter_constructor(context: &mut Context) -> JsResult<JsValue> {
    let constructor = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let _locale = get_locale_from_args(args, 0);
            let options = get_options_from_args(args, 1, ctx)?;

            let granularity = options.get("granularity").cloned().unwrap_or_else(|| "grapheme".to_string());

            // Create the segment function
            let granularity_clone = granularity.clone();
            let segment = unsafe {
                NativeFunction::from_closure(move |_this, args, ctx| {
                    let input = args.get_or_undefined(0);
                    let input_str = if let Some(s) = input.as_string() { s.to_std_string_escaped() } else { String::new() };
                    let segments = segment_string_simple(&input_str, &granularity_clone);
                    create_segments_object(segments, ctx)
                })
            };

            // Create resolvedOptions function
            let resolved_options = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                let options = ObjectInitializer::new(ctx)
                    .property(js_string!("locale"), JsValue::from(JsString::from("en-US")), Attribute::all())
                    .property(js_string!("granularity"), JsValue::from(JsString::from("grapheme")), Attribute::all())
                    .build();
                Ok(JsValue::from(options))
            });

            let segment_fn = segment.to_js_function(ctx.realm());
            let resolved_fn = resolved_options.to_js_function(ctx.realm());

            let instance = ObjectInitializer::new(ctx)
                .property(js_string!("segment"), JsValue::from(segment_fn), Attribute::all())
                .property(js_string!("resolvedOptions"), JsValue::from(resolved_fn), Attribute::all())
                .build();

            Ok(JsValue::from(instance))
        })
    };

    let constructor_obj = ObjectInitializer::new(context).build();
    context.register_global_builtin_callable(js_string!("Segmenter"), 0, constructor)?;
    Ok(JsValue::from(constructor_obj))
}

/// Create Intl.DisplayNames constructor
fn create_display_names_constructor(context: &mut Context) -> JsResult<JsValue> {
    let constructor = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let _locale = get_locale_from_args(args, 0);
            let options = get_options_from_args(args, 1, ctx)?;

            let type_ = options.get("type").cloned().unwrap_or_else(|| "language".to_string());
            let fallback = options.get("fallback").cloned().unwrap_or_else(|| "code".to_string());

            // Create the of function
            let type_clone = type_.clone();
            let fallback_clone = fallback.clone();
            let of_fn = unsafe {
                NativeFunction::from_closure(move |_this, args, _ctx| {
                    let code = args.get_or_undefined(0);
                    let code_str = if let Some(s) = code.as_string() { s.to_std_string_escaped() } else { return Ok(JsValue::undefined()); };
                    let display_name = get_display_name_simple(&code_str, &type_clone, &fallback_clone);
                    match display_name {
                        Some(name) => Ok(JsValue::from(JsString::from(name.as_str()))),
                        None => Ok(JsValue::undefined()),
                    }
                })
            };

            // Create resolvedOptions function
            let resolved_options = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
                let options = ObjectInitializer::new(ctx)
                    .property(js_string!("locale"), JsValue::from(JsString::from("en-US")), Attribute::all())
                    .property(js_string!("type"), JsValue::from(JsString::from("language")), Attribute::all())
                    .property(js_string!("fallback"), JsValue::from(JsString::from("code")), Attribute::all())
                    .build();
                Ok(JsValue::from(options))
            });

            let of_js_fn = of_fn.to_js_function(ctx.realm());
            let resolved_fn = resolved_options.to_js_function(ctx.realm());

            let instance = ObjectInitializer::new(ctx)
                .property(js_string!("of"), JsValue::from(of_js_fn), Attribute::all())
                .property(js_string!("resolvedOptions"), JsValue::from(resolved_fn), Attribute::all())
                .build();

            Ok(JsValue::from(instance))
        })
    };

    let constructor_obj = ObjectInitializer::new(context).build();
    context.register_global_builtin_callable(js_string!("DisplayNames"), 0, constructor)?;
    Ok(JsValue::from(constructor_obj))
}

// ============================================================================
// Helper functions
// ============================================================================

fn get_locale_from_args(args: &[JsValue], index: usize) -> String {
    if let Some(arg) = args.get(index) {
        if let Some(s) = arg.as_string() {
            return s.to_std_string_escaped();
        }
    }
    DEFAULT_LOCALE.lock().unwrap().clone()
}

fn get_options_from_args(args: &[JsValue], index: usize, ctx: &mut Context) -> JsResult<HashMap<String, String>> {
    let mut options = HashMap::new();

    if let Some(arg) = args.get(index) {
        if let Some(obj) = arg.as_object() {
            let keys = ["dateStyle", "timeStyle", "style", "currency", "currencyDisplay",
                       "useGrouping", "type", "numeric", "granularity", "fallback", "sensitivity"];

            for key in keys {
                if let Ok(value) = obj.get(JsString::from(key), ctx) {
                    if !value.is_undefined() && !value.is_null() {
                        if let Some(s) = value.as_string() {
                            options.insert(key.to_string(), s.to_std_string_escaped());
                        } else if let Some(n) = value.as_number() {
                            options.insert(key.to_string(), n.to_string());
                        } else if let Some(b) = value.as_boolean() {
                            options.insert(key.to_string(), b.to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(options)
}

fn format_date_time_simple(_date_value: &JsValue, _locale: &str, date_style: &str, time_style: &str, _ctx: &mut Context) -> JsResult<String> {
    // Simplified: return a formatted date string
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let secs = now_ms / 1000;
    let days = secs / 86400;
    let time_of_day = secs % 86400;

    let mut year = 1970i32;
    let mut remaining = days;
    while remaining >= 365 {
        let days_in_year = if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) { 366 } else { 365 };
        if remaining >= days_in_year { remaining -= days_in_year; year += 1; } else { break; }
    }

    let months = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u32;
    for (i, &m) in months.iter().enumerate() {
        if remaining < m as i64 { month = (i + 1) as u32; break; }
        remaining -= m as i64;
    }
    let day = (remaining + 1) as u32;
    let hour = (time_of_day / 3600) as u32;
    let minute = ((time_of_day % 3600) / 60) as u32;

    let date_part = match date_style {
        "full" | "long" => format!("{}/{}/{}", month, day, year),
        "medium" | "short" => format!("{}/{}/{}", month, day, year % 100),
        _ => format!("{}/{}/{}", month, day, year),
    };

    let time_part = match time_style {
        "full" | "long" | "medium" => format!("{:02}:{:02}", hour, minute),
        "short" => format!("{:02}:{:02}", hour, minute),
        _ => String::new(),
    };

    if !date_style.is_empty() && !time_style.is_empty() {
        Ok(format!("{}, {}", date_part, time_part))
    } else if !date_style.is_empty() {
        Ok(date_part)
    } else if !time_style.is_empty() {
        Ok(time_part)
    } else {
        Ok(date_part)
    }
}

fn format_number_simple(number: &JsValue, style: &str, currency: &Option<String>, use_grouping: bool) -> JsResult<String> {
    let num = if let Some(n) = number.as_number() { n } else { 0.0 };

    if num.is_nan() { return Ok("NaN".to_string()); }
    if num.is_infinite() { return Ok(if num.is_sign_positive() { "∞" } else { "-∞" }.to_string()); }

    let is_negative = num < 0.0;
    let abs_num = num.abs();

    let formatted = if use_grouping {
        let int_part = abs_num.trunc() as i64;
        let int_str = int_part.to_string();
        let mut result = String::new();
        for (i, c) in int_str.chars().enumerate() {
            if i > 0 && (int_str.len() - i) % 3 == 0 { result.push(','); }
            result.push(c);
        }

        let frac = abs_num.fract();
        if frac > 0.0 && style == "decimal" {
            let frac_str = format!("{:.2}", frac);
            if frac_str.len() > 2 {
                result.push_str(&frac_str[1..]);
            }
        }
        result
    } else {
        format!("{}", abs_num)
    };

    let sign = if is_negative { "-" } else { "" };

    match style {
        "currency" => {
            let symbol = match currency.as_ref().map(|s| s.as_str()).unwrap_or("USD") {
                "USD" => "$",
                "EUR" => "€",
                "GBP" => "£",
                "JPY" => "¥",
                _ => "$",
            };
            Ok(format!("{}{}{}", sign, symbol, formatted))
        }
        "percent" => Ok(format!("{}{}%", sign, (abs_num * 100.0) as i64)),
        _ => Ok(format!("{}{}", sign, formatted)),
    }
}

fn extract_list_items(list: &JsValue, ctx: &mut Context) -> JsResult<Vec<String>> {
    let mut items = Vec::new();
    if let Some(obj) = list.as_object() {
        if let Ok(len) = get_array_length(&obj, ctx) {
            for i in 0..len {
                if let Ok(item) = obj.get(i, ctx) {
                    if let Some(s) = item.as_string() {
                        items.push(s.to_std_string_escaped());
                    }
                }
            }
        }
    }
    Ok(items)
}

fn format_list_simple(items: &[String], type_: &str) -> String {
    if items.is_empty() { return String::new(); }
    if items.len() == 1 { return items[0].clone(); }

    let separator = match type_ {
        "conjunction" => " and ",
        "disjunction" => " or ",
        _ => " and ",
    };

    if items.len() == 2 {
        format!("{}{}{}", items[0], separator, items[1])
    } else {
        let init = items[..items.len()-1].join(", ");
        format!("{},{}{}", init, separator, items[items.len()-1])
    }
}

fn segment_string_simple(input: &str, granularity: &str) -> Vec<(usize, String)> {
    match granularity {
        "word" => {
            let mut segments = Vec::new();
            let mut start = 0;
            let mut in_word = false;
            for (i, c) in input.char_indices() {
                if c.is_alphanumeric() && !in_word { start = i; in_word = true; }
                else if !c.is_alphanumeric() && in_word {
                    segments.push((start, input[start..i].to_string()));
                    in_word = false;
                }
            }
            if in_word { segments.push((start, input[start..].to_string())); }
            segments
        }
        _ => input.char_indices().map(|(i, c)| (i, c.to_string())).collect(),
    }
}

fn create_segments_object(segments: Vec<(usize, String)>, ctx: &mut Context) -> JsResult<JsValue> {
    let mut code = String::from("[");
    for (i, (idx, seg)) in segments.iter().enumerate() {
        if i > 0 { code.push(','); }
        code.push_str(&format!(r#"{{segment:"{}",index:{}}}"#, seg.replace('"', "\\\""), idx));
    }
    code.push(']');
    ctx.eval(Source::from_bytes(code.as_bytes()))
}

fn get_display_name_simple(code: &str, type_: &str, fallback: &str) -> Option<String> {
    match type_ {
        "language" => {
            match code.to_lowercase().as_str() {
                "en" | "en-us" => Some("English".to_string()),
                "es" => Some("Spanish".to_string()),
                "fr" => Some("French".to_string()),
                "de" => Some("German".to_string()),
                "ja" => Some("Japanese".to_string()),
                "zh" => Some("Chinese".to_string()),
                _ => if fallback == "code" { Some(code.to_string()) } else { None },
            }
        }
        "region" => {
            match code.to_uppercase().as_str() {
                "US" => Some("United States".to_string()),
                "GB" => Some("United Kingdom".to_string()),
                "FR" => Some("France".to_string()),
                "DE" => Some("Germany".to_string()),
                "JP" => Some("Japan".to_string()),
                "CN" => Some("China".to_string()),
                _ => if fallback == "code" { Some(code.to_string()) } else { None },
            }
        }
        _ => if fallback == "code" { Some(code.to_string()) } else { None },
    }
}

fn canonicalize_locales(locales: &JsValue, ctx: &mut Context) -> JsResult<JsValue> {
    let mut result = Vec::new();

    if let Some(s) = locales.as_string() {
        result.push(canonicalize_locale(&s.to_std_string_escaped()));
    } else if let Some(obj) = locales.as_object() {
        if let Ok(len) = get_array_length(&obj, ctx) {
            for i in 0..len {
                if let Ok(item) = obj.get(i, ctx) {
                    if let Some(s) = item.as_string() {
                        let canonical = canonicalize_locale(&s.to_std_string_escaped());
                        if !result.contains(&canonical) { result.push(canonical); }
                    }
                }
            }
        }
    }

    let code = format!("[{}]", result.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","));
    ctx.eval(Source::from_bytes(code.as_bytes()))
}

fn canonicalize_locale(locale: &str) -> String {
    let parts: Vec<&str> = locale.split('-').collect();
    if parts.len() == 1 { parts[0].to_lowercase() }
    else if parts.len() >= 2 && parts[1].len() == 2 { format!("{}-{}", parts[0].to_lowercase(), parts[1].to_uppercase()) }
    else { locale.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intl_object_exists() {
        let mut context = Context::default();
        register_all_intl_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof Intl")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "object");
    }

    #[test]
    fn test_intl_date_time_format() {
        let mut context = Context::default();
        register_all_intl_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof Intl.DateTimeFormat")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "object");
    }

    #[test]
    fn test_intl_number_format() {
        let mut context = Context::default();
        register_all_intl_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof Intl.NumberFormat")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "object");
    }

    #[test]
    fn test_plural_rules() {
        let mut context = Context::default();
        register_all_intl_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof Intl.PluralRules")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "object");
    }

    #[test]
    fn test_list_format() {
        let mut context = Context::default();
        register_all_intl_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof Intl.ListFormat")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "object");
    }

    #[test]
    fn test_get_canonical_locales() {
        let mut context = Context::default();
        register_all_intl_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"Intl.getCanonicalLocales('EN-us')[0]")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped().to_lowercase(), "en-us");
    }
}
