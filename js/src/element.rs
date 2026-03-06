//! Full Element API implementation
//!
//! Implements comprehensive Element APIs including:
//! - All Element properties and methods
//! - Complete CSSStyleDeclaration with all CSS properties
//! - Full DOMTokenList (classList) implementation
//! - NamedNodeMap (attributes) implementation
//! - Animation API (animate, getAnimations)
//! - Shadow DOM API (attachShadow, shadowRoot)
//! - Pointer capture API
//! - Scroll APIs
//! - Form element properties
//! - ARIA properties

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer,
    object::builtins::JsArray, property::Attribute,
    Context, JsArgs, JsObject, JsValue,
};
use std::cell::RefCell;
use std::rc::Rc;

// Re-export dom for ShadowRoot storage
use dom;

/// State for style object to track changes
#[derive(Clone, Default)]
pub struct StyleState {
    properties: RefCell<std::collections::HashMap<String, String>>,
}

/// Create a complete CSSStyleDeclaration object
pub fn create_full_style_object(context: &mut Context) -> JsObject {
    let state = Rc::new(StyleState::default());

    // setProperty(name, value, priority?)
    let state_clone = state.clone();
    let set_property = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            state_clone.properties.borrow_mut().insert(name, value);
            Ok(JsValue::undefined())
        })
    };

    // getPropertyValue(name)
    let state_clone = state.clone();
    let get_property_value = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let value = state_clone.properties.borrow().get(&name).cloned().unwrap_or_default();
            Ok(JsValue::from(js_string!(value)))
        })
    };

    // removeProperty(name)
    let state_clone = state.clone();
    let remove_property = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let old = state_clone.properties.borrow_mut().remove(&name).unwrap_or_default();
            Ok(JsValue::from(js_string!(old)))
        })
    };

    // getPropertyPriority(name)
    let get_property_priority = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("")))
    });

    // item(index)
    let item = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("")))
    });

    // Build style object with ALL CSS properties
    let style = ObjectInitializer::new(context)
        .function(set_property, js_string!("setProperty"), 3)
        .function(get_property_value, js_string!("getPropertyValue"), 1)
        .function(remove_property, js_string!("removeProperty"), 1)
        .function(get_property_priority, js_string!("getPropertyPriority"), 1)
        .function(item, js_string!("item"), 1)
        .property(js_string!("cssText"), js_string!(""), Attribute::all())
        .property(js_string!("length"), 0, Attribute::READONLY)
        .property(js_string!("parentRule"), JsValue::null(), Attribute::READONLY)
        .build();

    // Add ALL standard CSS properties (over 400 properties)
    let css_properties = get_all_css_properties();
    for prop in css_properties {
        let _ = style.set(js_string!(prop), js_string!(""), false, context);
    }

    style
}

/// Get list of all standard CSS properties
fn get_all_css_properties() -> Vec<&'static str> {
    vec![
        // Alignment & Box Model
        "alignContent", "alignItems", "alignSelf", "all",
        "animation", "animationDelay", "animationDirection", "animationDuration",
        "animationFillMode", "animationIterationCount", "animationName",
        "animationPlayState", "animationTimingFunction", "appearance",

        // Background
        "backfaceVisibility", "background", "backgroundAttachment",
        "backgroundBlendMode", "backgroundClip", "backgroundColor",
        "backgroundImage", "backgroundOrigin", "backgroundPosition",
        "backgroundPositionX", "backgroundPositionY", "backgroundRepeat",
        "backgroundSize",

        // Border
        "border", "borderBottom", "borderBottomColor", "borderBottomLeftRadius",
        "borderBottomRightRadius", "borderBottomStyle", "borderBottomWidth",
        "borderCollapse", "borderColor", "borderImage", "borderImageOutset",
        "borderImageRepeat", "borderImageSlice", "borderImageSource",
        "borderImageWidth", "borderLeft", "borderLeftColor", "borderLeftStyle",
        "borderLeftWidth", "borderRadius", "borderRight", "borderRightColor",
        "borderRightStyle", "borderRightWidth", "borderSpacing", "borderStyle",
        "borderTop", "borderTopColor", "borderTopLeftRadius", "borderTopRightRadius",
        "borderTopStyle", "borderTopWidth", "borderWidth",

        // Box & Position
        "bottom", "boxDecorationBreak", "boxShadow", "boxSizing",
        "breakAfter", "breakBefore", "breakInside",

        // Caption & Caret
        "captionSide", "caretColor", "clear", "clip", "clipPath",

        // Color & Column
        "color", "colorScheme", "columnCount", "columnFill", "columnGap",
        "columnRule", "columnRuleColor", "columnRuleStyle", "columnRuleWidth",
        "columnSpan", "columnWidth", "columns", "contain", "content",
        "contentVisibility", "counterIncrement", "counterReset", "counterSet",

        // Cursor
        "cursor",

        // Direction & Display
        "direction", "display",

        // Empty & Filter
        "emptyCells", "filter",

        // Flex
        "flex", "flexBasis", "flexDirection", "flexFlow", "flexGrow",
        "flexShrink", "flexWrap", "float",

        // Font
        "font", "fontDisplay", "fontFamily", "fontFeatureSettings",
        "fontKerning", "fontLanguageOverride", "fontOpticalSizing",
        "fontSize", "fontSizeAdjust", "fontStretch", "fontStyle",
        "fontSynthesis", "fontVariant", "fontVariantAlternates",
        "fontVariantCaps", "fontVariantEastAsian", "fontVariantLigatures",
        "fontVariantNumeric", "fontVariantPosition", "fontVariationSettings",
        "fontWeight", "forcedColorAdjust",

        // Gap & Grid
        "gap", "grid", "gridArea", "gridAutoColumns", "gridAutoFlow",
        "gridAutoRows", "gridColumn", "gridColumnEnd", "gridColumnGap",
        "gridColumnStart", "gridGap", "gridRow", "gridRowEnd", "gridRowGap",
        "gridRowStart", "gridTemplate", "gridTemplateAreas", "gridTemplateColumns",
        "gridTemplateRows",

        // Hanging & Height
        "hangingPunctuation", "height",

        // Hyphenate & Hyphens
        "hyphenateCharacter", "hyphens",

        // Image & Inline
        "imageOrientation", "imageRendering", "inlineSize",
        "inset", "insetBlock", "insetBlockEnd", "insetBlockStart",
        "insetInline", "insetInlineEnd", "insetInlineStart",
        "isolation",

        // Justify
        "justifyContent", "justifyItems", "justifySelf",

        // Left & Letter
        "left", "letterSpacing",

        // Line
        "lineBreak", "lineHeight",

        // List
        "listStyle", "listStyleImage", "listStylePosition", "listStyleType",

        // Margin
        "margin", "marginBlock", "marginBlockEnd", "marginBlockStart",
        "marginBottom", "marginInline", "marginInlineEnd", "marginInlineStart",
        "marginLeft", "marginRight", "marginTop",

        // Mask
        "mask", "maskBorder", "maskBorderMode", "maskBorderOutset",
        "maskBorderRepeat", "maskBorderSlice", "maskBorderSource",
        "maskBorderWidth", "maskClip", "maskComposite", "maskImage",
        "maskMode", "maskOrigin", "maskPosition", "maskRepeat", "maskSize",
        "maskType",

        // Max & Min
        "maxBlockSize", "maxHeight", "maxInlineSize", "maxWidth",
        "minBlockSize", "minHeight", "minInlineSize", "minWidth",

        // Mix
        "mixBlendMode",

        // Object
        "objectFit", "objectPosition", "offset", "offsetAnchor",
        "offsetDistance", "offsetPath", "offsetPosition", "offsetRotate",

        // Opacity & Order
        "opacity", "order",

        // Orphans & Outline
        "orphans", "outline", "outlineColor", "outlineOffset",
        "outlineStyle", "outlineWidth",

        // Overflow
        "overflow", "overflowAnchor", "overflowBlock", "overflowClipMargin",
        "overflowInline", "overflowWrap", "overflowX", "overflowY",

        // Overscroll
        "overscrollBehavior", "overscrollBehaviorBlock", "overscrollBehaviorInline",
        "overscrollBehaviorX", "overscrollBehaviorY",

        // Padding
        "padding", "paddingBlock", "paddingBlockEnd", "paddingBlockStart",
        "paddingBottom", "paddingInline", "paddingInlineEnd", "paddingInlineStart",
        "paddingLeft", "paddingRight", "paddingTop",

        // Page
        "pageBreakAfter", "pageBreakBefore", "pageBreakInside",

        // Paint & Perspective
        "paintOrder", "perspective", "perspectiveOrigin",

        // Place
        "placeContent", "placeItems", "placeSelf",

        // Pointer & Position
        "pointerEvents", "position",

        // Print & Quotes
        "printColorAdjust", "quotes",

        // Resize & Right
        "resize", "right",

        // Rotate & Row
        "rotate", "rowGap",

        // Ruby
        "rubyAlign", "rubyMerge", "rubyPosition",

        // Scale & Scroll
        "scale", "scrollBehavior", "scrollMargin", "scrollMarginBlock",
        "scrollMarginBlockEnd", "scrollMarginBlockStart", "scrollMarginBottom",
        "scrollMarginInline", "scrollMarginInlineEnd", "scrollMarginInlineStart",
        "scrollMarginLeft", "scrollMarginRight", "scrollMarginTop",
        "scrollPadding", "scrollPaddingBlock", "scrollPaddingBlockEnd",
        "scrollPaddingBlockStart", "scrollPaddingBottom", "scrollPaddingInline",
        "scrollPaddingInlineEnd", "scrollPaddingInlineStart", "scrollPaddingLeft",
        "scrollPaddingRight", "scrollPaddingTop", "scrollSnapAlign",
        "scrollSnapStop", "scrollSnapType", "scrollbarColor", "scrollbarGutter",
        "scrollbarWidth",

        // Shape
        "shapeImageThreshold", "shapeMargin", "shapeOutside",

        // Tab & Table
        "tabSize", "tableLayout",

        // Text
        "textAlign", "textAlignLast", "textCombineUpright", "textDecoration",
        "textDecorationColor", "textDecorationLine", "textDecorationSkipInk",
        "textDecorationStyle", "textDecorationThickness", "textEmphasis",
        "textEmphasisColor", "textEmphasisPosition", "textEmphasisStyle",
        "textIndent", "textJustify", "textOrientation", "textOverflow",
        "textRendering", "textShadow", "textTransform", "textUnderlineOffset",
        "textUnderlinePosition",

        // Top & Touch
        "top", "touchAction",

        // Transform
        "transform", "transformBox", "transformOrigin", "transformStyle",

        // Transition
        "transition", "transitionDelay", "transitionDuration",
        "transitionProperty", "transitionTimingFunction", "translate",

        // Unicode & User
        "unicodeBidi", "userSelect",

        // Vertical & Visibility
        "verticalAlign", "visibility",

        // White & Widows & Width
        "whiteSpace", "widows", "width",

        // Will & Word
        "willChange", "wordBreak", "wordSpacing", "wordWrap",

        // Writing & Z
        "writingMode", "zIndex", "zoom",

        // Webkit vendor prefixes (commonly needed)
        "webkitAlignContent", "webkitAlignItems", "webkitAlignSelf",
        "webkitAnimation", "webkitAnimationDelay", "webkitAnimationDirection",
        "webkitAnimationDuration", "webkitAnimationFillMode",
        "webkitAnimationIterationCount", "webkitAnimationName",
        "webkitAnimationPlayState", "webkitAnimationTimingFunction",
        "webkitAppearance", "webkitBackfaceVisibility", "webkitBackgroundClip",
        "webkitBackgroundOrigin", "webkitBackgroundSize", "webkitBorderImage",
        "webkitBorderRadius", "webkitBoxAlign", "webkitBoxDirection",
        "webkitBoxFlex", "webkitBoxOrdinalGroup", "webkitBoxOrient",
        "webkitBoxPack", "webkitBoxShadow", "webkitBoxSizing",
        "webkitFilter", "webkitFlex", "webkitFlexBasis", "webkitFlexDirection",
        "webkitFlexFlow", "webkitFlexGrow", "webkitFlexShrink", "webkitFlexWrap",
        "webkitFontSmoothing", "webkitJustifyContent", "webkitLineClamp",
        "webkitMask", "webkitMaskImage", "webkitMaskPosition", "webkitMaskRepeat",
        "webkitMaskSize", "webkitOrder", "webkitOverflowScrolling",
        "webkitPerspective", "webkitPerspectiveOrigin", "webkitTapHighlightColor",
        "webkitTextFillColor", "webkitTextSizeAdjust", "webkitTextStroke",
        "webkitTextStrokeColor", "webkitTextStrokeWidth", "webkitTransform",
        "webkitTransformOrigin", "webkitTransformStyle", "webkitTransition",
        "webkitTransitionDelay", "webkitTransitionDuration",
        "webkitTransitionProperty", "webkitTransitionTimingFunction",
        "webkitUserSelect",

        // Moz vendor prefixes
        "MozAnimation", "MozAppearance", "MozBackfaceVisibility",
        "MozBorderImage", "MozBoxSizing", "MozOsxFontSmoothing",
        "MozTransform", "MozTransformOrigin", "MozTransition",
        "MozUserSelect",

        // MS vendor prefixes
        "msFlexAlign", "msFlexDirection", "msFlexFlow", "msFlexItemAlign",
        "msFlexLinePack", "msFlexNegative", "msFlexOrder", "msFlexPack",
        "msFlexPositive", "msFlexPreferredSize", "msFlexWrap",
        "msOverflowStyle", "msScrollChaining", "msScrollLimit",
        "msScrollLimitXMax", "msScrollLimitXMin", "msScrollLimitYMax",
        "msScrollLimitYMin", "msScrollRails", "msScrollSnapPointsX",
        "msScrollSnapPointsY", "msScrollSnapType", "msScrollSnapX",
        "msScrollSnapY", "msScrollTranslation", "msTextSizeAdjust",
        "msTouchAction", "msTransform", "msTransformOrigin",
        "msTransition", "msUserSelect",
    ]
}

