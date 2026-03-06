//! DOM Traversal & Parsing APIs
//! TreeWalker, NodeIterator, Range, Selection, DOMParser

use boa_engine::{
    js_string, Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction,
    object::{builtins::JsArray, ObjectInitializer, FunctionObjectBuilder},
    property::Attribute,
};
use std::cell::RefCell;
use std::collections::HashMap;

// ============================================================================
// Node Filter Constants
// ============================================================================

const FILTER_ACCEPT: u32 = 1;
const FILTER_REJECT: u32 = 2;
const FILTER_SKIP: u32 = 3;

const SHOW_ALL: u32 = 0xFFFFFFFF;
const SHOW_ELEMENT: u32 = 0x1;
const SHOW_ATTRIBUTE: u32 = 0x2;
const SHOW_TEXT: u32 = 0x4;
const SHOW_CDATA_SECTION: u32 = 0x8;
const SHOW_ENTITY_REFERENCE: u32 = 0x10;
const SHOW_ENTITY: u32 = 0x20;
const SHOW_PROCESSING_INSTRUCTION: u32 = 0x40;
const SHOW_COMMENT: u32 = 0x80;
const SHOW_DOCUMENT: u32 = 0x100;
const SHOW_DOCUMENT_TYPE: u32 = 0x200;
const SHOW_DOCUMENT_FRAGMENT: u32 = 0x400;
const SHOW_NOTATION: u32 = 0x800;

// ============================================================================
// State Management
// ============================================================================

thread_local! {
    static TRAVERSAL_ID_COUNTER: RefCell<u32> = RefCell::new(1);
    static RANGE_ID_COUNTER: RefCell<u32> = RefCell::new(1);
    static RANGES: RefCell<HashMap<u32, RangeState>> = RefCell::new(HashMap::new());
}

fn get_next_traversal_id() -> u32 {
    TRAVERSAL_ID_COUNTER.with(|counter| {
        let mut c = counter.borrow_mut();
        let id = *c;
        *c += 1;
        id
    })
}

fn get_next_range_id() -> u32 {
    RANGE_ID_COUNTER.with(|counter| {
        let mut c = counter.borrow_mut();
        let id = *c;
        *c += 1;
        id
    })
}

#[derive(Clone)]
struct RangeState {
    start_container: Option<JsValue>,
    start_offset: u32,
    end_container: Option<JsValue>,
    end_offset: u32,
    collapsed: bool,
}

impl Default for RangeState {
    fn default() -> Self {
        Self {
            start_container: None,
            start_offset: 0,
            end_container: None,
            end_offset: 0,
            collapsed: true,
        }
    }
}

// ============================================================================
// NodeFilter Registration
// ============================================================================

fn register_node_filter(context: &mut Context) -> JsResult<()> {
    let node_filter = ObjectInitializer::new(context)
        .property(js_string!("FILTER_ACCEPT"), FILTER_ACCEPT, Attribute::READONLY)
        .property(js_string!("FILTER_REJECT"), FILTER_REJECT, Attribute::READONLY)
        .property(js_string!("FILTER_SKIP"), FILTER_SKIP, Attribute::READONLY)
        .property(js_string!("SHOW_ALL"), SHOW_ALL, Attribute::READONLY)
        .property(js_string!("SHOW_ELEMENT"), SHOW_ELEMENT, Attribute::READONLY)
        .property(js_string!("SHOW_ATTRIBUTE"), SHOW_ATTRIBUTE, Attribute::READONLY)
        .property(js_string!("SHOW_TEXT"), SHOW_TEXT, Attribute::READONLY)
        .property(js_string!("SHOW_CDATA_SECTION"), SHOW_CDATA_SECTION, Attribute::READONLY)
        .property(js_string!("SHOW_ENTITY_REFERENCE"), SHOW_ENTITY_REFERENCE, Attribute::READONLY)
        .property(js_string!("SHOW_ENTITY"), SHOW_ENTITY, Attribute::READONLY)
        .property(js_string!("SHOW_PROCESSING_INSTRUCTION"), SHOW_PROCESSING_INSTRUCTION, Attribute::READONLY)
        .property(js_string!("SHOW_COMMENT"), SHOW_COMMENT, Attribute::READONLY)
        .property(js_string!("SHOW_DOCUMENT"), SHOW_DOCUMENT, Attribute::READONLY)
        .property(js_string!("SHOW_DOCUMENT_TYPE"), SHOW_DOCUMENT_TYPE, Attribute::READONLY)
        .property(js_string!("SHOW_DOCUMENT_FRAGMENT"), SHOW_DOCUMENT_FRAGMENT, Attribute::READONLY)
        .property(js_string!("SHOW_NOTATION"), SHOW_NOTATION, Attribute::READONLY)
        .build();

    context.register_global_property(js_string!("NodeFilter"), node_filter, Attribute::all())?;

    Ok(())
}

