//! DOM Core APIs implementation
//!
//! Implements the remaining DOM Core APIs:
//! - CharacterData (base interface for Text, Comment, CDATASection)
//! - DOMException (full exception class with all error codes)
//! - DOMStringList (string list interface)
//! - DOMStringMap (dataset property)
//! - DOMImplementation (document.implementation)
//! - DocumentType (full doctype node)
//! - AbstractRange (base for Range and StaticRange)
//! - ProcessingInstruction (XML processing instruction node)
//! - XMLDocument (XML document type)

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer,
    object::FunctionObjectBuilder, object::builtins::JsArray, property::Attribute,
    Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue,
};
use std::cell::RefCell;
use std::rc::Rc;

/// Register all DOM Core APIs
pub fn register_all_dom_core_apis(context: &mut Context) -> JsResult<()> {
    register_dom_exception(context)?;
    register_dom_string_list(context)?;
    register_dom_implementation(context)?;
    register_document_type(context)?;
    register_character_data(context)?;
    register_abstract_range(context)?;
    register_processing_instruction(context)?;
    register_xml_document(context)?;
    Ok(())
}

// ============================================================================
// DOMException - Full implementation with all error codes
// ============================================================================

/// DOMException error code lookup (code, default message)
fn get_exception_info(name: &str) -> (u16, &'static str) {
    match name {
        "IndexSizeError" => (1, "The index is not in the allowed range."),
        "HierarchyRequestError" => (3, "The operation would yield an incorrect node tree."),
        "WrongDocumentError" => (4, "The object is in the wrong document."),
        "InvalidCharacterError" => (5, "The string contains invalid characters."),
        "NoModificationAllowedError" => (7, "The object can not be modified."),
        "NotFoundError" => (8, "The object can not be found here."),
        "NotSupportedError" => (9, "The operation is not supported."),
        "InUseAttributeError" => (10, "The attribute is in use."),
        "InvalidStateError" => (11, "The object is in an invalid state."),
        "SyntaxError" => (12, "The string did not match the expected pattern."),
        "InvalidModificationError" => (13, "The object can not be modified in this way."),
        "NamespaceError" => (14, "The operation is not allowed by Namespaces in XML."),
        "InvalidAccessError" => (15, "The object does not support the operation or argument."),
        "TypeMismatchError" => (17, "The type of the object does not match the expected type."),
        "SecurityError" => (18, "The operation is insecure."),
        "NetworkError" => (19, "A network error occurred."),
        "AbortError" => (20, "The operation was aborted."),
        "URLMismatchError" => (21, "The given URL does not match another URL."),
        "QuotaExceededError" => (22, "The quota has been exceeded."),
        "TimeoutError" => (23, "The operation timed out."),
        "InvalidNodeTypeError" => (24, "The supplied node is incorrect or has an incorrect ancestor for this operation."),
        "DataCloneError" => (25, "The object can not be cloned."),
        "EncodingError" => (0, "The encoding operation failed."),
        "NotReadableError" => (0, "The I/O read operation failed."),
        "UnknownError" => (0, "An unknown error occurred."),
        "ConstraintError" => (0, "A constraint was not satisfied."),
        "DataError" => (0, "Provided data is inadequate."),
        "TransactionInactiveError" => (0, "A request was placed against a transaction which is currently not active."),
        "ReadOnlyError" => (0, "The mutating operation was attempted in a read-only transaction."),
        "VersionError" => (0, "An attempt was made to open a database using a lower version."),
        "OperationError" => (0, "The operation failed for an operation-specific reason."),
        "NotAllowedError" => (0, "The request is not allowed."),
        _ => (0, "An unknown error occurred."),
    }
}


/// Register DOMException constructor and prototype
fn register_dom_exception(context: &mut Context) -> JsResult<()> {
    // DOMException constructor
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let message = args.get_or_undefined(0);
        let name = args.get_or_undefined(1);

        let message_str = if message.is_undefined() {
            String::new()
        } else {
            message.to_string(ctx)?.to_std_string_escaped()
        };

        let name_str = if name.is_undefined() {
            "Error".to_string()
        } else {
            name.to_string(ctx)?.to_std_string_escaped()
        };

        let (code, default_message) = get_exception_info(&name_str);
        let final_message = if message_str.is_empty() {
            default_message.to_string()
        } else {
            message_str
        };

        // Create the formatted string for toString before building the object
        let to_string_result = format!("{}: {}", name_str, final_message);

        let exception = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name_str), Attribute::all())
            .property(js_string!("message"), js_string!(final_message), Attribute::all())
            .property(js_string!("code"), code, Attribute::READONLY)
            .build();

        // Add toString method - capture the pre-built string
        let to_string = unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                Ok(JsValue::from(js_string!(to_string_result.clone())))
            })
        };
        let _ = exception.set(js_string!("toString"), to_string.to_js_function(ctx.realm()), false, ctx);

        Ok(JsValue::from(exception))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("DOMException"))
        .length(2)
        .constructor(true)
        .build();

    // Add static error code constants
    let error_codes = [
        ("INDEX_SIZE_ERR", 1),
        ("DOMSTRING_SIZE_ERR", 2),
        ("HIERARCHY_REQUEST_ERR", 3),
        ("WRONG_DOCUMENT_ERR", 4),
        ("INVALID_CHARACTER_ERR", 5),
        ("NO_DATA_ALLOWED_ERR", 6),
        ("NO_MODIFICATION_ALLOWED_ERR", 7),
        ("NOT_FOUND_ERR", 8),
        ("NOT_SUPPORTED_ERR", 9),
        ("INUSE_ATTRIBUTE_ERR", 10),
        ("INVALID_STATE_ERR", 11),
        ("SYNTAX_ERR", 12),
        ("INVALID_MODIFICATION_ERR", 13),
        ("NAMESPACE_ERR", 14),
        ("INVALID_ACCESS_ERR", 15),
        ("VALIDATION_ERR", 16),
        ("TYPE_MISMATCH_ERR", 17),
        ("SECURITY_ERR", 18),
        ("NETWORK_ERR", 19),
        ("ABORT_ERR", 20),
        ("URL_MISMATCH_ERR", 21),
        ("QUOTA_EXCEEDED_ERR", 22),
        ("TIMEOUT_ERR", 23),
        ("INVALID_NODE_TYPE_ERR", 24),
        ("DATA_CLONE_ERR", 25),
    ];

    for (name, code) in error_codes {
        let _ = ctor.set(js_string!(name), JsValue::from(code), false, context);
    }

    context.register_global_property(js_string!("DOMException"), ctor, Attribute::all())?;

    Ok(())
}