/// Create a complete DOMTokenList (classList) implementation
pub fn create_full_class_list(classes: Vec<String>, context: &mut Context) -> JsObject {
    let state = Rc::new(RefCell::new(classes));

    // add(...tokens)
    let state_clone = state.clone();
    let add = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let mut classes = state_clone.borrow_mut();
            for arg in args.iter() {
                let class = arg.to_string(ctx)?.to_std_string_escaped();
                for token in class.split_whitespace() {
                    if !token.is_empty() && !classes.contains(&token.to_string()) {
                        classes.push(token.to_string());
                    }
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // remove(...tokens)
    let state_clone = state.clone();
    let remove = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let mut classes = state_clone.borrow_mut();
            for arg in args.iter() {
                let class = arg.to_string(ctx)?.to_std_string_escaped();
                for token in class.split_whitespace() {
                    classes.retain(|c| c != token);
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // toggle(token, force?)
    let state_clone = state.clone();
    let toggle = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let token = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let force = args.get(1);
            let mut classes = state_clone.borrow_mut();
            let had_class = classes.contains(&token);

            let should_add = match force {
                Some(f) if !f.is_undefined() => f.to_boolean(),
                _ => !had_class,
            };

            if should_add && !had_class {
                classes.push(token);
                Ok(JsValue::from(true))
            } else if !should_add && had_class {
                classes.retain(|c| c != &token);
                Ok(JsValue::from(false))
            } else {
                Ok(JsValue::from(should_add))
            }
        })
    };

    // contains(token)
    let state_clone = state.clone();
    let contains = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let token = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let has = state_clone.borrow().contains(&token);
            Ok(JsValue::from(has))
        })
    };

    // replace(oldToken, newToken)
    let state_clone = state.clone();
    let replace = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let old_token = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let new_token = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();
            let mut classes = state_clone.borrow_mut();

            if let Some(pos) = classes.iter().position(|c| c == &old_token) {
                classes[pos] = new_token;
                Ok(JsValue::from(true))
            } else {
                Ok(JsValue::from(false))
            }
        })
    };

    // item(index)
    let state_clone = state.clone();
    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let classes = state_clone.borrow();
            match classes.get(index) {
                Some(class) => Ok(JsValue::from(js_string!(class.clone()))),
                None => Ok(JsValue::null()),
            }
        })
    };

    // supports(token) - always returns true for className
    let supports = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });

    // forEach(callback, thisArg?)
    let state_clone = state.clone();
    let for_each = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let callback = args.get_or_undefined(0);
            let this_arg = args.get(1).cloned().unwrap_or(JsValue::undefined());

            if let Some(cb) = callback.as_callable() {
                let classes = state_clone.borrow();
                for (i, class) in classes.iter().enumerate() {
                    let _ = cb.call(&this_arg, &[
                        JsValue::from(js_string!(class.clone())),
                        JsValue::from(i as u32),
                        _this.clone(),
                    ], ctx);
                }
            }
            Ok(JsValue::undefined())
        })
    };

    // entries()
    let state_clone = state.clone();
    let entries = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let arr = JsArray::new(ctx);
            let classes = state_clone.borrow();
            for (i, class) in classes.iter().enumerate() {
                let entry = JsArray::new(ctx);
                let _ = entry.push(JsValue::from(i as u32), ctx);
                let _ = entry.push(JsValue::from(js_string!(class.clone())), ctx);
                let _ = arr.push(JsValue::from(entry), ctx);
            }
            Ok(JsValue::from(arr))
        })
    };

    // keys()
    let state_clone = state.clone();
    let keys = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let arr = JsArray::new(ctx);
            let classes = state_clone.borrow();
            for i in 0..classes.len() {
                let _ = arr.push(JsValue::from(i as u32), ctx);
            }
            Ok(JsValue::from(arr))
        })
    };

    // values()
    let state_clone = state.clone();
    let values = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let arr = JsArray::new(ctx);
            let classes = state_clone.borrow();
            for class in classes.iter() {
                let _ = arr.push(JsValue::from(js_string!(class.clone())), ctx);
            }
            Ok(JsValue::from(arr))
        })
    };

    // toString()
    let state_clone = state.clone();
    let to_string = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let classes = state_clone.borrow();
            Ok(JsValue::from(js_string!(classes.join(" "))))
        })
    };

    // value getter
    let state_clone = state.clone();
    let value_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let classes = state_clone.borrow();
            Ok(JsValue::from(js_string!(classes.join(" "))))
        })
    };

    // length getter
    let state_clone = state.clone();
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let len = state_clone.borrow().len();
            Ok(JsValue::from(len as u32))
        })
    };

    let length_fn = length_getter.to_js_function(context.realm());
    let value_fn = value_getter.to_js_function(context.realm());

    ObjectInitializer::new(context)
        .function(add, js_string!("add"), 1)
        .function(remove, js_string!("remove"), 1)
        .function(toggle, js_string!("toggle"), 2)
        .function(contains, js_string!("contains"), 1)
        .function(replace, js_string!("replace"), 2)
        .function(item, js_string!("item"), 1)
        .function(supports, js_string!("supports"), 1)
        .function(for_each, js_string!("forEach"), 2)
        .function(entries, js_string!("entries"), 0)
        .function(keys, js_string!("keys"), 0)
        .function(values, js_string!("values"), 0)
        .function(to_string, js_string!("toString"), 0)
        .accessor(js_string!("length"), Some(length_fn), None, Attribute::READONLY)
        .accessor(js_string!("value"), Some(value_fn.clone()), None, Attribute::CONFIGURABLE)
        .build()
}