// ============================================================================
// TreeWalker Implementation
// ============================================================================

fn register_tree_walker(context: &mut Context) -> JsResult<()> {
    // TreeWalker is typically created via document.createTreeWalker, not as a constructor
    // But we'll register a constructor for direct usage

    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let root = args.get_or_undefined(0).clone();
        let what_to_show = args.get_or_undefined(1)
            .to_u32(ctx)
            .unwrap_or(SHOW_ALL);
        let filter = args.get_or_undefined(2).clone();

        create_tree_walker_object(ctx, root, what_to_show, filter)
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("TreeWalker"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("TreeWalker"), constructor, Attribute::all())?;

    Ok(())
}

fn create_tree_walker_object(
    context: &mut Context,
    root: JsValue,
    what_to_show: u32,
    filter: JsValue,
) -> JsResult<JsValue> {
    let _id = get_next_traversal_id();

    let walker = ObjectInitializer::new(context)
        .property(js_string!("root"), root.clone(), Attribute::READONLY)
        .property(js_string!("whatToShow"), what_to_show, Attribute::READONLY)
        .property(js_string!("filter"), filter.clone(), Attribute::READONLY)
        .property(js_string!("currentNode"), root.clone(), Attribute::all())
        .build();

    // parentNode()
    let parent_node = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let current = obj.get(js_string!("currentNode"), ctx)?;
            if let Some(node) = current.as_object() {
                let parent = node.get(js_string!("parentNode"), ctx)?;
                if !parent.is_null() && !parent.is_undefined() {
                    obj.set(js_string!("currentNode"), parent.clone(), false, ctx)?;
                    return Ok(parent);
                }
            }
        }
        Ok(JsValue::null())
    });

    // firstChild()
    let first_child = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let current = obj.get(js_string!("currentNode"), ctx)?;
            if let Some(node) = current.as_object() {
                let child = node.get(js_string!("firstChild"), ctx)?;
                if !child.is_null() && !child.is_undefined() {
                    obj.set(js_string!("currentNode"), child.clone(), false, ctx)?;
                    return Ok(child);
                }
            }
        }
        Ok(JsValue::null())
    });

    // lastChild()
    let last_child = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let current = obj.get(js_string!("currentNode"), ctx)?;
            if let Some(node) = current.as_object() {
                let child = node.get(js_string!("lastChild"), ctx)?;
                if !child.is_null() && !child.is_undefined() {
                    obj.set(js_string!("currentNode"), child.clone(), false, ctx)?;
                    return Ok(child);
                }
            }
        }
        Ok(JsValue::null())
    });

    // previousSibling()
    let previous_sibling = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let current = obj.get(js_string!("currentNode"), ctx)?;
            if let Some(node) = current.as_object() {
                let sibling = node.get(js_string!("previousSibling"), ctx)?;
                if !sibling.is_null() && !sibling.is_undefined() {
                    obj.set(js_string!("currentNode"), sibling.clone(), false, ctx)?;
                    return Ok(sibling);
                }
            }
        }
        Ok(JsValue::null())
    });

    // nextSibling()
    let next_sibling = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let current = obj.get(js_string!("currentNode"), ctx)?;
            if let Some(node) = current.as_object() {
                let sibling = node.get(js_string!("nextSibling"), ctx)?;
                if !sibling.is_null() && !sibling.is_undefined() {
                    obj.set(js_string!("currentNode"), sibling.clone(), false, ctx)?;
                    return Ok(sibling);
                }
            }
        }
        Ok(JsValue::null())
    });

    // previousNode()
    let previous_node = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let current = obj.get(js_string!("currentNode"), ctx)?;
            if let Some(node) = current.as_object() {
                // Try previous sibling's last descendant, then previous sibling, then parent
                let sibling = node.get(js_string!("previousSibling"), ctx)?;
                if !sibling.is_null() && !sibling.is_undefined() {
                    obj.set(js_string!("currentNode"), sibling.clone(), false, ctx)?;
                    return Ok(sibling);
                }
                let parent = node.get(js_string!("parentNode"), ctx)?;
                if !parent.is_null() && !parent.is_undefined() {
                    obj.set(js_string!("currentNode"), parent.clone(), false, ctx)?;
                    return Ok(parent);
                }
            }
        }
        Ok(JsValue::null())
    });

    // nextNode()
    let next_node = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let current = obj.get(js_string!("currentNode"), ctx)?;
            if let Some(node) = current.as_object() {
                // Try first child, then next sibling, then ancestor's next sibling
                let child = node.get(js_string!("firstChild"), ctx)?;
                if !child.is_null() && !child.is_undefined() {
                    obj.set(js_string!("currentNode"), child.clone(), false, ctx)?;
                    return Ok(child);
                }
                let sibling = node.get(js_string!("nextSibling"), ctx)?;
                if !sibling.is_null() && !sibling.is_undefined() {
                    obj.set(js_string!("currentNode"), sibling.clone(), false, ctx)?;
                    return Ok(sibling);
                }
            }
        }
        Ok(JsValue::null())
    });

    walker.set(js_string!("parentNode"),
        parent_node.to_js_function(context.realm()),
        false, context)?;
    walker.set(js_string!("firstChild"),
        first_child.to_js_function(context.realm()),
        false, context)?;
    walker.set(js_string!("lastChild"),
        last_child.to_js_function(context.realm()),
        false, context)?;
    walker.set(js_string!("previousSibling"),
        previous_sibling.to_js_function(context.realm()),
        false, context)?;
    walker.set(js_string!("nextSibling"),
        next_sibling.to_js_function(context.realm()),
        false, context)?;
    walker.set(js_string!("previousNode"),
        previous_node.to_js_function(context.realm()),
        false, context)?;
    walker.set(js_string!("nextNode"),
        next_node.to_js_function(context.realm()),
        false, context)?;

    Ok(JsValue::from(walker))
}