// ============================================================================
// DOMStringList - String list interface
// ============================================================================

/// Create a DOMStringList object
pub fn create_dom_string_list(strings: Vec<String>, context: &mut Context) -> JsObject {
    let state = Rc::new(RefCell::new(strings));

    // length getter
    let state_clone = state.clone();
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(state_clone.borrow().len() as u32))
        })
    };

    // item(index)
    let state_clone = state.clone();
    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let strings = state_clone.borrow();
            match strings.get(index) {
                Some(s) => Ok(JsValue::from(js_string!(s.clone()))),
                None => Ok(JsValue::null()),
            }
        })
    };

    // contains(string)
    let state_clone = state.clone();
    let contains = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let search = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let strings = state_clone.borrow();
            Ok(JsValue::from(strings.contains(&search)))
        })
    };

    let length_fn = length_getter.to_js_function(context.realm());

    let list = ObjectInitializer::new(context)
        .function(item, js_string!("item"), 1)
        .function(contains, js_string!("contains"), 1)
        .accessor(js_string!("length"), Some(length_fn), None, Attribute::READONLY)
        .build();

    // Add indexed properties
    let strings = state.borrow();
    for (i, s) in strings.iter().enumerate() {
        let _ = list.set(js_string!(i.to_string()), js_string!(s.clone()), false, context);
    }

    list
}

/// Register DOMStringList constructor
fn register_dom_string_list(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_string_list(vec![], ctx)))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("DOMStringList"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("DOMStringList"), ctor, Attribute::all())?;

    Ok(())
}

// ============================================================================
// DOMStringMap - dataset property implementation
// ============================================================================

/// Create a DOMStringMap object for element.dataset
pub fn create_dom_string_map(
    initial_data: Vec<(String, String)>,
    context: &mut Context,
) -> JsObject {
    let state = Rc::new(RefCell::new(
        initial_data.into_iter().collect::<std::collections::HashMap<String, String>>()
    ));

    // Create proxy-like behavior with getters/setters
    let map = ObjectInitializer::new(context).build();

    // Add initial properties
    let data = state.borrow();
    for (key, value) in data.iter() {
        let _ = map.set(js_string!(key.clone()), js_string!(value.clone()), false, context);
    }
    drop(data);

    // Add iterator support
    let state_clone = state.clone();
    let keys_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let arr = JsArray::new(ctx);
            let data = state_clone.borrow();
            for key in data.keys() {
                let _ = arr.push(JsValue::from(js_string!(key.clone())), ctx);
            }
            Ok(JsValue::from(arr))
        })
    };

    let state_clone = state.clone();
    let values_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let arr = JsArray::new(ctx);
            let data = state_clone.borrow();
            for value in data.values() {
                let _ = arr.push(JsValue::from(js_string!(value.clone())), ctx);
            }
            Ok(JsValue::from(arr))
        })
    };

    let state_clone = state.clone();
    let entries_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let arr = JsArray::new(ctx);
            let data = state_clone.borrow();
            for (key, value) in data.iter() {
                let entry = JsArray::new(ctx);
                let _ = entry.push(JsValue::from(js_string!(key.clone())), ctx);
                let _ = entry.push(JsValue::from(js_string!(value.clone())), ctx);
                let _ = arr.push(JsValue::from(entry), ctx);
            }
            Ok(JsValue::from(arr))
        })
    };

    let _ = map.set(js_string!("keys"), keys_fn.to_js_function(context.realm()), false, context);
    let _ = map.set(js_string!("values"), values_fn.to_js_function(context.realm()), false, context);
    let _ = map.set(js_string!("entries"), entries_fn.to_js_function(context.realm()), false, context);

    map
}