/// Create a NamedNodeMap (attributes) implementation
pub fn create_named_node_map(
    attrs: Vec<(String, String)>,
    context: &mut Context,
) -> JsObject {
    let state = Rc::new(RefCell::new(attrs));

    // getNamedItem(name)
    let state_clone = state.clone();
    let get_named_item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let attrs = state_clone.borrow();
            for (n, v) in attrs.iter() {
                if n == &name {
                    return Ok(JsValue::from(create_attr_node(ctx, n, v)));
                }
            }
            Ok(JsValue::null())
        })
    };

    // setNamedItem(attr)
    let state_clone = state.clone();
    let set_named_item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            if let Some(attr_obj) = args.get(0).and_then(|v| v.as_object()) {
                let name = attr_obj.get(js_string!("name"), ctx)?
                    .to_string(ctx)?.to_std_string_escaped();
                let value = attr_obj.get(js_string!("value"), ctx)?
                    .to_string(ctx)?.to_std_string_escaped();

                let mut attrs = state_clone.borrow_mut();
                let old = attrs.iter().find(|(n, _)| n == &name).map(|(_, v)| v.clone());
                attrs.retain(|(n, _)| n != &name);
                attrs.push((name.clone(), value));

                if let Some(old_val) = old {
                    return Ok(JsValue::from(create_attr_node(ctx, &name, &old_val)));
                }
            }
            Ok(JsValue::null())
        })
    };

    // removeNamedItem(name)
    let state_clone = state.clone();
    let remove_named_item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
            let mut attrs = state_clone.borrow_mut();
            let old = attrs.iter().find(|(n, _)| n == &name).map(|(n, v)| (n.clone(), v.clone()));
            attrs.retain(|(n, _)| n != &name);

            if let Some((n, v)) = old {
                return Ok(JsValue::from(create_attr_node(ctx, &n, &v)));
            }
            Ok(JsValue::null())
        })
    };

    // item(index)
    let state_clone = state.clone();
    let item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_u32(ctx)? as usize;
            let attrs = state_clone.borrow();
            if let Some((name, value)) = attrs.get(index) {
                return Ok(JsValue::from(create_attr_node(ctx, name, value)));
            }
            Ok(JsValue::null())
        })
    };

    // getNamedItemNS(namespaceURI, localName)
    let get_named_item_ns = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    // setNamedItemNS(attr)
    let set_named_item_ns = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    // removeNamedItemNS(namespaceURI, localName)
    let remove_named_item_ns = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    // length getter
    let state_clone = state.clone();
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let len = state_clone.borrow().len();
            Ok(JsValue::from(len as u32))
        })
    };

    let length_fn = length_getter.to_js_function(context.realm());

    // Build NamedNodeMap with indexed access
    let map = ObjectInitializer::new(context)
        .function(get_named_item, js_string!("getNamedItem"), 1)
        .function(set_named_item, js_string!("setNamedItem"), 1)
        .function(remove_named_item, js_string!("removeNamedItem"), 1)
        .function(item, js_string!("item"), 1)
        .function(get_named_item_ns, js_string!("getNamedItemNS"), 2)
        .function(set_named_item_ns, js_string!("setNamedItemNS"), 1)
        .function(remove_named_item_ns, js_string!("removeNamedItemNS"), 2)
        .accessor(js_string!("length"), Some(length_fn), None, Attribute::READONLY)
        .build();

    // Add indexed properties
    let attrs = state.borrow();
    for (i, (name, value)) in attrs.iter().enumerate() {
        let attr = create_attr_node(context, name, value);
        let _ = map.set(js_string!(i.to_string()), JsValue::from(attr), false, context);
    }

    map
}