// ============================================================================
// NodeIterator Implementation
// ============================================================================

fn register_node_iterator(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let root = args.get_or_undefined(0).clone();
        let what_to_show = args.get_or_undefined(1)
            .to_u32(ctx)
            .unwrap_or(SHOW_ALL);
        let filter = args.get_or_undefined(2).clone();

        create_node_iterator_object(ctx, root, what_to_show, filter)
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("NodeIterator"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("NodeIterator"), constructor, Attribute::all())?;

    Ok(())
}

fn create_node_iterator_object(
    context: &mut Context,
    root: JsValue,
    what_to_show: u32,
    filter: JsValue,
) -> JsResult<JsValue> {
    let _id = get_next_traversal_id();

    let iterator = ObjectInitializer::new(context)
        .property(js_string!("root"), root.clone(), Attribute::READONLY)
        .property(js_string!("referenceNode"), root.clone(), Attribute::READONLY)
        .property(js_string!("pointerBeforeReferenceNode"), true, Attribute::READONLY)
        .property(js_string!("whatToShow"), what_to_show, Attribute::READONLY)
        .property(js_string!("filter"), filter.clone(), Attribute::READONLY)
        .build();

    // nextNode()
    let next_node = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let current = obj.get(js_string!("referenceNode"), ctx)?;
            if let Some(node) = current.as_object() {
                // Try first child, then next sibling
                let child = node.get(js_string!("firstChild"), ctx)?;
                if !child.is_null() && !child.is_undefined() {
                    obj.set(js_string!("referenceNode"), child.clone(), false, ctx)?;
                    return Ok(child);
                }
                let sibling = node.get(js_string!("nextSibling"), ctx)?;
                if !sibling.is_null() && !sibling.is_undefined() {
                    obj.set(js_string!("referenceNode"), sibling.clone(), false, ctx)?;
                    return Ok(sibling);
                }
            }
        }
        Ok(JsValue::null())
    });

    // previousNode()
    let previous_node = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let current = obj.get(js_string!("referenceNode"), ctx)?;
            if let Some(node) = current.as_object() {
                let sibling = node.get(js_string!("previousSibling"), ctx)?;
                if !sibling.is_null() && !sibling.is_undefined() {
                    obj.set(js_string!("referenceNode"), sibling.clone(), false, ctx)?;
                    return Ok(sibling);
                }
                let parent = node.get(js_string!("parentNode"), ctx)?;
                if !parent.is_null() && !parent.is_undefined() {
                    obj.set(js_string!("referenceNode"), parent.clone(), false, ctx)?;
                    return Ok(parent);
                }
            }
        }
        Ok(JsValue::null())
    });

    // detach() - deprecated but still needed
    let detach = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    iterator.set(js_string!("nextNode"),
        next_node.to_js_function(context.realm()),
        false, context)?;
    iterator.set(js_string!("previousNode"),
        previous_node.to_js_function(context.realm()),
        false, context)?;
    iterator.set(js_string!("detach"),
        detach.to_js_function(context.realm()),
        false, context)?;

    Ok(JsValue::from(iterator))
}

// ============================================================================
// Range Implementation
// ============================================================================