/// Convert camelCase to data-kebab-case
pub fn camel_to_data_attr(name: &str) -> String {
    let mut result = String::from("data-");
    for c in name.chars() {
        if c.is_uppercase() {
            result.push('-');
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Convert data-kebab-case to camelCase
pub fn data_attr_to_camel(name: &str) -> String {
    let name = name.strip_prefix("data-").unwrap_or(name);
    let mut result = String::new();
    let mut capitalize_next = false;

    for c in name.chars() {
        if c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

// ============================================================================
// DOMImplementation - document.implementation
// ============================================================================

/// Register DOMImplementation
fn register_dom_implementation(context: &mut Context) -> JsResult<()> {
    // hasFeature(feature, version) - always returns true per spec
    let has_feature = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });

    // createDocumentType(qualifiedName, publicId, systemId)
    let create_document_type = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let qualified_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let public_id = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        let system_id = args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped();

        Ok(JsValue::from(create_document_type_node(ctx, &qualified_name, &public_id, &system_id)))
    });

    // createDocument(namespaceURI, qualifiedName, doctype)
    let create_document = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let namespace_uri = if args.get_or_undefined(0).is_null() {
            None
        } else {
            Some(args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped())
        };
        let qualified_name = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        let doctype = if args.get_or_undefined(2).is_null_or_undefined() {
            JsValue::null()
        } else {
            args.get_or_undefined(2).clone()
        };

        // Create full XMLDocument with all methods
        let child_nodes = JsArray::new(ctx);
        let style_sheets = JsArray::new(ctx);
        let scripts = JsArray::new(ctx);
        let images = JsArray::new(ctx);
        let links = JsArray::new(ctx);
        let forms = JsArray::new(ctx);

        // createElement
        let create_element_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let tag_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let child_nodes = JsArray::new(ctx);
            let element = ObjectInitializer::new(ctx)
                .property(js_string!("nodeType"), 1, Attribute::READONLY)
                .property(js_string!("nodeName"), js_string!(tag_name.clone()), Attribute::READONLY)
                .property(js_string!("tagName"), js_string!(tag_name.clone()), Attribute::READONLY)
                .property(js_string!("localName"), js_string!(tag_name), Attribute::READONLY)
                .property(js_string!("namespaceURI"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("prefix"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
                .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("parentNode"), JsValue::null(), Attribute::all())
                .property(js_string!("parentElement"), JsValue::null(), Attribute::all())
                .property(js_string!("textContent"), js_string!(""), Attribute::all())
                .property(js_string!("innerHTML"), js_string!(""), Attribute::all())
                .build();
            Ok(JsValue::from(element))
        });

        // createElementNS
        let create_element_ns_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let ns = if args.get_or_undefined(0).is_null() {
                None
            } else {
                Some(args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped())
            };
            let qname = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let (prefix, local_name) = if let Some(colon_pos) = qname.find(':') {
                (Some(qname[..colon_pos].to_string()), qname[colon_pos + 1..].to_string())
            } else {
                (None, qname.clone())
            };
            let child_nodes = JsArray::new(ctx);
            let element = ObjectInitializer::new(ctx)
                .property(js_string!("nodeType"), 1, Attribute::READONLY)
                .property(js_string!("nodeName"), js_string!(qname.clone()), Attribute::READONLY)
                .property(js_string!("tagName"), js_string!(qname), Attribute::READONLY)
                .property(js_string!("localName"), js_string!(local_name), Attribute::READONLY)
                .property(js_string!("namespaceURI"), ns.map(|s| JsValue::from(js_string!(s))).unwrap_or(JsValue::null()), Attribute::READONLY)
                .property(js_string!("prefix"), prefix.map(|s| JsValue::from(js_string!(s))).unwrap_or(JsValue::null()), Attribute::READONLY)
                .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
                .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("parentNode"), JsValue::null(), Attribute::all())
                .property(js_string!("textContent"), js_string!(""), Attribute::all())
                .build();
            Ok(JsValue::from(element))
        });

        // createTextNode
        let create_text_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::from(create_character_data_object(ctx, 3, "#text", &data)))
        });

        // createComment
        let create_comment_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::from(create_character_data_object(ctx, 8, "#comment", &data)))
        });

        // createCDATASection
        let create_cdata_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::from(create_character_data_object(ctx, 4, "#cdata-section", &data)))
        });

        // createProcessingInstruction
        let create_pi_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let target = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let data = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::from(create_processing_instruction(ctx, &target, &data)))
        });

        // createDocumentFragment
        let create_frag_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let child_nodes = JsArray::new(ctx);
            let frag = ObjectInitializer::new(ctx)
                .property(js_string!("nodeType"), 11, Attribute::READONLY)
                .property(js_string!("nodeName"), js_string!("#document-fragment"), Attribute::READONLY)
                .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
                .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("textContent"), js_string!(""), Attribute::all())
                .build();
            Ok(JsValue::from(frag))
        });

        // getElementById
        let get_by_id_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        // getElementsByTagName
        let get_by_tag_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        // getElementsByTagNameNS
        let get_by_tag_ns_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        // querySelector
        let query_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        // querySelectorAll
        let query_all_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        // importNode
        let import_fn = NativeFunction::from_copy_closure(|_this, args, _ctx| {
            Ok(args.get_or_undefined(0).clone())
        });

        // adoptNode
        let adopt_fn = NativeFunction::from_copy_closure(|_this, args, _ctx| {
            Ok(args.get_or_undefined(0).clone())
        });

        let doc = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 9, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#document"), Attribute::READONLY)
            .property(js_string!("contentType"), js_string!("application/xml"), Attribute::READONLY)
            .property(js_string!("characterSet"), js_string!("UTF-8"), Attribute::READONLY)
            .property(js_string!("charset"), js_string!("UTF-8"), Attribute::READONLY)
            .property(js_string!("inputEncoding"), js_string!("UTF-8"), Attribute::READONLY)
            .property(js_string!("URL"), js_string!("about:blank"), Attribute::READONLY)
            .property(js_string!("documentURI"), js_string!("about:blank"), Attribute::READONLY)
            .property(js_string!("compatMode"), js_string!("CSS1Compat"), Attribute::READONLY)
            .property(js_string!("xmlVersion"), js_string!("1.0"), Attribute::all())
            .property(js_string!("xmlEncoding"), js_string!("UTF-8"), Attribute::READONLY)
            .property(js_string!("xmlStandalone"), false, Attribute::all())
            .property(js_string!("namespaceURI"), namespace_uri.as_ref().map(|s| JsValue::from(js_string!(s.clone()))).unwrap_or(JsValue::null()), Attribute::READONLY)
            .property(js_string!("qualifiedName"), js_string!(qualified_name), Attribute::READONLY)
            .property(js_string!("doctype"), doctype, Attribute::all())
            .property(js_string!("documentElement"), JsValue::null(), Attribute::all())
            .property(js_string!("head"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("body"), JsValue::null(), Attribute::all())
            .property(js_string!("title"), js_string!(""), Attribute::all())
            .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
            .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("parentNode"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("parentElement"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("previousSibling"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("nextSibling"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("nodeValue"), JsValue::null(), Attribute::all())
            .property(js_string!("textContent"), JsValue::null(), Attribute::all())
            .property(js_string!("ownerDocument"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("styleSheets"), JsValue::from(style_sheets), Attribute::READONLY)
            .property(js_string!("scripts"), JsValue::from(scripts), Attribute::READONLY)
            .property(js_string!("images"), JsValue::from(images), Attribute::READONLY)
            .property(js_string!("links"), JsValue::from(links), Attribute::READONLY)
            .property(js_string!("forms"), JsValue::from(forms), Attribute::READONLY)
            .function(create_element_fn, js_string!("createElement"), 1)
            .function(create_element_ns_fn, js_string!("createElementNS"), 2)
            .function(create_text_fn, js_string!("createTextNode"), 1)
            .function(create_comment_fn, js_string!("createComment"), 1)
            .function(create_cdata_fn, js_string!("createCDATASection"), 1)
            .function(create_pi_fn, js_string!("createProcessingInstruction"), 2)
            .function(create_frag_fn, js_string!("createDocumentFragment"), 0)
            .function(get_by_id_fn, js_string!("getElementById"), 1)
            .function(get_by_tag_fn, js_string!("getElementsByTagName"), 1)
            .function(get_by_tag_ns_fn, js_string!("getElementsByTagNameNS"), 2)
            .function(query_fn, js_string!("querySelector"), 1)
            .function(query_all_fn, js_string!("querySelectorAll"), 1)
            .function(import_fn, js_string!("importNode"), 2)
            .function(adopt_fn, js_string!("adoptNode"), 1)
            .build();

        Ok(JsValue::from(doc))
    });

    // createHTMLDocument(title?)
    let create_html_document = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let title = if args.get_or_undefined(0).is_undefined() {
            String::new()
        } else {
            args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()
        };

        // Create a minimal HTMLDocument
        let doc = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 9, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#document"), Attribute::READONLY)
            .property(js_string!("contentType"), js_string!("text/html"), Attribute::READONLY)
            .property(js_string!("title"), js_string!(title), Attribute::all())
            .property(js_string!("documentElement"), JsValue::null(), Attribute::all())
            .property(js_string!("head"), JsValue::null(), Attribute::all())
            .property(js_string!("body"), JsValue::null(), Attribute::all())
            .build();

        Ok(JsValue::from(doc))
    });

    let implementation = ObjectInitializer::new(context)
        .function(has_feature, js_string!("hasFeature"), 2)
        .function(create_document_type, js_string!("createDocumentType"), 3)
        .function(create_document, js_string!("createDocument"), 3)
        .function(create_html_document, js_string!("createHTMLDocument"), 1)
        .build();

    // Also make it available as a constructor (for instanceof checks)
    let constructor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(JsNativeError::typ().with_message("Illegal constructor").into())
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("DOMImplementation"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("DOMImplementation"), ctor, Attribute::all())?;

    // Register the implementation object on document if it exists
    if let Ok(doc_val) = context.global_object().get(js_string!("document"), context) {
        if let Some(doc_obj) = doc_val.as_object() {
            let _ = doc_obj.set(js_string!("implementation"), implementation, false, context);
        }
    }

    Ok(())
}