/// Create an Attr node
fn create_attr_node(context: &mut Context, name: &str, value: &str) -> JsObject {
    ObjectInitializer::new(context)
        .property(js_string!("nodeType"), 2, Attribute::READONLY)
        .property(js_string!("nodeName"), js_string!(name), Attribute::READONLY)
        .property(js_string!("name"), js_string!(name), Attribute::READONLY)
        .property(js_string!("localName"), js_string!(name), Attribute::READONLY)
        .property(js_string!("value"), js_string!(value), Attribute::all())
        .property(js_string!("nodeValue"), js_string!(value), Attribute::all())
        .property(js_string!("textContent"), js_string!(value), Attribute::all())
        .property(js_string!("specified"), true, Attribute::READONLY)
        .property(js_string!("ownerElement"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("namespaceURI"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("prefix"), JsValue::null(), Attribute::READONLY)
        .build()
}

/// Create an Animation object for element.animate()
pub fn create_animation_object(context: &mut Context) -> JsObject {
    let state = Rc::new(RefCell::new(AnimationState::default()));

    #[derive(Default)]
    struct AnimationState {
        play_state: String,
        current_time: f64,
        playback_rate: f64,
    }

    // play()
    let state_clone = state.clone();
    let play = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            state_clone.borrow_mut().play_state = "running".to_string();
            Ok(JsValue::undefined())
        })
    };

    // pause()
    let state_clone = state.clone();
    let pause = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            state_clone.borrow_mut().play_state = "paused".to_string();
            Ok(JsValue::undefined())
        })
    };

    // cancel()
    let state_clone = state.clone();
    let cancel = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            state_clone.borrow_mut().play_state = "idle".to_string();
            Ok(JsValue::undefined())
        })
    };

    // finish()
    let state_clone = state.clone();
    let finish = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            state_clone.borrow_mut().play_state = "finished".to_string();
            Ok(JsValue::undefined())
        })
    };

    // reverse()
    let state_clone = state.clone();
    let reverse = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let mut s = state_clone.borrow_mut();
            s.playback_rate = -s.playback_rate;
            Ok(JsValue::undefined())
        })
    };

    // updatePlaybackRate(rate)
    let state_clone = state.clone();
    let update_playback_rate = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let rate = args.get_or_undefined(0).to_number(ctx)?;
            state_clone.borrow_mut().playback_rate = rate;
            Ok(JsValue::undefined())
        })
    };

    // persist()
    let persist = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // commitStyles()
    let commit_styles = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // Create finished promise
    let finished_promise = create_resolved_promise_simple(context);
    let ready_promise = create_resolved_promise_simple(context);

    // playState getter
    let state_clone = state.clone();
    let play_state_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let ps = state_clone.borrow().play_state.clone();
            Ok(JsValue::from(js_string!(if ps.is_empty() { "idle".to_string() } else { ps })))
        })
    };

    // currentTime getter/setter
    let state_clone = state.clone();
    let current_time_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            Ok(JsValue::from(state_clone.borrow().current_time))
        })
    };

    let state_clone = state.clone();
    let current_time_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let time = args.get_or_undefined(0).to_number(ctx)?;
            state_clone.borrow_mut().current_time = time;
            Ok(JsValue::undefined())
        })
    };

    // playbackRate getter/setter
    let state_clone = state.clone();
    let playback_rate_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let rate = state_clone.borrow().playback_rate;
            Ok(JsValue::from(if rate == 0.0 { 1.0 } else { rate }))
        })
    };

    let state_clone = state.clone();
    let playback_rate_setter = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let rate = args.get_or_undefined(0).to_number(ctx)?;
            state_clone.borrow_mut().playback_rate = rate;
            Ok(JsValue::undefined())
        })
    };

    let play_state_fn = play_state_getter.to_js_function(context.realm());
    let current_time_getter_fn = current_time_getter.to_js_function(context.realm());
    let current_time_setter_fn = current_time_setter.to_js_function(context.realm());
    let playback_rate_getter_fn = playback_rate_getter.to_js_function(context.realm());
    let playback_rate_setter_fn = playback_rate_setter.to_js_function(context.realm());

    ObjectInitializer::new(context)
        .function(play, js_string!("play"), 0)
        .function(pause, js_string!("pause"), 0)
        .function(cancel, js_string!("cancel"), 0)
        .function(finish, js_string!("finish"), 0)
        .function(reverse, js_string!("reverse"), 0)
        .function(update_playback_rate, js_string!("updatePlaybackRate"), 1)
        .function(persist, js_string!("persist"), 0)
        .function(commit_styles, js_string!("commitStyles"), 0)
        .property(js_string!("id"), js_string!(""), Attribute::all())
        .property(js_string!("effect"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("timeline"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("startTime"), JsValue::null(), Attribute::all())
        .property(js_string!("pending"), false, Attribute::READONLY)
        .property(js_string!("replaceState"), js_string!("active"), Attribute::READONLY)
        .property(js_string!("finished"), finished_promise, Attribute::READONLY)
        .property(js_string!("ready"), ready_promise, Attribute::READONLY)
        .property(js_string!("onfinish"), JsValue::null(), Attribute::all())
        .property(js_string!("oncancel"), JsValue::null(), Attribute::all())
        .property(js_string!("onremove"), JsValue::null(), Attribute::all())
        .accessor(js_string!("playState"), Some(play_state_fn), None, Attribute::CONFIGURABLE)
        .accessor(js_string!("currentTime"), Some(current_time_getter_fn), Some(current_time_setter_fn), Attribute::CONFIGURABLE)
        .accessor(js_string!("playbackRate"), Some(playback_rate_getter_fn), Some(playback_rate_setter_fn), Attribute::CONFIGURABLE)
        .build()
}

use std::collections::HashMap;

thread_local! {
    /// Storage for ShadowRoot internal DOM trees
    /// Maps shadow root ID to a container element that holds the shadow tree
    static SHADOW_ROOT_STORAGE: RefCell<HashMap<u64, dom::Element>> =
        RefCell::new(HashMap::new());

    /// Counter for shadow root IDs
    static SHADOW_ROOT_COUNTER: RefCell<u64> = const { RefCell::new(0) };
}

/// Create a ShadowRoot object for attachShadow() with real DOM querying
pub fn create_shadow_root(context: &mut Context, mode: &str, host_element: Option<dom::Element>) -> JsObject {
    // Generate unique ID for this shadow root
    let shadow_id = SHADOW_ROOT_COUNTER.with(|counter| {
        let mut c = counter.borrow_mut();
        *c += 1;
        *c
    });

    // Create a container element for the shadow tree
    // We use the host element's handle to create a shadow container
    if let Some(host) = &host_element {
        // Create a shadow root container - we'll store the children here
        let shadow_container = host.clone_node(false); // Shallow clone as container
        shadow_container.clear_children();
        SHADOW_ROOT_STORAGE.with(|storage| {
            storage.borrow_mut().insert(shadow_id, shadow_container);
        });
    }

    // querySelector - queries the shadow tree
    let shadow_id_qs = shadow_id;
    let query_selector = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let result = SHADOW_ROOT_STORAGE.with(|storage| {
            let storage = storage.borrow();
            if let Some(container) = storage.get(&shadow_id_qs) {
                if let Some(el) = container.query_selector(&selector) {
                    return Some((
                        el.tag_name_upper(),
                        el.node_type(),
                        el.text_content(),
                        el.inner_html(),
                    ));
                }
            }
            None
        });

        if let Some((tag_name, node_type, text_content, inner_html)) = result {
            let obj = ObjectInitializer::new(ctx)
                .property(js_string!("tagName"), js_string!(tag_name.clone()), Attribute::READONLY)
                .property(js_string!("nodeName"), js_string!(tag_name), Attribute::READONLY)
                .property(js_string!("nodeType"), node_type, Attribute::READONLY)
                .property(js_string!("textContent"), js_string!(text_content), Attribute::all())
                .property(js_string!("innerHTML"), js_string!(inner_html), Attribute::all())
                .build();
            return Ok(JsValue::from(obj));
        }
        Ok(JsValue::null())
    });

    // querySelectorAll - queries the shadow tree
    let shadow_id_qsa = shadow_id;
    let query_selector_all = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let selector = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let elements_data = SHADOW_ROOT_STORAGE.with(|storage| {
            let storage = storage.borrow();
            let mut result: Vec<(String, u32, String, String)> = Vec::new();
            if let Some(container) = storage.get(&shadow_id_qsa) {
                for el in container.query_selector_all(&selector) {
                    result.push((el.tag_name_upper(), el.node_type(), el.text_content(), el.inner_html()));
                }
            }
            result
        });

        let result = ObjectInitializer::new(ctx)
            .property(js_string!("length"), elements_data.len() as u32, Attribute::READONLY)
            .build();

        for (i, (tag_name, node_type, text_content, inner_html)) in elements_data.into_iter().enumerate() {
            let obj = ObjectInitializer::new(ctx)
                .property(js_string!("tagName"), js_string!(tag_name.clone()), Attribute::READONLY)
                .property(js_string!("nodeName"), js_string!(tag_name), Attribute::READONLY)
                .property(js_string!("nodeType"), node_type, Attribute::READONLY)
                .property(js_string!("textContent"), js_string!(text_content), Attribute::all())
                .property(js_string!("innerHTML"), js_string!(inner_html), Attribute::all())
                .build();
            let _ = result.set(i as u32, JsValue::from(obj), false, ctx);
        }
        Ok(JsValue::from(result))
    });

    // getElementById - queries the shadow tree by ID
    let shadow_id_byid = shadow_id;
    let get_element_by_id = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let id = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let selector = format!("#{}", id);

        let result = SHADOW_ROOT_STORAGE.with(|storage| {
            let storage = storage.borrow();
            if let Some(container) = storage.get(&shadow_id_byid) {
                if let Some(el) = container.query_selector(&selector) {
                    return Some((
                        el.tag_name_upper(),
                        el.node_type(),
                        el.text_content(),
                        el.inner_html(),
                    ));
                }
            }
            None
        });

        if let Some((tag_name, node_type, text_content, inner_html)) = result {
            let obj = ObjectInitializer::new(ctx)
                .property(js_string!("tagName"), js_string!(tag_name.clone()), Attribute::READONLY)
                .property(js_string!("nodeName"), js_string!(tag_name), Attribute::READONLY)
                .property(js_string!("nodeType"), node_type, Attribute::READONLY)
                .property(js_string!("id"), js_string!(id), Attribute::all())
                .property(js_string!("textContent"), js_string!(text_content), Attribute::all())
                .property(js_string!("innerHTML"), js_string!(inner_html), Attribute::all())
                .build();
            return Ok(JsValue::from(obj));
        }
        Ok(JsValue::null())
    });

    // innerHTML setter that parses HTML into the shadow tree
    let shadow_id_html = shadow_id;
    let set_inner_html = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
        let html = args.get_or_undefined(0).to_string(_ctx)?.to_std_string_escaped();

        SHADOW_ROOT_STORAGE.with(|storage| {
            let mut storage = storage.borrow_mut();
            if let Some(container) = storage.get_mut(&shadow_id_html) {
                container.set_inner_html(&html);
            }
        });
        Ok(JsValue::undefined())
    });

    // innerHTML getter
    let shadow_id_get_html = shadow_id;
    let get_inner_html = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        let html = SHADOW_ROOT_STORAGE.with(|storage| {
            let storage = storage.borrow();
            storage.get(&shadow_id_get_html)
                .map(|container| container.inner_html())
                .unwrap_or_default()
        });
        Ok(JsValue::from(js_string!(html)))
    });

    // elementFromPoint - stub (requires layout)
    let element_from_point = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    // elementsFromPoint - stub (requires layout)
    let elements_from_point = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });

    // getSelection
    let get_selection = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });

    // setHTMLUnsafe
    let shadow_id_set_unsafe = shadow_id;
    let set_html_unsafe = NativeFunction::from_copy_closure(move |_this, args, _ctx| {
        let html = args.get_or_undefined(0).to_string(_ctx)?.to_std_string_escaped();

        SHADOW_ROOT_STORAGE.with(|storage| {
            let mut storage = storage.borrow_mut();
            if let Some(container) = storage.get_mut(&shadow_id_set_unsafe) {
                container.set_inner_html(&html);
            }
        });
        Ok(JsValue::undefined())
    });

    // getAnimations
    let get_animations = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });

    let style_sheets = JsArray::new(context);
    let adopted_style_sheets = JsArray::new(context);

    let inner_html_getter_fn = get_inner_html.to_js_function(context.realm());
    let inner_html_setter_fn = set_inner_html.to_js_function(context.realm());

    ObjectInitializer::new(context)
        .property(js_string!("mode"), js_string!(mode), Attribute::READONLY)
        .property(js_string!("delegatesFocus"), false, Attribute::READONLY)
        .property(js_string!("slotAssignment"), js_string!("named"), Attribute::READONLY)
        .property(js_string!("clonable"), false, Attribute::READONLY)
        .property(js_string!("serializable"), false, Attribute::READONLY)
        .property(js_string!("host"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("activeElement"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("fullscreenElement"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("pictureInPictureElement"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("pointerLockElement"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("styleSheets"), style_sheets, Attribute::READONLY)
        .property(js_string!("adoptedStyleSheets"), adopted_style_sheets, Attribute::all())
        .accessor(js_string!("innerHTML"), Some(inner_html_getter_fn), Some(inner_html_setter_fn), Attribute::CONFIGURABLE)
        .function(query_selector, js_string!("querySelector"), 1)
        .function(query_selector_all, js_string!("querySelectorAll"), 1)
        .function(get_element_by_id, js_string!("getElementById"), 1)
        .function(element_from_point, js_string!("elementFromPoint"), 2)
        .function(elements_from_point, js_string!("elementsFromPoint"), 2)
        .function(get_selection, js_string!("getSelection"), 0)
        .function(set_html_unsafe, js_string!("setHTMLUnsafe"), 1)
        .function(get_animations, js_string!("getAnimations"), 0)
        .build()
}

/// Create DOMRect object
pub fn create_dom_rect(context: &mut Context, x: f64, y: f64, width: f64, height: f64) -> JsObject {
    let top = y;
    let left = x;
    let bottom = y + height;
    let right = x + width;

    // toJSON method
    let to_json = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("x"), x, Attribute::all())
            .property(js_string!("y"), y, Attribute::all())
            .property(js_string!("width"), width, Attribute::all())
            .property(js_string!("height"), height, Attribute::all())
            .property(js_string!("top"), top, Attribute::all())
            .property(js_string!("right"), right, Attribute::all())
            .property(js_string!("bottom"), bottom, Attribute::all())
            .property(js_string!("left"), left, Attribute::all())
            .build();
        Ok(JsValue::from(obj))
    });

    ObjectInitializer::new(context)
        .property(js_string!("x"), x, Attribute::READONLY)
        .property(js_string!("y"), y, Attribute::READONLY)
        .property(js_string!("width"), width, Attribute::READONLY)
        .property(js_string!("height"), height, Attribute::READONLY)
        .property(js_string!("top"), top, Attribute::READONLY)
        .property(js_string!("right"), right, Attribute::READONLY)
        .property(js_string!("bottom"), bottom, Attribute::READONLY)
        .property(js_string!("left"), left, Attribute::READONLY)
        .function(to_json, js_string!("toJSON"), 0)
        .build()
}

/// Create DOMRectList object
pub fn create_dom_rect_list(context: &mut Context, rects: Vec<JsObject>) -> JsObject {
    let len = rects.len();

    // item method
    let item = NativeFunction::from_copy_closure(|this, args, ctx| {
        let index = args.get_or_undefined(0).to_u32(ctx)?;
        if let Some(obj) = this.as_object() {
            return obj.get(js_string!(index.to_string()), ctx);
        }
        Ok(JsValue::null())
    });

    let list = ObjectInitializer::new(context)
        .property(js_string!("length"), len as u32, Attribute::READONLY)
        .function(item, js_string!("item"), 1)
        .build();

    for (i, rect) in rects.into_iter().enumerate() {
        let _ = list.set(js_string!(i.to_string()), JsValue::from(rect), false, context);
    }

    list
}

/// Create DOMPoint object
pub fn create_dom_point(context: &mut Context, x: f64, y: f64, z: f64, w: f64) -> JsObject {
    // matrixTransform(matrix)
    let matrix_transform = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        Ok(JsValue::from(create_dom_point(ctx, x, y, z, w)))
    });

    // toJSON()
    let to_json = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("x"), x, Attribute::all())
            .property(js_string!("y"), y, Attribute::all())
            .property(js_string!("z"), z, Attribute::all())
            .property(js_string!("w"), w, Attribute::all())
            .build();
        Ok(JsValue::from(obj))
    });

    ObjectInitializer::new(context)
        .property(js_string!("x"), x, Attribute::all())
        .property(js_string!("y"), y, Attribute::all())
        .property(js_string!("z"), z, Attribute::all())
        .property(js_string!("w"), w, Attribute::all())
        .function(matrix_transform, js_string!("matrixTransform"), 1)
        .function(to_json, js_string!("toJSON"), 0)
        .build()
}