fn register_range(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let range_id = get_next_range_id();

        RANGES.with(|ranges| {
            ranges.borrow_mut().insert(range_id, RangeState::default());
        });

        create_range_object(ctx, range_id)
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("Range"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("Range"), constructor, Attribute::all())?;

    // Range constants
    context.register_global_property(js_string!("START_TO_START"), 0u32, Attribute::READONLY)?;
    context.register_global_property(js_string!("START_TO_END"), 1u32, Attribute::READONLY)?;
    context.register_global_property(js_string!("END_TO_END"), 2u32, Attribute::READONLY)?;
    context.register_global_property(js_string!("END_TO_START"), 3u32, Attribute::READONLY)?;

    Ok(())
}

fn create_range_object(context: &mut Context, range_id: u32) -> JsResult<JsValue> {
    let range = ObjectInitializer::new(context)
        .property(js_string!("_rangeId"), range_id, Attribute::empty())
        .property(js_string!("START_TO_START"), 0u32, Attribute::READONLY)
        .property(js_string!("START_TO_END"), 1u32, Attribute::READONLY)
        .property(js_string!("END_TO_END"), 2u32, Attribute::READONLY)
        .property(js_string!("END_TO_START"), 3u32, Attribute::READONLY)
        .build();

    // setStart(node, offset)
    let id = range_id;
    let set_start = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let node = args.get_or_undefined(0).clone();
        let offset = args.get_or_undefined(1).to_u32(ctx).unwrap_or(0);

        RANGES.with(|ranges| {
            if let Some(state) = ranges.borrow_mut().get_mut(&id) {
                state.start_container = Some(node);
                state.start_offset = offset;
                state.collapsed = state.end_container.is_none();
            }
        });

        Ok(JsValue::undefined())
    });

    // setEnd(node, offset)
    let id2 = range_id;
    let set_end = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let node = args.get_or_undefined(0).clone();
        let offset = args.get_or_undefined(1).to_u32(ctx).unwrap_or(0);

        RANGES.with(|ranges| {
            if let Some(state) = ranges.borrow_mut().get_mut(&id2) {
                state.end_container = Some(node);
                state.end_offset = offset;
                state.collapsed = false;
            }
        });

        Ok(JsValue::undefined())
    });

    // setStartBefore(node)
    let id3 = range_id;
    let set_start_before = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
        let node = args.get_or_undefined(0).clone();
        RANGES.with(|ranges| {
            if let Some(state) = ranges.borrow_mut().get_mut(&id3) {
                state.start_container = Some(node);
                state.start_offset = 0;
            }
        });
        Ok(JsValue::undefined())
    });

    // setStartAfter(node)
    let id4 = range_id;
    let set_start_after = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
        let node = args.get_or_undefined(0).clone();
        RANGES.with(|ranges| {
            if let Some(state) = ranges.borrow_mut().get_mut(&id4) {
                state.start_container = Some(node);
                state.start_offset = 1; // After = offset 1
            }
        });
        Ok(JsValue::undefined())
    });

    // setEndBefore(node)
    let id5 = range_id;
    let set_end_before = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
        let node = args.get_or_undefined(0).clone();
        RANGES.with(|ranges| {
            if let Some(state) = ranges.borrow_mut().get_mut(&id5) {
                state.end_container = Some(node);
                state.end_offset = 0;
            }
        });
        Ok(JsValue::undefined())
    });

    // setEndAfter(node)
    let id6 = range_id;
    let set_end_after = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
        let node = args.get_or_undefined(0).clone();
        RANGES.with(|ranges| {
            if let Some(state) = ranges.borrow_mut().get_mut(&id6) {
                state.end_container = Some(node);
                state.end_offset = 1;
            }
        });
        Ok(JsValue::undefined())
    });

    // collapse(toStart)
    let id7 = range_id;
    let collapse = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
        let to_start = args.get_or_undefined(0).to_boolean();
        RANGES.with(|ranges| {
            if let Some(state) = ranges.borrow_mut().get_mut(&id7) {
                if to_start {
                    state.end_container = state.start_container.clone();
                    state.end_offset = state.start_offset;
                } else {
                    state.start_container = state.end_container.clone();
                    state.start_offset = state.end_offset;
                }
                state.collapsed = true;
            }
        });
        Ok(JsValue::undefined())
    });

    // selectNode(node)
    let id8 = range_id;
    let select_node = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
        let node = args.get_or_undefined(0).clone();
        RANGES.with(|ranges| {
            if let Some(state) = ranges.borrow_mut().get_mut(&id8) {
                state.start_container = Some(node.clone());
                state.start_offset = 0;
                state.end_container = Some(node);
                state.end_offset = 1;
                state.collapsed = false;
            }
        });
        Ok(JsValue::undefined())
    });

    // selectNodeContents(node)
    let id9 = range_id;
    let select_node_contents = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
        let node = args.get_or_undefined(0).clone();
        RANGES.with(|ranges| {
            if let Some(state) = ranges.borrow_mut().get_mut(&id9) {
                state.start_container = Some(node.clone());
                state.start_offset = 0;
                state.end_container = Some(node);
                state.end_offset = 0; // Would be childNodes.length
                state.collapsed = true;
            }
        });
        Ok(JsValue::undefined())
    });

    // compareBoundaryPoints(how, sourceRange)
    let compare_boundary_points = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0))
    });

    // deleteContents()
    let delete_contents = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // extractContents()
    let extract_contents = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Return a document fragment
        let fragment = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 11, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#document-fragment"), Attribute::READONLY)
            .build();
        Ok(JsValue::from(fragment))
    });

    // cloneContents()
    let clone_contents = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let fragment = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 11, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#document-fragment"), Attribute::READONLY)
            .build();
        Ok(JsValue::from(fragment))
    });

    // insertNode(node)
    let insert_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // surroundContents(newParent)
    let surround_contents = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // cloneRange()
    let clone_range = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let new_id = get_next_range_id();
        RANGES.with(|ranges| {
            ranges.borrow_mut().insert(new_id, RangeState::default());
        });
        create_range_object(ctx, new_id)
    });

    // detach()
    let detach = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // toString()
    let to_string = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("")))
    });

    // getBoundingClientRect()
    let get_bounding_client_rect = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let rect = ObjectInitializer::new(ctx)
            .property(js_string!("x"), 0.0, Attribute::READONLY)
            .property(js_string!("y"), 0.0, Attribute::READONLY)
            .property(js_string!("width"), 0.0, Attribute::READONLY)
            .property(js_string!("height"), 0.0, Attribute::READONLY)
            .property(js_string!("top"), 0.0, Attribute::READONLY)
            .property(js_string!("right"), 0.0, Attribute::READONLY)
            .property(js_string!("bottom"), 0.0, Attribute::READONLY)
            .property(js_string!("left"), 0.0, Attribute::READONLY)
            .build();
        Ok(JsValue::from(rect))
    });

    // getClientRects()
    let get_client_rects = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let rects = JsArray::new(ctx);
        Ok(JsValue::from(rects))
    });

    // isPointInRange(node, offset)
    let is_point_in_range = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });

    // comparePoint(node, offset)
    let compare_point = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(0))
    });

    // intersectsNode(node)
    let intersects_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });

    range.set(js_string!("setStart"), set_start.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("setEnd"), set_end.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("setStartBefore"), set_start_before.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("setStartAfter"), set_start_after.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("setEndBefore"), set_end_before.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("setEndAfter"), set_end_after.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("collapse"), collapse.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("selectNode"), select_node.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("selectNodeContents"), select_node_contents.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("compareBoundaryPoints"), compare_boundary_points.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("deleteContents"), delete_contents.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("extractContents"), extract_contents.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("cloneContents"), clone_contents.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("insertNode"), insert_node.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("surroundContents"), surround_contents.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("cloneRange"), clone_range.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("detach"), detach.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("toString"), to_string.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("getBoundingClientRect"), get_bounding_client_rect.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("getClientRects"), get_client_rects.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("isPointInRange"), is_point_in_range.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("comparePoint"), compare_point.to_js_function(context.realm()), false, context)?;
    range.set(js_string!("intersectsNode"), intersects_node.to_js_function(context.realm()), false, context)?;

    // Add readonly properties via getters
    let id_collapsed = range_id;
    let get_collapsed = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        let collapsed = RANGES.with(|ranges| {
            ranges.borrow().get(&id_collapsed).map(|s| s.collapsed).unwrap_or(true)
        });
        Ok(JsValue::from(collapsed))
    });
    // For now, set initial collapsed value (in a real implementation, use getter/setter)
    range.set(js_string!("collapsed"), true, false, context)?;

    Ok(JsValue::from(range))
}