// ============================================================================
// DocumentType - Full doctype node implementation
// ============================================================================

/// Create a DocumentType node
pub fn create_document_type_node(
    context: &mut Context,
    name: &str,
    public_id: &str,
    system_id: &str,
) -> JsObject {
    // Create childNodes array before using context in ObjectInitializer
    let child_nodes = JsArray::new(context);

    ObjectInitializer::new(context)
        .property(js_string!("nodeType"), 10, Attribute::READONLY)  // DOCUMENT_TYPE_NODE = 10
        .property(js_string!("nodeName"), js_string!(name), Attribute::READONLY)
        .property(js_string!("name"), js_string!(name), Attribute::READONLY)
        .property(js_string!("publicId"), js_string!(public_id), Attribute::READONLY)
        .property(js_string!("systemId"), js_string!(system_id), Attribute::READONLY)
        .property(js_string!("internalSubset"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("ownerDocument"), JsValue::null(), Attribute::all())
        .property(js_string!("parentNode"), JsValue::null(), Attribute::all())
        .property(js_string!("parentElement"), JsValue::null(), Attribute::all())
        .property(js_string!("previousSibling"), JsValue::null(), Attribute::all())
        .property(js_string!("nextSibling"), JsValue::null(), Attribute::all())
        .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
        .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("textContent"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("nodeValue"), JsValue::null(), Attribute::READONLY)
        .build()
}

/// Register DocumentType constructor
fn register_document_type(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(JsNativeError::typ().with_message("Illegal constructor").into())
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("DocumentType"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("DocumentType"), ctor, Attribute::all())?;

    Ok(())
}

// ============================================================================
// CharacterData - Base interface for Text, Comment, CDATASection
// ============================================================================

/// Create a CharacterData-like object with all CharacterData methods
pub fn create_character_data_object(
    context: &mut Context,
    node_type: u32,
    node_name: &str,
    initial_data: &str,
) -> JsObject {
    let state = Rc::new(RefCell::new(initial_data.to_string()));

    // data getter
    let state_clone = state.clone();
    let data_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(state_clone.borrow().clone())))
        })
    };

    // data setter
    let state_clone = state.clone();
    let data_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            *state_clone.borrow_mut() = data;
            Ok(JsValue::undefined())
        })
    };

    // length getter
    let state_clone = state.clone();
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(state_clone.borrow().chars().count() as u32))
        })
    };

    // substringData(offset, count)
    let state_clone = state.clone();
    let substring_data = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let offset = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let count = args.get_or_undefined(1).to_u32(ctx)? as usize;
            let data = state_clone.borrow();
            let chars: Vec<char> = data.chars().collect();

            if offset > chars.len() {
                return Err(JsNativeError::range()
                    .with_message("Offset is out of range")
                    .into());
            }

            let end = std::cmp::min(offset + count, chars.len());
            let substring: String = chars[offset..end].iter().collect();
            Ok(JsValue::from(js_string!(substring)))
        })
    };

    // appendData(data)
    let state_clone = state.clone();
    let append_data = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            state_clone.borrow_mut().push_str(&data);
            Ok(JsValue::undefined())
        })
    };

    // insertData(offset, data)
    let state_clone = state.clone();
    let insert_data = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let offset = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let data = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let mut current = state_clone.borrow_mut();
            let chars: Vec<char> = current.chars().collect();

            if offset > chars.len() {
                return Err(JsNativeError::range()
                    .with_message("Offset is out of range")
                    .into());
            }

            let mut result: String = chars[..offset].iter().collect();
            result.push_str(&data);
            result.extend(chars[offset..].iter());
            *current = result;
            Ok(JsValue::undefined())
        })
    };

    // deleteData(offset, count)
    let state_clone = state.clone();
    let delete_data = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let offset = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let count = args.get_or_undefined(1).to_u32(ctx)? as usize;
            let mut current = state_clone.borrow_mut();
            let chars: Vec<char> = current.chars().collect();

            if offset > chars.len() {
                return Err(JsNativeError::range()
                    .with_message("Offset is out of range")
                    .into());
            }

            let end = std::cmp::min(offset + count, chars.len());
            let mut result: String = chars[..offset].iter().collect();
            result.extend(chars[end..].iter());
            *current = result;
            Ok(JsValue::undefined())
        })
    };

    // replaceData(offset, count, data)
    let state_clone = state.clone();
    let replace_data = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let offset = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let count = args.get_or_undefined(1).to_u32(ctx)? as usize;
            let data = args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped();
            let mut current = state_clone.borrow_mut();
            let chars: Vec<char> = current.chars().collect();

            if offset > chars.len() {
                return Err(JsNativeError::range()
                    .with_message("Offset is out of range")
                    .into());
            }

            let end = std::cmp::min(offset + count, chars.len());
            let mut result: String = chars[..offset].iter().collect();
            result.push_str(&data);
            result.extend(chars[end..].iter());
            *current = result;
            Ok(JsValue::undefined())
        })
    };

    // before(...nodes)
    let before = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // after(...nodes)
    let after = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // replaceWith(...nodes)
    let replace_with = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // remove()
    let remove = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let data_getter_fn = data_getter.to_js_function(context.realm());
    let data_setter_fn = data_setter.to_js_function(context.realm());
    let length_fn = length_getter.to_js_function(context.realm());

    ObjectInitializer::new(context)
        .property(js_string!("nodeType"), node_type, Attribute::READONLY)
        .property(js_string!("nodeName"), js_string!(node_name), Attribute::READONLY)
        .accessor(js_string!("data"), Some(data_getter_fn.clone()), Some(data_setter_fn.clone()), Attribute::CONFIGURABLE)
        .accessor(js_string!("nodeValue"), Some(data_getter_fn.clone()), Some(data_setter_fn.clone()), Attribute::CONFIGURABLE)
        .accessor(js_string!("textContent"), Some(data_getter_fn), Some(data_setter_fn), Attribute::CONFIGURABLE)
        .accessor(js_string!("length"), Some(length_fn), None, Attribute::READONLY)
        .function(substring_data, js_string!("substringData"), 2)
        .function(append_data, js_string!("appendData"), 1)
        .function(insert_data, js_string!("insertData"), 2)
        .function(delete_data, js_string!("deleteData"), 2)
        .function(replace_data, js_string!("replaceData"), 3)
        .function(before, js_string!("before"), 0)
        .function(after, js_string!("after"), 0)
        .function(replace_with, js_string!("replaceWith"), 0)
        .function(remove, js_string!("remove"), 0)
        .property(js_string!("parentNode"), JsValue::null(), Attribute::all())
        .property(js_string!("parentElement"), JsValue::null(), Attribute::all())
        .property(js_string!("previousSibling"), JsValue::null(), Attribute::all())
        .property(js_string!("nextSibling"), JsValue::null(), Attribute::all())
        .property(js_string!("previousElementSibling"), JsValue::null(), Attribute::all())
        .property(js_string!("nextElementSibling"), JsValue::null(), Attribute::all())
        .build()
}