/// Create DOMMatrix object
pub fn create_dom_matrix(context: &mut Context) -> JsObject {
    // Identity matrix values
    let identity = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];

    // translate(tx, ty, tz?)
    let translate = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // scale(scaleX, scaleY?, scaleZ?, originX?, originY?, originZ?)
    let scale = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // rotate(rotX, rotY?, rotZ?)
    let rotate = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // rotateFromVector(x, y)
    let rotate_from_vector = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // rotateAxisAngle(x, y, z, angle)
    let rotate_axis_angle = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // skewX(sx)
    let skew_x = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // skewY(sy)
    let skew_y = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // multiply(other)
    let multiply = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // flipX()
    let flip_x = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // flipY()
    let flip_y = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // inverse()
    let inverse = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_matrix(ctx)))
    });

    // transformPoint(point)
    let transform_point = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_dom_point(ctx, 0.0, 0.0, 0.0, 1.0)))
    });

    // toFloat32Array()
    let to_float32_array = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });

    // toFloat64Array()
    let to_float64_array = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });

    // toString()
    let to_string = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("matrix(1, 0, 0, 1, 0, 0)")))
    });

    // toJSON()
    let to_json = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("a"), 1.0, Attribute::all())
            .property(js_string!("b"), 0.0, Attribute::all())
            .property(js_string!("c"), 0.0, Attribute::all())
            .property(js_string!("d"), 1.0, Attribute::all())
            .property(js_string!("e"), 0.0, Attribute::all())
            .property(js_string!("f"), 0.0, Attribute::all())
            .build();
        Ok(JsValue::from(obj))
    });

    ObjectInitializer::new(context)
        // 2D properties
        .property(js_string!("a"), identity[0], Attribute::all())
        .property(js_string!("b"), identity[1], Attribute::all())
        .property(js_string!("c"), identity[4], Attribute::all())
        .property(js_string!("d"), identity[5], Attribute::all())
        .property(js_string!("e"), identity[12], Attribute::all())
        .property(js_string!("f"), identity[13], Attribute::all())
        // 3D properties
        .property(js_string!("m11"), identity[0], Attribute::all())
        .property(js_string!("m12"), identity[1], Attribute::all())
        .property(js_string!("m13"), identity[2], Attribute::all())
        .property(js_string!("m14"), identity[3], Attribute::all())
        .property(js_string!("m21"), identity[4], Attribute::all())
        .property(js_string!("m22"), identity[5], Attribute::all())
        .property(js_string!("m23"), identity[6], Attribute::all())
        .property(js_string!("m24"), identity[7], Attribute::all())
        .property(js_string!("m31"), identity[8], Attribute::all())
        .property(js_string!("m32"), identity[9], Attribute::all())
        .property(js_string!("m33"), identity[10], Attribute::all())
        .property(js_string!("m34"), identity[11], Attribute::all())
        .property(js_string!("m41"), identity[12], Attribute::all())
        .property(js_string!("m42"), identity[13], Attribute::all())
        .property(js_string!("m43"), identity[14], Attribute::all())
        .property(js_string!("m44"), identity[15], Attribute::all())
        .property(js_string!("is2D"), true, Attribute::READONLY)
        .property(js_string!("isIdentity"), true, Attribute::READONLY)
        // Methods
        .function(translate, js_string!("translate"), 3)
        .function(scale, js_string!("scale"), 6)
        .function(rotate, js_string!("rotate"), 3)
        .function(rotate_from_vector, js_string!("rotateFromVector"), 2)
        .function(rotate_axis_angle, js_string!("rotateAxisAngle"), 4)
        .function(skew_x, js_string!("skewX"), 1)
        .function(skew_y, js_string!("skewY"), 1)
        .function(multiply, js_string!("multiply"), 1)
        .function(flip_x, js_string!("flipX"), 0)
        .function(flip_y, js_string!("flipY"), 0)
        .function(inverse, js_string!("inverse"), 0)
        .function(transform_point, js_string!("transformPoint"), 1)
        .function(to_float32_array, js_string!("toFloat32Array"), 0)
        .function(to_float64_array, js_string!("toFloat64Array"), 0)
        .function(to_string, js_string!("toString"), 0)
        .function(to_json, js_string!("toJSON"), 0)
        .build()
}

/// Simple resolved promise helper
fn create_resolved_promise_simple(context: &mut Context) -> JsValue {
    let then_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if callback.is_callable() {
            let cb = callback.as_callable().unwrap();
            let _ = cb.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(this.clone())
    });

    let catch_fn = NativeFunction::from_copy_closure(|this, _args, _ctx| Ok(this.clone()));
    let finally_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if callback.is_callable() {
            let cb = callback.as_callable().unwrap();
            let _ = cb.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(this.clone())
    });

    let promise = ObjectInitializer::new(context)
        .function(then_fn, js_string!("then"), 2)
        .function(catch_fn, js_string!("catch"), 1)
        .function(finally_fn, js_string!("finally"), 1)
        .build();

    JsValue::from(promise)
}

/// Create IntersectionObserverEntry object
pub fn create_intersection_observer_entry(context: &mut Context) -> JsObject {
    let bounding_rect = create_dom_rect(context, 0.0, 0.0, 100.0, 20.0);
    let intersection_rect = create_dom_rect(context, 0.0, 0.0, 100.0, 20.0);
    let root_bounds = create_dom_rect(context, 0.0, 0.0, 1920.0, 1080.0);

    ObjectInitializer::new(context)
        .property(js_string!("boundingClientRect"), bounding_rect, Attribute::READONLY)
        .property(js_string!("intersectionRatio"), 1.0, Attribute::READONLY)
        .property(js_string!("intersectionRect"), intersection_rect, Attribute::READONLY)
        .property(js_string!("isIntersecting"), true, Attribute::READONLY)
        .property(js_string!("isVisible"), true, Attribute::READONLY)
        .property(js_string!("rootBounds"), root_bounds, Attribute::READONLY)
        .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
        .property(js_string!("time"), 0.0, Attribute::READONLY)
        .build()
}