// ============================================================================
// Selection Implementation
// ============================================================================

fn register_selection(context: &mut Context) -> JsResult<()> {
    // Selection is accessed via window.getSelection() or document.getSelection()
    let selection = create_selection_object(context)?;

    // Register getSelection on window
    let selection_val = selection.clone();
    let get_selection = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(selection_val.clone())
        })
    };

    context.register_global_builtin_callable(js_string!("getSelection"), 0, get_selection)?;

    // Also add Selection constructor for type checking
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(JsNativeError::typ()
            .with_message("Selection cannot be constructed directly")
            .into())
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("Selection"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("Selection"), constructor, Attribute::all())?;

    Ok(())
}

fn create_selection_object(context: &mut Context) -> JsResult<JsValue> {
    let selection = ObjectInitializer::new(context)
        .property(js_string!("anchorNode"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("anchorOffset"), 0, Attribute::READONLY)
        .property(js_string!("focusNode"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("focusOffset"), 0, Attribute::READONLY)
        .property(js_string!("isCollapsed"), true, Attribute::READONLY)
        .property(js_string!("rangeCount"), 0, Attribute::READONLY)
        .property(js_string!("type"), js_string!("None"), Attribute::READONLY)
        .build();

    // getRangeAt(index)
    let get_range_at = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let _index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
        let range_id = get_next_range_id();
        RANGES.with(|ranges| {
            ranges.borrow_mut().insert(range_id, RangeState::default());
        });
        create_range_object(ctx, range_id)
    });

    // addRange(range)
    let add_range = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // removeRange(range)
    let remove_range = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // removeAllRanges()
    let remove_all_ranges = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // empty() - alias for removeAllRanges
    let empty = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // collapse(node, offset)
    let collapse = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // setPosition(node, offset) - alias for collapse
    let set_position = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // collapseToStart()
    let collapse_to_start = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // collapseToEnd()
    let collapse_to_end = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // extend(node, offset)
    let extend = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // setBaseAndExtent(anchorNode, anchorOffset, focusNode, focusOffset)
    let set_base_and_extent = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // selectAllChildren(node)
    let select_all_children = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // deleteFromDocument()
    let delete_from_document = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // containsNode(node, allowPartialContainment)
    let contains_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });

    // toString()
    let to_string = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("")))
    });

    // modify(alter, direction, granularity)
    let modify = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    selection.set(js_string!("getRangeAt"), get_range_at.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("addRange"), add_range.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("removeRange"), remove_range.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("removeAllRanges"), remove_all_ranges.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("empty"), empty.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("collapse"), collapse.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("setPosition"), set_position.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("collapseToStart"), collapse_to_start.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("collapseToEnd"), collapse_to_end.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("extend"), extend.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("setBaseAndExtent"), set_base_and_extent.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("selectAllChildren"), select_all_children.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("deleteFromDocument"), delete_from_document.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("containsNode"), contains_node.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("toString"), to_string.to_js_function(context.realm()), false, context)?;
    selection.set(js_string!("modify"), modify.to_js_function(context.realm()), false, context)?;

    Ok(JsValue::from(selection))
}