/// Register CharacterData as a base class
fn register_character_data(context: &mut Context) -> JsResult<()> {
    // CharacterData is abstract, cannot be directly instantiated
    let constructor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(JsNativeError::typ().with_message("Illegal constructor").into())
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("CharacterData"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("CharacterData"), ctor, Attribute::all())?;

    // Also register Text, Comment constructors
    register_text_constructor(context)?;
    register_comment_constructor(context)?;

    Ok(())
}

/// Register Text constructor
fn register_text_constructor(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let data = if args.get_or_undefined(0).is_undefined() {
            String::new()
        } else {
            args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()
        };

        let text_node = create_character_data_object(ctx, 3, "#text", &data);

        // Add Text-specific methods
        let split_text = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            // Return a new empty text node (simplified)
            Ok(JsValue::from(create_character_data_object(ctx, 3, "#text", "")))
        });
        let _ = text_node.set(js_string!("splitText"), split_text.to_js_function(ctx.realm()), false, ctx);

        // wholeText getter (returns same as data for now)
        let whole_text = text_node.get(js_string!("data"), ctx).unwrap_or(JsValue::from(js_string!("")));
        let _ = text_node.set(js_string!("wholeText"), whole_text, false, ctx);

        // assignedSlot
        let _ = text_node.set(js_string!("assignedSlot"), JsValue::null(), false, ctx);

        Ok(JsValue::from(text_node))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("Text"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("Text"), ctor, Attribute::all())?;

    Ok(())
}

/// Register Comment constructor
fn register_comment_constructor(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let data = if args.get_or_undefined(0).is_undefined() {
            String::new()
        } else {
            args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped()
        };

        Ok(JsValue::from(create_character_data_object(ctx, 8, "#comment", &data)))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("Comment"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("Comment"), ctor, Attribute::all())?;

    Ok(())
}

// ============================================================================
// AbstractRange - Base for Range and StaticRange
// ============================================================================

/// Register AbstractRange base class
fn register_abstract_range(context: &mut Context) -> JsResult<()> {
    // AbstractRange is abstract, cannot be directly instantiated
    let constructor = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(JsNativeError::typ().with_message("Illegal constructor").into())
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("AbstractRange"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("AbstractRange"), ctor, Attribute::all())?;

    Ok(())
}