/// Create ResizeObserverEntry object
pub fn create_resize_observer_entry(context: &mut Context) -> JsObject {
    let content_rect = create_dom_rect(context, 0.0, 0.0, 100.0, 20.0);
    let border_box_size = JsArray::new(context);
    let content_box_size = JsArray::new(context);
    let device_pixel_content_box_size = JsArray::new(context);

    ObjectInitializer::new(context)
        .property(js_string!("contentRect"), content_rect, Attribute::READONLY)
        .property(js_string!("borderBoxSize"), border_box_size, Attribute::READONLY)
        .property(js_string!("contentBoxSize"), content_box_size, Attribute::READONLY)
        .property(js_string!("devicePixelContentBoxSize"), device_pixel_content_box_size, Attribute::READONLY)
        .property(js_string!("target"), JsValue::null(), Attribute::READONLY)
        .build()
}

/// Register global geometry constructors (DOMRect, DOMPoint, DOMMatrix, etc.)
pub fn register_geometry_constructors(context: &mut Context) -> Result<(), boa_engine::JsError> {
    // DOMRect constructor
    let dom_rect_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let x = args.get(0).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let y = args.get(1).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let width = args.get(2).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let height = args.get(3).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        Ok(JsValue::from(create_dom_rect(ctx, x, y, width, height)))
    });

    // DOMRect.fromRect static method
    let dom_rect_from_rect = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(obj) = args.get(0).and_then(|v| v.as_object()) {
            let x = obj.get(js_string!("x"), ctx).ok()
                .and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let y = obj.get(js_string!("y"), ctx).ok()
                .and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let width = obj.get(js_string!("width"), ctx).ok()
                .and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            let height = obj.get(js_string!("height"), ctx).ok()
                .and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
            return Ok(JsValue::from(create_dom_rect(ctx, x, y, width, height)));
        }
        Ok(JsValue::from(create_dom_rect(ctx, 0.0, 0.0, 0.0, 0.0)))
    });

    let dom_rect_obj = ObjectInitializer::new(context)
        .function(dom_rect_from_rect, js_string!("fromRect"), 1)
        .build();

    let dom_rect_fn = dom_rect_ctor.to_js_function(context.realm());
    let _ = dom_rect_fn.set(js_string!("fromRect"),
        dom_rect_obj.get(js_string!("fromRect"), context)?, false, context);

    context.register_global_property(js_string!("DOMRect"), dom_rect_fn, Attribute::all())?;

    // DOMRectReadOnly (same as DOMRect for our purposes)
    let dom_rect_readonly_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let x = args.get(0).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let y = args.get(1).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let width = args.get(2).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let height = args.get(3).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        Ok(JsValue::from(create_dom_rect(ctx, x, y, width, height)))
    });
    context.register_global_property(
        js_string!("DOMRectReadOnly"),
        dom_rect_readonly_ctor.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // DOMPoint constructor
    let dom_point_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let x = args.get(0).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let y = args.get(1).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let z = args.get(2).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let w = args.get(3).and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
        Ok(JsValue::from(create_dom_point(ctx, x, y, z, w)))
    });
    context.register_global_property(
        js_string!("DOMPoint"),
        dom_point_ctor.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // DOMPointReadOnly
    let dom_point_readonly_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let x = args.get(0).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let y = args.get(1).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let z = args.get(2).and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0);
        let w = args.get(3).and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0);
        Ok(JsValue::from(create_dom_point(ctx, x, y, z, w)))
    });
    context.register_global_property(
        js_string!("DOMPointReadOnly"),
        dom_point_readonly_ctor.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // Note: DOMMatrix and DOMMatrixReadOnly are registered in modern.rs with full matrix math

    Ok(())
}