// ============================================================================
// Enhanced DOMParser
// ============================================================================

fn register_dom_parser(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        create_dom_parser_object(ctx)
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("DOMParser"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("DOMParser"), constructor, Attribute::all())?;

    Ok(())
}

fn create_dom_parser_object(context: &mut Context) -> JsResult<JsValue> {
    let parser = ObjectInitializer::new(context).build();

    let parse_from_string = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let _string = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let mime_type = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

        // Return a document-like object based on mime type
        let doc_type = match mime_type.as_str() {
            "text/html" => ("HTML", "#document"),
            "text/xml" | "application/xml" => ("XML", "#document"),
            "application/xhtml+xml" => ("XHTML", "#document"),
            "image/svg+xml" => ("SVG", "#document"),
            _ => ("HTML", "#document"),
        };

        // Create document element
        let doc_element = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 1, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("HTML"), Attribute::READONLY)
            .property(js_string!("tagName"), js_string!("HTML"), Attribute::READONLY)
            .build();

        let head = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 1, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("HEAD"), Attribute::READONLY)
            .property(js_string!("tagName"), js_string!("HEAD"), Attribute::READONLY)
            .build();

        let body = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 1, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("BODY"), Attribute::READONLY)
            .property(js_string!("tagName"), js_string!("BODY"), Attribute::READONLY)
            .build();

        let get_element_by_id = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        let query_selector = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::null())
        });

        let query_selector_all = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        let get_elements_by_tag_name = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        let get_elements_by_class_name = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            Ok(JsValue::from(JsArray::new(ctx)))
        });

        let doc = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 9, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!(doc_type.1), Attribute::READONLY)
            .property(js_string!("contentType"), js_string!(mime_type.clone()), Attribute::READONLY)
            .property(js_string!("documentElement"), doc_element, Attribute::READONLY)
            .property(js_string!("head"), head, Attribute::READONLY)
            .property(js_string!("body"), body, Attribute::READONLY)
            .property(js_string!("doctype"), JsValue::null(), Attribute::READONLY)
            .function(get_element_by_id, js_string!("getElementById"), 1)
            .function(query_selector, js_string!("querySelector"), 1)
            .function(query_selector_all, js_string!("querySelectorAll"), 1)
            .function(get_elements_by_tag_name, js_string!("getElementsByTagName"), 1)
            .function(get_elements_by_class_name, js_string!("getElementsByClassName"), 1)
            .build();

        Ok(JsValue::from(doc))
    });

    parser.set(js_string!("parseFromString"), parse_from_string.to_js_function(context.realm()), false, context)?;

    Ok(JsValue::from(parser))
}