/// Create an AbstractRange-like object
pub fn create_abstract_range(
    context: &mut Context,
    start_container: JsValue,
    start_offset: u32,
    end_container: JsValue,
    end_offset: u32,
) -> JsObject {
    let collapsed = start_container == end_container && start_offset == end_offset;

    ObjectInitializer::new(context)
        .property(js_string!("startContainer"), start_container.clone(), Attribute::READONLY)
        .property(js_string!("startOffset"), start_offset, Attribute::READONLY)
        .property(js_string!("endContainer"), end_container, Attribute::READONLY)
        .property(js_string!("endOffset"), end_offset, Attribute::READONLY)
        .property(js_string!("collapsed"), collapsed, Attribute::READONLY)
        .build()
}

// ============================================================================
// ProcessingInstruction - XML processing instruction node
// ============================================================================

/// Register ProcessingInstruction constructor
fn register_processing_instruction(context: &mut Context) -> JsResult<()> {
    // ProcessingInstruction constructor: new ProcessingInstruction(target, data)
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let target = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let data = if args.len() > 1 {
            args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped()
        } else {
            String::new()
        };
        Ok(JsValue::from(create_processing_instruction(ctx, &target, &data)))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("ProcessingInstruction"))
        .length(2)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("ProcessingInstruction"), ctor, Attribute::all())?;

    Ok(())
}

/// Create a ProcessingInstruction node
/// ProcessingInstruction inherits from CharacterData
/// nodeType = 7 (PROCESSING_INSTRUCTION_NODE)
pub fn create_processing_instruction(
    context: &mut Context,
    target: &str,
    data: &str,
) -> JsObject {
    let state = Rc::new(RefCell::new(data.to_string()));
    let target_str = target.to_string();

    // data getter
    let state_clone = state.clone();
    let data_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(js_string!(state_clone.borrow().clone())))
        })
    };

    // data setter
    let state_clone = state.clone();
    let data_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            *state_clone.borrow_mut() = data;
            Ok(JsValue::undefined())
        })
    };

    // length getter
    let state_clone = state.clone();
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(state_clone.borrow().chars().count() as u32))
        })
    };

    // substringData(offset, count)
    let state_clone = state.clone();
    let substring_data = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let offset = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let count = args.get_or_undefined(1).to_u32(ctx)? as usize;
            let data = state_clone.borrow();
            let chars: Vec<char> = data.chars().collect();

            if offset > chars.len() {
                return Err(JsNativeError::range()
                    .with_message("Offset is out of range")
                    .into());
            }

            let end = std::cmp::min(offset + count, chars.len());
            let substring: String = chars[offset..end].iter().collect();
            Ok(JsValue::from(js_string!(substring)))
        })
    };

    // appendData(data)
    let state_clone = state.clone();
    let append_data = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            state_clone.borrow_mut().push_str(&data);
            Ok(JsValue::undefined())
        })
    };

    // insertData(offset, data)
    let state_clone = state.clone();
    let insert_data = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let offset = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let data = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let mut current = state_clone.borrow_mut();
            let chars: Vec<char> = current.chars().collect();

            if offset > chars.len() {
                return Err(JsNativeError::range()
                    .with_message("Offset is out of range")
                    .into());
            }

            let mut result: String = chars[..offset].iter().collect();
            result.push_str(&data);
            result.extend(chars[offset..].iter());
            *current = result;
            Ok(JsValue::undefined())
        })
    };

    // deleteData(offset, count)
    let state_clone = state.clone();
    let delete_data = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let offset = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let count = args.get_or_undefined(1).to_u32(ctx)? as usize;
            let mut current = state_clone.borrow_mut();
            let chars: Vec<char> = current.chars().collect();

            if offset > chars.len() {
                return Err(JsNativeError::range()
                    .with_message("Offset is out of range")
                    .into());
            }

            let end = std::cmp::min(offset + count, chars.len());
            let mut result: String = chars[..offset].iter().collect();
            result.extend(chars[end..].iter());
            *current = result;
            Ok(JsValue::undefined())
        })
    };

    // replaceData(offset, count, data)
    let state_clone = state.clone();
    let replace_data = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let offset = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let count = args.get_or_undefined(1).to_u32(ctx)? as usize;
            let data = args.get_or_undefined(2).to_string(ctx)?.to_std_string_escaped();
            let mut current = state_clone.borrow_mut();
            let chars: Vec<char> = current.chars().collect();

            if offset > chars.len() {
                return Err(JsNativeError::range()
                    .with_message("Offset is out of range")
                    .into());
            }

            let end = std::cmp::min(offset + count, chars.len());
            let mut result: String = chars[..offset].iter().collect();
            result.push_str(&data);
            result.extend(chars[end..].iter());
            *current = result;
            Ok(JsValue::undefined())
        })
    };

    // before(...nodes)
    let before = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // after(...nodes)
    let after = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // replaceWith(...nodes)
    let replace_with = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // remove()
    let remove = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let data_getter_fn = data_getter.to_js_function(context.realm());
    let data_setter_fn = data_setter.to_js_function(context.realm());
    let length_fn = length_getter.to_js_function(context.realm());

    // Create childNodes array before ObjectInitializer
    let child_nodes = JsArray::new(context);

    ObjectInitializer::new(context)
        // ProcessingInstruction-specific properties
        .property(js_string!("nodeType"), 7, Attribute::READONLY)  // PROCESSING_INSTRUCTION_NODE = 7
        .property(js_string!("nodeName"), js_string!(target_str.clone()), Attribute::READONLY)
        .property(js_string!("target"), js_string!(target_str), Attribute::READONLY)
        // CharacterData interface
        .accessor(js_string!("data"), Some(data_getter_fn.clone()), Some(data_setter_fn.clone()), Attribute::CONFIGURABLE)
        .accessor(js_string!("nodeValue"), Some(data_getter_fn.clone()), Some(data_setter_fn.clone()), Attribute::CONFIGURABLE)
        .accessor(js_string!("textContent"), Some(data_getter_fn), Some(data_setter_fn), Attribute::CONFIGURABLE)
        .accessor(js_string!("length"), Some(length_fn), None, Attribute::READONLY)
        .function(substring_data, js_string!("substringData"), 2)
        .function(append_data, js_string!("appendData"), 1)
        .function(insert_data, js_string!("insertData"), 2)
        .function(delete_data, js_string!("deleteData"), 2)
        .function(replace_data, js_string!("replaceData"), 3)
        .function(before, js_string!("before"), 0)
        .function(after, js_string!("after"), 0)
        .function(replace_with, js_string!("replaceWith"), 0)
        .function(remove, js_string!("remove"), 0)
        // Node interface properties
        .property(js_string!("ownerDocument"), JsValue::null(), Attribute::all())
        .property(js_string!("parentNode"), JsValue::null(), Attribute::all())
        .property(js_string!("parentElement"), JsValue::null(), Attribute::all())
        .property(js_string!("previousSibling"), JsValue::null(), Attribute::all())
        .property(js_string!("nextSibling"), JsValue::null(), Attribute::all())
        .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
        .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
        .build()
}