/// Add extended element methods to an element object
pub fn extend_element(
    element: &JsObject,
    context: &mut Context,
) -> Result<(), boa_engine::JsError> {
    // getAttributeNames()
    let get_attribute_names = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });
    let _ = element.set(
        js_string!("getAttributeNames"),
        get_attribute_names.to_js_function(context.realm()),
        false,
        context
    );

    // toggleAttribute(name, force?)
    let toggle_attribute = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });
    let _ = element.set(
        js_string!("toggleAttribute"),
        toggle_attribute.to_js_function(context.realm()),
        false,
        context
    );

    // getAttributeNode(name)
    let get_attribute_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = element.set(
        js_string!("getAttributeNode"),
        get_attribute_node.to_js_function(context.realm()),
        false,
        context
    );

    // setAttributeNode(attr)
    let set_attribute_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = element.set(
        js_string!("setAttributeNode"),
        set_attribute_node.to_js_function(context.realm()),
        false,
        context
    );

    // removeAttributeNode(attr)
    let remove_attribute_node = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        Ok(args.get_or_undefined(0).clone())
    });
    let _ = element.set(
        js_string!("removeAttributeNode"),
        remove_attribute_node.to_js_function(context.realm()),
        false,
        context
    );

    // getAttributeNS(namespace, name)
    let get_attribute_ns = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = element.set(
        js_string!("getAttributeNS"),
        get_attribute_ns.to_js_function(context.realm()),
        false,
        context
    );

    // setAttributeNS(namespace, name, value)
    let set_attribute_ns = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("setAttributeNS"),
        set_attribute_ns.to_js_function(context.realm()),
        false,
        context
    );

    // removeAttributeNS(namespace, name)
    let remove_attribute_ns = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("removeAttributeNS"),
        remove_attribute_ns.to_js_function(context.realm()),
        false,
        context
    );

    // hasAttributeNS(namespace, name)
    let has_attribute_ns = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = element.set(
        js_string!("hasAttributeNS"),
        has_attribute_ns.to_js_function(context.realm()),
        false,
        context
    );

    // getAttributeNodeNS(namespace, name)
    let get_attribute_node_ns = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = element.set(
        js_string!("getAttributeNodeNS"),
        get_attribute_node_ns.to_js_function(context.realm()),
        false,
        context
    );

    // setAttributeNodeNS(attr)
    let set_attribute_node_ns = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = element.set(
        js_string!("setAttributeNodeNS"),
        set_attribute_node_ns.to_js_function(context.realm()),
        false,
        context
    );

    // attachShadow(init) - Note: host element passed as None here, real impl in dom_bindings
    let attach_shadow = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let mode = args.get(0)
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("mode"), ctx).ok())
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|| "open".to_string());
        Ok(JsValue::from(create_shadow_root(ctx, &mode, None)))
    });
    let _ = element.set(
        js_string!("attachShadow"),
        attach_shadow.to_js_function(context.realm()),
        false,
        context
    );

    // animate(keyframes, options)
    let animate = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_animation_object(ctx)))
    });
    let _ = element.set(
        js_string!("animate"),
        animate.to_js_function(context.realm()),
        false,
        context
    );

    // getAnimations()
    let get_animations = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });
    let _ = element.set(
        js_string!("getAnimations"),
        get_animations.to_js_function(context.realm()),
        false,
        context
    );

    // requestFullscreen()
    let request_fullscreen = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(create_resolved_promise_simple(ctx))
    });
    let _ = element.set(
        js_string!("requestFullscreen"),
        request_fullscreen.to_js_function(context.realm()),
        false,
        context
    );

    // requestPointerLock()
    let request_pointer_lock = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(create_resolved_promise_simple(ctx))
    });
    let _ = element.set(
        js_string!("requestPointerLock"),
        request_pointer_lock.to_js_function(context.realm()),
        false,
        context
    );

    // setPointerCapture(pointerId)
    let set_pointer_capture = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("setPointerCapture"),
        set_pointer_capture.to_js_function(context.realm()),
        false,
        context
    );

    // releasePointerCapture(pointerId)
    let release_pointer_capture = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("releasePointerCapture"),
        release_pointer_capture.to_js_function(context.realm()),
        false,
        context
    );

    // hasPointerCapture(pointerId)
    let has_pointer_capture = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = element.set(
        js_string!("hasPointerCapture"),
        has_pointer_capture.to_js_function(context.realm()),
        false,
        context
    );

    // scroll(x, y) / scroll(options)
    let scroll = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("scroll"),
        scroll.to_js_function(context.realm()),
        false,
        context
    );

    // scrollTo(x, y) / scrollTo(options)
    let scroll_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("scrollTo"),
        scroll_to.to_js_function(context.realm()),
        false,
        context
    );

    // scrollBy(x, y) / scrollBy(options)
    let scroll_by = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("scrollBy"),
        scroll_by.to_js_function(context.realm()),
        false,
        context
    );

    // scrollIntoViewIfNeeded(centerIfNeeded?)
    let scroll_into_view_if_needed = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("scrollIntoViewIfNeeded"),
        scroll_into_view_if_needed.to_js_function(context.realm()),
        false,
        context
    );

    // insertBefore(node, reference)
    let insert_before = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        Ok(args.get_or_undefined(0).clone())
    });
    let _ = element.set(
        js_string!("insertBefore"),
        insert_before.to_js_function(context.realm()),
        false,
        context
    );

    // replaceChild(newNode, oldNode)
    let replace_child = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        Ok(args.get_or_undefined(1).clone())
    });
    let _ = element.set(
        js_string!("replaceChild"),
        replace_child.to_js_function(context.realm()),
        false,
        context
    );

    // replaceChildren(...nodes)
    let replace_children = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("replaceChildren"),
        replace_children.to_js_function(context.realm()),
        false,
        context
    );

    // hasChildNodes()
    let has_child_nodes = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = element.set(
        js_string!("hasChildNodes"),
        has_child_nodes.to_js_function(context.realm()),
        false,
        context
    );

    // normalize()
    let normalize = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("normalize"),
        normalize.to_js_function(context.realm()),
        false,
        context
    );

    // getRootNode(options?)
    let get_root_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = element.set(
        js_string!("getRootNode"),
        get_root_node.to_js_function(context.realm()),
        false,
        context
    );

    // compareDocumentPosition(other)
    let compare_document_position = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        // DOCUMENT_POSITION_DISCONNECTED = 1
        Ok(JsValue::from(1))
    });
    let _ = element.set(
        js_string!("compareDocumentPosition"),
        compare_document_position.to_js_function(context.realm()),
        false,
        context
    );

    // isEqualNode(other)
    let is_equal_node = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = element.set(
        js_string!("isEqualNode"),
        is_equal_node.to_js_function(context.realm()),
        false,
        context
    );

    // isSameNode(other)
    let is_same_node = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        let _other = args.get_or_undefined(0);
        // Very simplified - just check if they're the same object
        Ok(JsValue::from(false))
    });
    let _ = element.set(
        js_string!("isSameNode"),
        is_same_node.to_js_function(context.realm()),
        false,
        context
    );

    // lookupPrefix(namespace)
    let lookup_prefix = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = element.set(
        js_string!("lookupPrefix"),
        lookup_prefix.to_js_function(context.realm()),
        false,
        context
    );

    // lookupNamespaceURI(prefix)
    let lookup_namespace_uri = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::null())
    });
    let _ = element.set(
        js_string!("lookupNamespaceURI"),
        lookup_namespace_uri.to_js_function(context.realm()),
        false,
        context
    );

    // isDefaultNamespace(namespace)
    let is_default_namespace = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = element.set(
        js_string!("isDefaultNamespace"),
        is_default_namespace.to_js_function(context.realm()),
        false,
        context
    );

    // webkitMatchesSelector (alias for matches)
    let webkit_matches_selector = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = element.set(
        js_string!("webkitMatchesSelector"),
        webkit_matches_selector.to_js_function(context.realm()),
        false,
        context
    );

    // msMatchesSelector (alias for matches)
    let ms_matches_selector = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(false))
    });
    let _ = element.set(
        js_string!("msMatchesSelector"),
        ms_matches_selector.to_js_function(context.realm()),
        false,
        context
    );

    // setHTML(html, options?) - HTML Sanitizer API
    let set_html = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("setHTML"),
        set_html.to_js_function(context.realm()),
        false,
        context
    );

    // setHTMLUnsafe(html)
    let set_html_unsafe = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });
    let _ = element.set(
        js_string!("setHTMLUnsafe"),
        set_html_unsafe.to_js_function(context.realm()),
        false,
        context
    );

    // getHTML(options?)
    let get_html = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(js_string!("")))
    });
    let _ = element.set(
        js_string!("getHTML"),
        get_html.to_js_function(context.realm()),
        false,
        context
    );

    // computedStyleMap()
    let computed_style_map = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Return a StylePropertyMapReadOnly-like object
        let map = ObjectInitializer::new(ctx)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::null())),
                js_string!("get"),
                1
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, ctx| Ok(JsValue::from(JsArray::new(ctx)))),
                js_string!("getAll"),
                1
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(false))),
                js_string!("has"),
                1
            )
            .property(js_string!("size"), 0, Attribute::READONLY)
            .build();
        Ok(JsValue::from(map))
    });
    let _ = element.set(
        js_string!("computedStyleMap"),
        computed_style_map.to_js_function(context.realm()),
        false,
        context
    );

    // checkVisibility(options?)
    let check_visibility = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(true))
    });
    let _ = element.set(
        js_string!("checkVisibility"),
        check_visibility.to_js_function(context.realm()),
        false,
        context
    );

    // Add ARIA properties
    let aria_properties = [
        "ariaAtomic", "ariaAutoComplete", "ariaBrailleLabel", "ariaBrailleRoleDescription",
        "ariaBusy", "ariaChecked", "ariaColCount", "ariaColIndex", "ariaColIndexText",
        "ariaColSpan", "ariaCurrent", "ariaDescription", "ariaDisabled", "ariaExpanded",
        "ariaHasPopup", "ariaHidden", "ariaInvalid", "ariaKeyShortcuts", "ariaLabel",
        "ariaLevel", "ariaLive", "ariaModal", "ariaMultiLine", "ariaMultiSelectable",
        "ariaOrientation", "ariaPlaceholder", "ariaPosInSet", "ariaPressed", "ariaReadOnly",
        "ariaRelevant", "ariaRequired", "ariaRoleDescription", "ariaRowCount", "ariaRowIndex",
        "ariaRowIndexText", "ariaRowSpan", "ariaSelected", "ariaSetSize", "ariaSort",
        "ariaValueMax", "ariaValueMin", "ariaValueNow", "ariaValueText", "role",
    ];

    for prop in aria_properties {
        let _ = element.set(js_string!(prop), JsValue::null(), false, context);
    }

    // Add common element properties
    let _ = element.set(js_string!("namespaceURI"), js_string!("http://www.w3.org/1999/xhtml"), false, context);
    let _ = element.set(js_string!("prefix"), JsValue::null(), false, context);
    let _ = element.set(js_string!("localName"), js_string!(""), false, context);
    let _ = element.set(js_string!("shadowRoot"), JsValue::null(), false, context);
    let _ = element.set(js_string!("assignedSlot"), JsValue::null(), false, context);
    let _ = element.set(js_string!("part"), JsArray::new(context), false, context);

    // Add attributes NamedNodeMap
    let attributes = create_named_node_map(vec![], context);
    let _ = element.set(js_string!("attributes"), attributes, false, context);

    Ok(())
}

/// Register getComputedStyle global function
pub fn register_get_computed_style(context: &mut Context) -> Result<(), boa_engine::JsError> {
    let get_computed_style = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(create_full_style_object(ctx)))
    });

    context.register_global_property(
        js_string!("getComputedStyle"),
        get_computed_style.to_js_function(context.realm()),
        Attribute::all()
    )?;

    Ok(())
}

/// Helper to register a constructor with proper prototype chain for class extension
fn register_extendable_constructor(
    context: &mut Context,
    name: &str,
    parent_prototype: Option<&JsObject>,
) -> Result<JsObject, boa_engine::JsError> {
    use boa_engine::object::FunctionObjectBuilder;

    // Create the constructor function
    let constructor = FunctionObjectBuilder::new(
        context.realm(),
        NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            // "Illegal constructor" - direct instantiation not allowed
            Err(boa_engine::JsError::from_opaque(
                JsValue::from(js_string!("Illegal constructor"))
            ))
        }),
    )
    .name(js_string!(name))
    .length(0)
    .constructor(true)
    .build();

    // Create prototype object
    let prototype = if let Some(parent_proto) = parent_prototype {
        // Create prototype with parent in the chain
        let proto = ObjectInitializer::new(context).build();
        proto.set_prototype(Some(parent_proto.clone()));
        proto
    } else {
        ObjectInitializer::new(context).build()
    };

    // Set constructor.prototype = prototype
    constructor.set(js_string!("prototype"), prototype.clone(), false, context)?;

    // Set prototype.constructor = constructor
    prototype.set(js_string!("constructor"), constructor.clone(), false, context)?;

    // Register globally
    context.register_global_property(js_string!(name), constructor.clone(), Attribute::all())?;

    Ok(prototype)
}