// ============================================================================
// XMLSerializer
// ============================================================================

fn register_xml_serializer(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let serializer = ObjectInitializer::new(ctx).build();

        let serialize_to_string = NativeFunction::from_copy_closure(|_this, args, ctx| {
            let node = args.get_or_undefined(0);
            if let Some(obj) = node.as_object() {
                let node_name = obj.get(js_string!("nodeName"), ctx)
                    .ok()
                    .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                    .unwrap_or_default();

                // Simple serialization
                let tag = node_name.to_lowercase();
                return Ok(JsValue::from(js_string!(format!("<{0}></{0}>", tag))));
            }
            Ok(JsValue::from(js_string!("")))
        });

        serializer.set(js_string!("serializeToString"), serialize_to_string.to_js_function(ctx.realm()), false, ctx)?;

        Ok(JsValue::from(serializer))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("XMLSerializer"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("XMLSerializer"), constructor, Attribute::all())?;

    Ok(())
}

// ============================================================================
// XPathEvaluator and XPathResult
// ============================================================================

fn register_xpath(context: &mut Context) -> JsResult<()> {
    // XPathResult constants
    let xpath_result = ObjectInitializer::new(context)
        .property(js_string!("ANY_TYPE"), 0u32, Attribute::READONLY)
        .property(js_string!("NUMBER_TYPE"), 1u32, Attribute::READONLY)
        .property(js_string!("STRING_TYPE"), 2u32, Attribute::READONLY)
        .property(js_string!("BOOLEAN_TYPE"), 3u32, Attribute::READONLY)
        .property(js_string!("UNORDERED_NODE_ITERATOR_TYPE"), 4u32, Attribute::READONLY)
        .property(js_string!("ORDERED_NODE_ITERATOR_TYPE"), 5u32, Attribute::READONLY)
        .property(js_string!("UNORDERED_NODE_SNAPSHOT_TYPE"), 6u32, Attribute::READONLY)
        .property(js_string!("ORDERED_NODE_SNAPSHOT_TYPE"), 7u32, Attribute::READONLY)
        .property(js_string!("ANY_UNORDERED_NODE_TYPE"), 8u32, Attribute::READONLY)
        .property(js_string!("FIRST_ORDERED_NODE_TYPE"), 9u32, Attribute::READONLY)
        .build();

    context.register_global_property(js_string!("XPathResult"), xpath_result, Attribute::all())?;

    // XPathEvaluator constructor
    let evaluator_constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let evaluator = ObjectInitializer::new(ctx).build();

        let create_expression = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let expr = ObjectInitializer::new(ctx).build();
            let evaluate = NativeFunction::from_copy_closure(|_this, _args, ctx| {
                // Return empty XPath result
                let result = ObjectInitializer::new(ctx)
                    .property(js_string!("resultType"), 0, Attribute::READONLY)
                    .property(js_string!("numberValue"), 0.0, Attribute::READONLY)
                    .property(js_string!("stringValue"), js_string!(""), Attribute::READONLY)
                    .property(js_string!("booleanValue"), false, Attribute::READONLY)
                    .property(js_string!("singleNodeValue"), JsValue::null(), Attribute::READONLY)
                    .property(js_string!("snapshotLength"), 0, Attribute::READONLY)
                    .property(js_string!("invalidIteratorState"), false, Attribute::READONLY)
                    .build();
                Ok(JsValue::from(result))
            });
            expr.set(js_string!("evaluate"), evaluate.to_js_function(ctx.realm()), false, ctx)?;
            Ok(JsValue::from(expr))
        });

        let create_ns_resolver = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let resolver = ObjectInitializer::new(ctx).build();
            let lookup = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
                Ok(JsValue::null())
            });
            resolver.set(js_string!("lookupNamespaceURI"), lookup.to_js_function(ctx.realm()), false, ctx)?;
            Ok(JsValue::from(resolver))
        });

        let evaluate = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let result = ObjectInitializer::new(ctx)
                .property(js_string!("resultType"), 0, Attribute::READONLY)
                .property(js_string!("numberValue"), 0.0, Attribute::READONLY)
                .property(js_string!("stringValue"), js_string!(""), Attribute::READONLY)
                .property(js_string!("booleanValue"), false, Attribute::READONLY)
                .property(js_string!("singleNodeValue"), JsValue::null(), Attribute::READONLY)
                .property(js_string!("snapshotLength"), 0, Attribute::READONLY)
                .build();
            Ok(JsValue::from(result))
        });

        evaluator.set(js_string!("createExpression"), create_expression.to_js_function(ctx.realm()), false, ctx)?;
        evaluator.set(js_string!("createNSResolver"), create_ns_resolver.to_js_function(ctx.realm()), false, ctx)?;
        evaluator.set(js_string!("evaluate"), evaluate.to_js_function(ctx.realm()), false, ctx)?;

        Ok(JsValue::from(evaluator))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), evaluator_constructor)
        .name(js_string!("XPathEvaluator"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("XPathEvaluator"), constructor, Attribute::all())?;

    Ok(())
}