// ============================================================================
// XMLDocument - XML document type
// ============================================================================

/// Register XMLDocument constructor
fn register_xml_document(context: &mut Context) -> JsResult<()> {
    // XMLDocument constructor: new XMLDocument() creates a blank XML document
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Create a minimal XMLDocument with basic properties and methods
        let child_nodes = JsArray::new(ctx);

        // createElement
        let create_element_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let tag_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let child_nodes = JsArray::new(ctx);
            let element = ObjectInitializer::new(ctx)
                .property(js_string!("nodeType"), 1, Attribute::READONLY)
                .property(js_string!("nodeName"), js_string!(tag_name.clone()), Attribute::READONLY)
                .property(js_string!("tagName"), js_string!(tag_name.clone()), Attribute::READONLY)
                .property(js_string!("localName"), js_string!(tag_name), Attribute::READONLY)
                .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
                .property(js_string!("textContent"), js_string!(""), Attribute::all())
                .build();
            Ok(JsValue::from(element))
        });

        // createTextNode
        let create_text_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::from(create_character_data_object(ctx, 3, "#text", &data)))
        });

        // createComment
        let create_comment_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            Ok(JsValue::from(create_character_data_object(ctx, 8, "#comment", &data)))
        });

        // getElementById
        let get_by_id_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        // querySelector
        let query_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        // querySelectorAll
        let query_all_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        let doc = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 9, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#document"), Attribute::READONLY)
            .property(js_string!("contentType"), js_string!("application/xml"), Attribute::READONLY)
            .property(js_string!("characterSet"), js_string!("UTF-8"), Attribute::READONLY)
            .property(js_string!("xmlVersion"), js_string!("1.0"), Attribute::all())
            .property(js_string!("xmlEncoding"), js_string!("UTF-8"), Attribute::READONLY)
            .property(js_string!("xmlStandalone"), false, Attribute::all())
            .property(js_string!("documentElement"), JsValue::null(), Attribute::all())
            .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
            .function(create_element_fn, js_string!("createElement"), 1)
            .function(create_text_fn, js_string!("createTextNode"), 1)
            .function(create_comment_fn, js_string!("createComment"), 1)
            .function(get_by_id_fn, js_string!("getElementById"), 1)
            .function(query_fn, js_string!("querySelector"), 1)
            .function(query_all_fn, js_string!("querySelectorAll"), 1)
            .build();

        Ok(JsValue::from(doc))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("XMLDocument"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("XMLDocument"), ctor, Attribute::all())?;

    Ok(())
}