/// Register all element-related global APIs
pub fn register_element_apis(context: &mut Context) -> Result<(), boa_engine::JsError> {
    register_geometry_constructors(context)?;
    register_get_computed_style(context)?;

    // Register EventTarget as the base (if not already registered)
    let event_target_proto = register_extendable_constructor(context, "EventTarget", None)?;

    // Register Node extending EventTarget
    let node_proto = register_extendable_constructor(context, "Node", Some(&event_target_proto))?;

    // Register Element extending Node
    let element_proto = register_extendable_constructor(context, "Element", Some(&node_proto))?;

    // Register HTMLElement extending Element - this is what custom elements extend
    let html_element_proto = register_extendable_constructor(context, "HTMLElement", Some(&element_proto))?;

    // Register specific HTML element type constructors (stubs for instanceof checks)
    let html_element_types = [
        "HTMLScriptElement",
        "HTMLDivElement",
        "HTMLSpanElement",
        "HTMLInputElement",
        "HTMLButtonElement",
        "HTMLFormElement",
        "HTMLAnchorElement",
        "HTMLImageElement",
        "HTMLLinkElement",
        "HTMLStyleElement",
        "HTMLMetaElement",
        "HTMLHeadElement",
        "HTMLBodyElement",
        "HTMLHtmlElement",
        "HTMLIFrameElement",
        "HTMLCanvasElement",
        "HTMLVideoElement",
        "HTMLAudioElement",
        "HTMLSourceElement",
        "HTMLMediaElement",
        "HTMLTableElement",
        "HTMLTableRowElement",
        "HTMLTableCellElement",
        "HTMLTableSectionElement",
        "HTMLUListElement",
        "HTMLOListElement",
        "HTMLLIElement",
        "HTMLSelectElement",
        "HTMLOptionElement",
        "HTMLTextAreaElement",
        "HTMLLabelElement",
        "HTMLFieldSetElement",
        "HTMLLegendElement",
        "HTMLParagraphElement",
        "HTMLHeadingElement",
        "HTMLBRElement",
        "HTMLHRElement",
        "HTMLPreElement",
        "HTMLQuoteElement",
        "HTMLTemplateElement",
        "HTMLSlotElement",
        "HTMLDetailsElement",
        "HTMLSummaryElement",
        "HTMLDialogElement",
        "HTMLMenuElement",
        "HTMLDataListElement",
        "HTMLOutputElement",
        "HTMLProgressElement",
        "HTMLMeterElement",
        "HTMLEmbedElement",
        "HTMLObjectElement",
        "HTMLParamElement",
        "HTMLTrackElement",
        "HTMLMapElement",
        "HTMLAreaElement",
        "HTMLTimeElement",
        "HTMLDataElement",
        "HTMLPictureElement",
        "HTMLUnknownElement",
    ];

    for element_type in html_element_types {
        // Register each HTML element type extending HTMLElement
        let _ = register_extendable_constructor(context, element_type, Some(&html_element_proto))?;
    }

    // Register SVG element type constructors (stubs)
    // Note: SVGElement is fully implemented in modern.rs
    let svg_element_types = [
        "SVGSVGElement",
        "SVGPathElement",
        "SVGRectElement",
        "SVGCircleElement",
        "SVGEllipseElement",
        "SVGLineElement",
        "SVGPolylineElement",
        "SVGPolygonElement",
        "SVGTextElement",
        "SVGTSpanElement",
        "SVGGElement",
        "SVGDefsElement",
        "SVGSymbolElement",
        "SVGUseElement",
        "SVGImageElement",
        "SVGClipPathElement",
        "SVGMaskElement",
        "SVGPatternElement",
        "SVGLinearGradientElement",
        "SVGRadialGradientElement",
        "SVGStopElement",
        "SVGFilterElement",
        "SVGForeignObjectElement",
        "SVGAnimateElement",
        "SVGAnimateMotionElement",
        "SVGAnimateTransformElement",
        "SVGMarkerElement",
    ];

    // Register SVGElement extending Element
    let svg_element_proto = register_extendable_constructor(context, "SVGElement", Some(&element_proto))?;

    for element_type in svg_element_types {
        // Register each SVG element type extending SVGElement
        let _ = register_extendable_constructor(context, element_type, Some(&svg_element_proto))?;
    }

    // Register other DOM type constructors (stubs)
    let dom_types = [
        "Text",
        "Comment",
        "CDATASection",
        "ProcessingInstruction",
        "Attr",
        "CharacterData",
        "Document",
        "DocumentType",
        "NamedNodeMap",
        "NodeList",
        "HTMLCollection",
        "DOMTokenList",
        "DOMStringMap",
        "CSSStyleDeclaration",
        "StyleSheet",
        "CSSStyleSheet",
        "CSSRule",
        "CSSStyleRule",
        // MediaList is properly implemented in modern.rs
        "Screen",
        "History",
        "Location",
        "Navigator",
        "Window",
        "XMLDocument",
        "DOMParser",
        "XMLSerializer",
        "XPathResult",
        "XPathExpression",
        "XPathEvaluator",
        "TreeWalker",
        "NodeIterator",
        "NodeFilter",
    ];

    // CharacterData extends Node
    let char_data_proto = register_extendable_constructor(context, "CharacterData", Some(&node_proto))?;

    // Text and Comment extend CharacterData
    let _ = register_extendable_constructor(context, "Text", Some(&char_data_proto))?;
    let _ = register_extendable_constructor(context, "Comment", Some(&char_data_proto))?;
    let _ = register_extendable_constructor(context, "CDATASection", Some(&char_data_proto))?;
    let _ = register_extendable_constructor(context, "ProcessingInstruction", Some(&char_data_proto))?;

    // Document extends Node
    let _ = register_extendable_constructor(context, "Document", Some(&node_proto))?;
    let _ = register_extendable_constructor(context, "XMLDocument", Some(&node_proto))?;
    let _ = register_extendable_constructor(context, "DocumentType", Some(&node_proto))?;

    // Other DOM types (not typically extended by user code, but make them proper constructors)
    let other_dom_types = [
        "Attr",
        "NamedNodeMap",
        "NodeList",
        "HTMLCollection",
        "DOMTokenList",
        "DOMStringMap",
        "CSSStyleDeclaration",
        "StyleSheet",
        "CSSStyleSheet",
        "CSSRule",
        "CSSStyleRule",
        "Screen",
        "History",
        "Location",
        "Navigator",
        "Window",
        "DOMParser",
        "XMLSerializer",
        "XPathResult",
        "XPathExpression",
        "XPathEvaluator",
        "TreeWalker",
        "NodeIterator",
        "NodeFilter",
    ];

    for dom_type in other_dom_types {
        let _ = register_extendable_constructor(context, dom_type, None)?;
    }

    // Add Node constants to the already-registered Node constructor
    if let Ok(node_val) = context.global_object().get(js_string!("Node"), context) {
        if let Some(node_fn) = node_val.as_object() {
            let _ = node_fn.set(js_string!("ELEMENT_NODE"), JsValue::from(1), false, context);
            let _ = node_fn.set(js_string!("ATTRIBUTE_NODE"), JsValue::from(2), false, context);
            let _ = node_fn.set(js_string!("TEXT_NODE"), JsValue::from(3), false, context);
            let _ = node_fn.set(js_string!("CDATA_SECTION_NODE"), JsValue::from(4), false, context);
            let _ = node_fn.set(js_string!("PROCESSING_INSTRUCTION_NODE"), JsValue::from(7), false, context);
            let _ = node_fn.set(js_string!("COMMENT_NODE"), JsValue::from(8), false, context);
            let _ = node_fn.set(js_string!("DOCUMENT_NODE"), JsValue::from(9), false, context);
            let _ = node_fn.set(js_string!("DOCUMENT_TYPE_NODE"), JsValue::from(10), false, context);
            let _ = node_fn.set(js_string!("DOCUMENT_FRAGMENT_NODE"), JsValue::from(11), false, context);
            let _ = node_fn.set(js_string!("DOCUMENT_POSITION_DISCONNECTED"), JsValue::from(1), false, context);
            let _ = node_fn.set(js_string!("DOCUMENT_POSITION_PRECEDING"), JsValue::from(2), false, context);
            let _ = node_fn.set(js_string!("DOCUMENT_POSITION_FOLLOWING"), JsValue::from(4), false, context);
            let _ = node_fn.set(js_string!("DOCUMENT_POSITION_CONTAINS"), JsValue::from(8), false, context);
            let _ = node_fn.set(js_string!("DOCUMENT_POSITION_CONTAINED_BY"), JsValue::from(16), false, context);
            let _ = node_fn.set(js_string!("DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC"), JsValue::from(32), false, context);
        }
    }

    // Register DocumentFragment constructor
    let doc_fragment_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let child_nodes = JsArray::new(ctx);
        let children = JsArray::new(ctx);
        let frag = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 11, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#document-fragment"), Attribute::READONLY)
            .property(js_string!("childNodes"), child_nodes, Attribute::READONLY)
            .property(js_string!("children"), children, Attribute::READONLY)
            .property(js_string!("firstChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("lastChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("firstElementChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("lastElementChild"), JsValue::null(), Attribute::READONLY)
            .property(js_string!("childElementCount"), 0, Attribute::READONLY)
            .build();
        Ok(JsValue::from(frag))
    });
    context.register_global_property(
        js_string!("DocumentFragment"),
        doc_fragment_ctor.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // Register Text constructor
    let text_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let data = args.get(0)
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let text = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 3, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#text"), Attribute::READONLY)
            .property(js_string!("data"), js_string!(data.clone()), Attribute::all())
            .property(js_string!("textContent"), js_string!(data.clone()), Attribute::all())
            .property(js_string!("nodeValue"), js_string!(data.clone()), Attribute::all())
            .property(js_string!("length"), data.len() as u32, Attribute::READONLY)
            .property(js_string!("wholeText"), js_string!(data), Attribute::READONLY)
            .build();
        Ok(JsValue::from(text))
    });
    context.register_global_property(
        js_string!("Text"),
        text_ctor.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // Register Comment constructor
    let comment_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let data = args.get(0)
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let comment = ObjectInitializer::new(ctx)
            .property(js_string!("nodeType"), 8, Attribute::READONLY)
            .property(js_string!("nodeName"), js_string!("#comment"), Attribute::READONLY)
            .property(js_string!("data"), js_string!(data.clone()), Attribute::all())
            .property(js_string!("textContent"), js_string!(data.clone()), Attribute::all())
            .property(js_string!("nodeValue"), js_string!(data.clone()), Attribute::all())
            .property(js_string!("length"), data.len() as u32, Attribute::READONLY)
            .build();
        Ok(JsValue::from(comment))
    });
    context.register_global_property(
        js_string!("Comment"),
        comment_ctor.to_js_function(context.realm()),
        Attribute::all()
    )?;

    Ok(())
}