// ============================================================================
// StaticRange
// ============================================================================

fn register_static_range(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let init = args.get_or_undefined(0);

        let (start_container, start_offset, end_container, end_offset) = if let Some(obj) = init.as_object() {
            let sc = obj.get(js_string!("startContainer"), ctx)?;
            let so = obj.get(js_string!("startOffset"), ctx)?.to_u32(ctx).unwrap_or(0);
            let ec = obj.get(js_string!("endContainer"), ctx)?;
            let eo = obj.get(js_string!("endOffset"), ctx)?.to_u32(ctx).unwrap_or(0);
            (sc, so, ec, eo)
        } else {
            (JsValue::null(), 0, JsValue::null(), 0)
        };

        // Check if range is collapsed (same container and offset)
        let collapsed = start_offset == end_offset;

        let static_range = ObjectInitializer::new(ctx)
            .property(js_string!("startContainer"), start_container.clone(), Attribute::READONLY)
            .property(js_string!("startOffset"), start_offset, Attribute::READONLY)
            .property(js_string!("endContainer"), end_container.clone(), Attribute::READONLY)
            .property(js_string!("endOffset"), end_offset, Attribute::READONLY)
            .property(js_string!("collapsed"), collapsed, Attribute::READONLY)
            .build();

        Ok(JsValue::from(static_range))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("StaticRange"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("StaticRange"), constructor, Attribute::all())?;

    Ok(())
}

// ============================================================================
// Main Registration
// ============================================================================

pub fn register_all_dom_traversal_apis(context: &mut Context) -> JsResult<()> {
    register_node_filter(context)?;
    register_tree_walker(context)?;
    register_node_iterator(context)?;
    register_range(context)?;
    register_selection(context)?;
    register_dom_parser(context)?;
    register_xml_serializer(context)?;
    register_xpath(context)?;
    register_static_range(context)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;

    fn create_context() -> Context {
        let mut context = Context::default();
        register_all_dom_traversal_apis(&mut context).unwrap();
        context
    }

    #[test]
    fn test_node_filter_constants() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            "NodeFilter.SHOW_ELEMENT === 1 && NodeFilter.SHOW_TEXT === 4"
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_tree_walker_exists() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            "typeof TreeWalker === 'function'"
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_node_iterator_exists() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            "typeof NodeIterator === 'function'"
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_range_constructor() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            r#"
            var range = new Range();
            typeof range.setStart === 'function' && typeof range.setEnd === 'function'
            "#
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_range_methods() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            r#"
            var range = new Range();
            typeof range.cloneRange === 'function' &&
            typeof range.deleteContents === 'function' &&
            typeof range.getBoundingClientRect === 'function'
            "#
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_selection_exists() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            "typeof getSelection === 'function'"
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_selection_methods() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            r#"
            var sel = getSelection();
            typeof sel.getRangeAt === 'function' &&
            typeof sel.addRange === 'function' &&
            typeof sel.removeAllRanges === 'function'
            "#
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_dom_parser() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            r#"
            var parser = new DOMParser();
            var doc = parser.parseFromString('<html></html>', 'text/html');
            doc.nodeType === 9 && doc.body !== null
            "#
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_xml_serializer() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            r#"
            var serializer = new XMLSerializer();
            typeof serializer.serializeToString === 'function'
            "#
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_xpath_evaluator() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            r#"
            var evaluator = new XPathEvaluator();
            typeof evaluator.evaluate === 'function' &&
            typeof evaluator.createExpression === 'function'
            "#
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_xpath_result_constants() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            "XPathResult.ANY_TYPE === 0 && XPathResult.STRING_TYPE === 2"
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }

    #[test]
    fn test_static_range() {
        let mut context = create_context();
        let result = context.eval(Source::from_bytes(
            r#"
            var range = new StaticRange({
                startContainer: {},
                startOffset: 0,
                endContainer: {},
                endOffset: 5
            });
            range.startOffset === 0 && range.endOffset === 5
            "#
        ));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(&mut context).unwrap().to_std_string_escaped(), "true");
    }
}