/// Create an XMLDocument object
/// XMLDocument extends Document with XML-specific behavior
pub fn create_xml_document(
    context: &mut Context,
    namespace_uri: Option<&str>,
    qualified_name: &str,
    doctype: Option<JsValue>,
) -> JsObject {
    // Create empty arrays/collections before ObjectInitializer
    let child_nodes = JsArray::new(context);
    let style_sheets = JsArray::new(context);
    let scripts = JsArray::new(context);
    let images = JsArray::new(context);
    let links = JsArray::new(context);
    let forms = JsArray::new(context);

    // createElement for XML documents
    let create_element = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let tag_name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        // Create a minimal XML element
        let child_nodes = JsArray::new(ctx);
        let element = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 1, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!(tag_name.clone()), Attribute::READONLY)
            .property(js_string!("tagName"), js_string!(tag_name.clone()), Attribute::READONLY)
            .property(js_string!("localName"), js_string!(tag_name), Attribute::READONLY)
            .property(js_string!("namespaceURI"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("prefix"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
            .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("parentNode"), JsValue::null(), Attribute::all())
            .property(js_string!("parentElement"), JsValue::null(), Attribute::all())
            .property(js_string!("textContent"), js_string!(""), Attribute::all())
            .property(js_string!("innerHTML"), js_string!(""), Attribute::all())
            .build();

        Ok(JsValue::from(element))
    });

    // createElementNS for namespaced elements
    let create_element_ns = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let ns = if args.get_or_undefined(0).is_null() {
            None
        } else {
            Some(args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped())
        };
        let qualified_name = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

        let (prefix, local_name) = if let Some(colon_pos) = qualified_name.find(':') {
            (Some(qualified_name[..colon_pos].to_string()), qualified_name[colon_pos + 1..].to_string())
        } else {
            (None, qualified_name.clone())
        };

        let child_nodes = JsArray::new(ctx);
        let element = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 1, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!(qualified_name.clone()), Attribute::READONLY)
            .property(js_string!("tagName"), js_string!(qualified_name), Attribute::READONLY)
            .property(js_string!("localName"), js_string!(local_name), Attribute::READONLY)
            .property(js_string!("namespaceURI"), ns.map(|s| JsValue::from(js_string!(s))).unwrap_or(JsValue::null()), Attribute::READONLY)
            .property(js_string!("prefix"), prefix.map(|s| JsValue::from(js_string!(s))).unwrap_or(JsValue::null()), Attribute::READONLY)
            .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
            .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("parentNode"), JsValue::null(), Attribute::all())
            .property(js_string!("parentElement"), JsValue::null(), Attribute::all())
            .property(js_string!("textContent"), js_string!(""), Attribute::all())
            .build();

        Ok(JsValue::from(element))
    });

    // createTextNode
    let create_text_node = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        Ok(JsValue::from(create_character_data_object(ctx, 3, "#text", &data)))
    });

    // createComment
    let create_comment = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        Ok(JsValue::from(create_character_data_object(ctx, 8, "#comment", &data)))
    });

    // createCDATASection (XML-specific)
    let create_cdata_section = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let data = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        // CDATA_SECTION_NODE = 4
        Ok(JsValue::from(create_character_data_object(ctx, 4, "#cdata-section", &data)))
    });

    // createProcessingInstruction
    let create_pi = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let target = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let data = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
        Ok(JsValue::from(create_processing_instruction(ctx, &target, &data)))
    });

    // createDocumentFragment
    let create_doc_fragment = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let child_nodes = JsArray::new(ctx);
        let frag = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 11, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#document-fragment"), Attribute::READONLY)
            .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
            .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("textContent"), js_string!(""), Attribute::all())
            .build();
        Ok(JsValue::from(frag))
    });

    // getElementById
    let get_element_by_id = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    // getElementsByTagName
    let get_elements_by_tag = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });

    // getElementsByTagNameNS
    let get_elements_by_tag_ns = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });

    // querySelector
    let query_selector = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    // querySelectorAll
    let query_selector_all = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });

    // importNode
    let import_node = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        // Return the node as-is (simplified)
        Ok(args.get_or_undefined(0).clone())
    });

    // adoptNode
    let adopt_node = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        Ok(args.get_or_undefined(0).clone())
    });

    ObjectInitializer::new(context)
        // Document interface properties
        .property(js_string!("nodeType"), 9, Attribute::READONLY)  // DOCUMENT_NODE = 9
        .property(js_string!("nodeName"), js_string!("#document"), Attribute::READONLY)
        .property(js_string!("contentType"), js_string!("application/xml"), Attribute::READONLY)
        .property(js_string!("characterSet"), js_string!("UTF-8"), Attribute::READONLY)
        .property(js_string!("charset"), js_string!("UTF-8"), Attribute::READONLY)
        .property(js_string!("inputEncoding"), js_string!("UTF-8"), Attribute::READONLY)
        .property(js_string!("URL"), js_string!("about:blank"), Attribute::READONLY)
        .property(js_string!("documentURI"), js_string!("about:blank"), Attribute::READONLY)
        .property(js_string!("compatMode"), js_string!("CSS1Compat"), Attribute::READONLY)
        .property(js_string!("xmlVersion"), js_string!("1.0"), Attribute::all())
        .property(js_string!("xmlEncoding"), js_string!("UTF-8"), Attribute::READONLY)
        .property(js_string!("xmlStandalone"), false, Attribute::all())
        // Namespace info
        .property(js_string!("namespaceURI"), namespace_uri.map(|s| JsValue::from(js_string!(s))).unwrap_or(JsValue::null()), Attribute::READONLY)
        .property(js_string!("qualifiedName"), js_string!(qualified_name), Attribute::READONLY)
        // Document structure
        .property(js_string!("doctype"), doctype.unwrap_or(JsValue::null()), Attribute::all())
        .property(js_string!("documentElement"), JsValue::null(), Attribute::all())
        .property(js_string!("head"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("body"), JsValue::null(), Attribute::all())
        .property(js_string!("title"), js_string!(""), Attribute::all())
        // Node interface
        .property(js_string!("childNodes"), JsValue::from(child_nodes), Attribute::READONLY)
        .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("parentNode"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("parentElement"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("previousSibling"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("nextSibling"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("nodeValue"), JsValue::null(), Attribute::all())
        .property(js_string!("textContent"), JsValue::null(), Attribute::all())
        .property(js_string!("ownerDocument"), JsValue::null(), Attribute::READONLY)
        // Collections
        .property(js_string!("styleSheets"), JsValue::from(style_sheets), Attribute::READONLY)
        .property(js_string!("scripts"), JsValue::from(scripts), Attribute::READONLY)
        .property(js_string!("images"), JsValue::from(images), Attribute::READONLY)
        .property(js_string!("links"), JsValue::from(links), Attribute::READONLY)
        .property(js_string!("forms"), JsValue::from(forms), Attribute::READONLY)
        // Document methods
        .function(create_element, js_string!("createElement"), 1)
        .function(create_element_ns, js_string!("createElementNS"), 2)
        .function(create_text_node, js_string!("createTextNode"), 1)
        .function(create_comment, js_string!("createComment"), 1)
        .function(create_cdata_section, js_string!("createCDATASection"), 1)
        .function(create_pi, js_string!("createProcessingInstruction"), 2)
        .function(create_doc_fragment, js_string!("createDocumentFragment"), 0)
        .function(get_element_by_id, js_string!("getElementById"), 1)
        .function(get_elements_by_tag, js_string!("getElementsByTagName"), 1)
        .function(get_elements_by_tag_ns, js_string!("getElementsByTagNameNS"), 2)
        .function(query_selector, js_string!("querySelector"), 1)
        .function(query_selector_all, js_string!("querySelectorAll"), 1)
        .function(import_node, js_string!("importNode"), 2)
        .function(adopt_node, js_string!("adoptNode"), 1)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camel_to_data_attr() {
        assert_eq!(camel_to_data_attr("userId"), "data-user-id");
        assert_eq!(camel_to_data_attr("myCustomValue"), "data-my-custom-value");
        assert_eq!(camel_to_data_attr("simple"), "data-simple");
    }

    #[test]
    fn test_data_attr_to_camel() {
        assert_eq!(data_attr_to_camel("data-user-id"), "userId");
        assert_eq!(data_attr_to_camel("data-my-custom-value"), "myCustomValue");
        assert_eq!(data_attr_to_camel("data-simple"), "simple");
    }
}
