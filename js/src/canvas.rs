// Canvas API stub implementation for JavaScript runtime
// Provides stub implementations for Canvas APIs that don't render but prevent errors

use boa_engine::{
    Context, JsArgs, JsResult, JsValue, Source,
    object::ObjectInitializer,
    property::Attribute,
    NativeFunction, JsString,
    js_string,
};

/// Register all Canvas APIs
pub fn register_all_canvas_apis(context: &mut Context) -> JsResult<()> {
    register_canvas_rendering_context_2d(context)?;
    register_image_data(context)?;
    register_path_2d(context)?;
    register_offscreen_canvas(context)?;
    register_canvas_gradient(context)?;
    register_canvas_pattern(context)?;
    Ok(())
}

/// Register CanvasRenderingContext2D globally
fn register_canvas_rendering_context_2d(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let obj = create_context_2d_object(ctx)?;
        Ok(JsValue::from(obj))
    });

    let ctor = boa_engine::object::FunctionObjectBuilder::new(context.realm(), constructor)
        .name(js_string!("CanvasRenderingContext2D"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("CanvasRenderingContext2D"), ctor, boa_engine::property::Attribute::all())?;
    Ok(())
}

/// Create a CanvasRenderingContext2D object
fn create_context_2d_object(context: &mut Context) -> JsResult<boa_engine::JsObject> {
    // Convert all functions first to avoid borrow issues
    let fill_rect = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let stroke_rect = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let clear_rect = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let fill_text = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let stroke_text = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());

    let measure_text = NativeFunction::from_copy_closure(|this, args, ctx| {
        let text = args.get_or_undefined(0);
        let text_str = if let Some(s) = text.as_string() {
            s.to_std_string_escaped()
        } else {
            String::new()
        };

        // Get font from context (this)
        let (font_size, is_monospace) = if let Some(obj) = this.as_object() {
            if let Ok(font_val) = obj.get(js_string!("font"), ctx) {
                if let Some(font_str) = font_val.as_string() {
                    parse_font_string(&font_str.to_std_string_escaped())
                } else {
                    (10.0, false)
                }
            } else {
                (10.0, false)
            }
        } else {
            (10.0, false)
        };

        // Calculate text width based on font characteristics
        let width = calculate_text_width(&text_str, font_size, is_monospace);

        // Calculate font metrics based on font size
        // Standard proportions: ascent ~80% of em, descent ~20% of em
        let em_height = font_size;
        let ascent = em_height * 0.8;
        let descent = em_height * 0.2;

        // Actual bounding box values are typically slightly smaller than font bounds
        let actual_ascent = ascent * 0.95;
        let actual_descent = descent * 0.9;

        let metrics = ObjectInitializer::new(ctx)
            // Primary width measurement
            .property(js_string!("width"), JsValue::from(width), Attribute::all())
            // Horizontal bounds (distance from alignment point)
            .property(js_string!("actualBoundingBoxLeft"), JsValue::from(0.0), Attribute::all())
            .property(js_string!("actualBoundingBoxRight"), JsValue::from(width), Attribute::all())
            // Font bounding box (em square based)
            .property(js_string!("fontBoundingBoxAscent"), JsValue::from(ascent), Attribute::all())
            .property(js_string!("fontBoundingBoxDescent"), JsValue::from(descent), Attribute::all())
            // Actual glyph bounding box
            .property(js_string!("actualBoundingBoxAscent"), JsValue::from(actual_ascent), Attribute::all())
            .property(js_string!("actualBoundingBoxDescent"), JsValue::from(actual_descent), Attribute::all())
            // Em height metrics
            .property(js_string!("emHeightAscent"), JsValue::from(ascent), Attribute::all())
            .property(js_string!("emHeightDescent"), JsValue::from(descent), Attribute::all())
            // Baseline metrics
            .property(js_string!("hangingBaseline"), JsValue::from(ascent * 0.8), Attribute::all())
            .property(js_string!("alphabeticBaseline"), JsValue::from(0.0), Attribute::all())
            .property(js_string!("ideographicBaseline"), JsValue::from(-descent * 0.5), Attribute::all())
            .build();
        Ok(JsValue::from(metrics))
    }).to_js_function(context.realm());

    // Path methods
    let begin_path = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let close_path = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let move_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let line_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let bezier_curve_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let quadratic_curve_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let arc = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let arc_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let ellipse = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let rect = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let round_rect = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let fill = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let stroke = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let clip = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());

    // Transform methods
    let scale = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let rotate = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let translate = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let transform = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let set_transform = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let reset_transform = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());

    let get_transform = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let matrix = ObjectInitializer::new(ctx)
            .property(js_string!("a"), JsValue::from(1.0), Attribute::all())
            .property(js_string!("b"), JsValue::from(0.0), Attribute::all())
            .property(js_string!("c"), JsValue::from(0.0), Attribute::all())
            .property(js_string!("d"), JsValue::from(1.0), Attribute::all())
            .property(js_string!("e"), JsValue::from(0.0), Attribute::all())
            .property(js_string!("f"), JsValue::from(0.0), Attribute::all())
            .build();
        Ok(JsValue::from(matrix))
    }).to_js_function(context.realm());

    // State methods
    let save = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let restore = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let reset = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());

    // Image methods
    let draw_image = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());
    let put_image_data = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(context.realm());

    let create_image_data = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let w = args.get_or_undefined(0).as_number().unwrap_or(1.0) as u32;
        let h = args.get_or_undefined(1).as_number().unwrap_or(1.0) as u32;
        let data_size = (w * h * 4) as usize;
        let data = ctx.eval(Source::from_bytes(format!("new Uint8ClampedArray({})", data_size).as_bytes()))?;
        let img = ObjectInitializer::new(ctx)
            .property(js_string!("width"), JsValue::from(w), Attribute::all())
            .property(js_string!("height"), JsValue::from(h), Attribute::all())
            .property(js_string!("data"), data, Attribute::all())
            .build();
        Ok(JsValue::from(img))
    }).to_js_function(context.realm());

    let get_image_data = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let w = args.get(2).and_then(|v| v.as_number()).unwrap_or(1.0) as u32;
        let h = args.get(3).and_then(|v| v.as_number()).unwrap_or(1.0) as u32;
        let data_size = (w * h * 4) as usize;
        let data = ctx.eval(Source::from_bytes(format!("new Uint8ClampedArray({})", data_size).as_bytes()))?;
        let img = ObjectInitializer::new(ctx)
            .property(js_string!("width"), JsValue::from(w), Attribute::all())
            .property(js_string!("height"), JsValue::from(h), Attribute::all())
            .property(js_string!("data"), data, Attribute::all())
            .build();
        Ok(JsValue::from(img))
    }).to_js_function(context.realm());

    // Gradient and pattern
    let create_linear_gradient = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let add_stop = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let g = ObjectInitializer::new(ctx).property(js_string!("addColorStop"), JsValue::from(add_stop), Attribute::all()).build();
        Ok(JsValue::from(g))
    }).to_js_function(context.realm());

    let create_radial_gradient = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let add_stop = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let g = ObjectInitializer::new(ctx).property(js_string!("addColorStop"), JsValue::from(add_stop), Attribute::all()).build();
        Ok(JsValue::from(g))
    }).to_js_function(context.realm());

    let create_conic_gradient = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let add_stop = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let g = ObjectInitializer::new(ctx).property(js_string!("addColorStop"), JsValue::from(add_stop), Attribute::all()).build();
        Ok(JsValue::from(g))
    }).to_js_function(context.realm());

    let create_pattern = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let set_trans = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let p = ObjectInitializer::new(ctx).property(js_string!("setTransform"), JsValue::from(set_trans), Attribute::all()).build();
        Ok(JsValue::from(p))
    }).to_js_function(context.realm());

    // Other methods
    let is_point_in_path = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(false))).to_js_function(context.realm());
    let is_point_in_stroke = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(false))).to_js_function(context.realm());
    let to_data_url = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(JsString::from("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==")))
    }).to_js_function(context.realm());

    let get_context_attributes = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let attrs = ObjectInitializer::new(ctx)
            .property(js_string!("alpha"), JsValue::from(true), Attribute::all())
            .property(js_string!("desynchronized"), JsValue::from(false), Attribute::all())
            .build();
        Ok(JsValue::from(attrs))
    }).to_js_function(context.realm());

    // Line dash methods
    let set_line_dash = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let get_line_dash = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        use boa_engine::object::builtins::JsArray;
        let arr = JsArray::new(ctx);
        Ok(JsValue::from(arr))
    }).to_js_function(context.realm());

    // Focus and scroll methods
    let draw_focus_if_needed = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let scroll_path_into_view = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    // Build the context object
    let ctx_obj = ObjectInitializer::new(context)
        .property(js_string!("canvas"), JsValue::null(), Attribute::all())
        .property(js_string!("fillStyle"), JsValue::from(JsString::from("#000000")), Attribute::all())
        .property(js_string!("strokeStyle"), JsValue::from(JsString::from("#000000")), Attribute::all())
        .property(js_string!("lineWidth"), JsValue::from(1.0), Attribute::all())
        .property(js_string!("lineCap"), JsValue::from(JsString::from("butt")), Attribute::all())
        .property(js_string!("lineJoin"), JsValue::from(JsString::from("miter")), Attribute::all())
        .property(js_string!("font"), JsValue::from(JsString::from("10px sans-serif")), Attribute::all())
        .property(js_string!("textAlign"), JsValue::from(JsString::from("start")), Attribute::all())
        .property(js_string!("textBaseline"), JsValue::from(JsString::from("alphabetic")), Attribute::all())
        .property(js_string!("globalAlpha"), JsValue::from(1.0), Attribute::all())
        .property(js_string!("globalCompositeOperation"), JsValue::from(JsString::from("source-over")), Attribute::all())
        .property(js_string!("imageSmoothingEnabled"), JsValue::from(true), Attribute::all())
        .property(js_string!("shadowBlur"), JsValue::from(0.0), Attribute::all())
        .property(js_string!("shadowColor"), JsValue::from(JsString::from("rgba(0,0,0,0)")), Attribute::all())
        .property(js_string!("shadowOffsetX"), JsValue::from(0.0), Attribute::all())
        .property(js_string!("shadowOffsetY"), JsValue::from(0.0), Attribute::all())
        .property(js_string!("fillRect"), JsValue::from(fill_rect), Attribute::all())
        .property(js_string!("strokeRect"), JsValue::from(stroke_rect), Attribute::all())
        .property(js_string!("clearRect"), JsValue::from(clear_rect), Attribute::all())
        .property(js_string!("fillText"), JsValue::from(fill_text), Attribute::all())
        .property(js_string!("strokeText"), JsValue::from(stroke_text), Attribute::all())
        .property(js_string!("measureText"), JsValue::from(measure_text), Attribute::all())
        .property(js_string!("beginPath"), JsValue::from(begin_path), Attribute::all())
        .property(js_string!("closePath"), JsValue::from(close_path), Attribute::all())
        .property(js_string!("moveTo"), JsValue::from(move_to), Attribute::all())
        .property(js_string!("lineTo"), JsValue::from(line_to), Attribute::all())
        .property(js_string!("bezierCurveTo"), JsValue::from(bezier_curve_to), Attribute::all())
        .property(js_string!("quadraticCurveTo"), JsValue::from(quadratic_curve_to), Attribute::all())
        .property(js_string!("arc"), JsValue::from(arc), Attribute::all())
        .property(js_string!("arcTo"), JsValue::from(arc_to), Attribute::all())
        .property(js_string!("ellipse"), JsValue::from(ellipse), Attribute::all())
        .property(js_string!("rect"), JsValue::from(rect), Attribute::all())
        .property(js_string!("roundRect"), JsValue::from(round_rect), Attribute::all())
        .property(js_string!("fill"), JsValue::from(fill), Attribute::all())
        .property(js_string!("stroke"), JsValue::from(stroke), Attribute::all())
        .property(js_string!("clip"), JsValue::from(clip), Attribute::all())
        .property(js_string!("scale"), JsValue::from(scale), Attribute::all())
        .property(js_string!("rotate"), JsValue::from(rotate), Attribute::all())
        .property(js_string!("translate"), JsValue::from(translate), Attribute::all())
        .property(js_string!("transform"), JsValue::from(transform), Attribute::all())
        .property(js_string!("setTransform"), JsValue::from(set_transform), Attribute::all())
        .property(js_string!("resetTransform"), JsValue::from(reset_transform), Attribute::all())
        .property(js_string!("getTransform"), JsValue::from(get_transform), Attribute::all())
        .property(js_string!("save"), JsValue::from(save), Attribute::all())
        .property(js_string!("restore"), JsValue::from(restore), Attribute::all())
        .property(js_string!("reset"), JsValue::from(reset), Attribute::all())
        .property(js_string!("drawImage"), JsValue::from(draw_image), Attribute::all())
        .property(js_string!("createImageData"), JsValue::from(create_image_data), Attribute::all())
        .property(js_string!("getImageData"), JsValue::from(get_image_data), Attribute::all())
        .property(js_string!("putImageData"), JsValue::from(put_image_data), Attribute::all())
        .property(js_string!("createLinearGradient"), JsValue::from(create_linear_gradient), Attribute::all())
        .property(js_string!("createRadialGradient"), JsValue::from(create_radial_gradient), Attribute::all())
        .property(js_string!("createConicGradient"), JsValue::from(create_conic_gradient), Attribute::all())
        .property(js_string!("createPattern"), JsValue::from(create_pattern), Attribute::all())
        .property(js_string!("isPointInPath"), JsValue::from(is_point_in_path), Attribute::all())
        .property(js_string!("isPointInStroke"), JsValue::from(is_point_in_stroke), Attribute::all())
        .property(js_string!("toDataURL"), JsValue::from(to_data_url), Attribute::all())
        .property(js_string!("getContextAttributes"), JsValue::from(get_context_attributes), Attribute::all())
        .property(js_string!("setLineDash"), JsValue::from(set_line_dash), Attribute::all())
        .property(js_string!("getLineDash"), JsValue::from(get_line_dash), Attribute::all())
        .property(js_string!("drawFocusIfNeeded"), JsValue::from(draw_focus_if_needed), Attribute::all())
        .property(js_string!("scrollPathIntoView"), JsValue::from(scroll_path_into_view), Attribute::all())
        .property(js_string!("lineDashOffset"), JsValue::from(0.0), Attribute::all())
        .property(js_string!("miterLimit"), JsValue::from(10.0), Attribute::all())
        .property(js_string!("direction"), JsValue::from(JsString::from("ltr")), Attribute::all())
        .property(js_string!("filter"), JsValue::from(JsString::from("none")), Attribute::all())
        .property(js_string!("fontKerning"), JsValue::from(JsString::from("auto")), Attribute::all())
        .property(js_string!("fontStretch"), JsValue::from(JsString::from("normal")), Attribute::all())
        .property(js_string!("fontVariantCaps"), JsValue::from(JsString::from("normal")), Attribute::all())
        .property(js_string!("letterSpacing"), JsValue::from(JsString::from("0px")), Attribute::all())
        .property(js_string!("wordSpacing"), JsValue::from(JsString::from("0px")), Attribute::all())
        .property(js_string!("textRendering"), JsValue::from(JsString::from("auto")), Attribute::all())
        .property(js_string!("imageSmoothingQuality"), JsValue::from(JsString::from("low")), Attribute::all())
        .build();

    Ok(ctx_obj)
}

/// Register ImageData constructor
fn register_image_data(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let w = args.get_or_undefined(0).as_number().unwrap_or(1.0) as u32;
        let h = args.get_or_undefined(1).as_number().unwrap_or(1.0) as u32;
        let data_size = (w * h * 4) as usize;
        let data = ctx.eval(Source::from_bytes(format!("new Uint8ClampedArray({})", data_size).as_bytes()))?;
        let obj = ObjectInitializer::new(ctx)
            .property(js_string!("width"), JsValue::from(w), Attribute::all())
            .property(js_string!("height"), JsValue::from(h), Attribute::all())
            .property(js_string!("data"), data, Attribute::all())
            .property(js_string!("colorSpace"), JsValue::from(JsString::from("srgb")), Attribute::all())
            .build();
        Ok(JsValue::from(obj))
    });
    context.register_global_builtin_callable(js_string!("ImageData"), 2, constructor)?;
    Ok(())
}

/// Register Path2D constructor
fn register_path_2d(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let add_path = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let close_path = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let move_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let line_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let bezier_curve_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let quadratic_curve_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let arc = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let arc_to = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let ellipse = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let rect = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let round_rect = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());

        let path = ObjectInitializer::new(ctx)
            .property(js_string!("addPath"), JsValue::from(add_path), Attribute::all())
            .property(js_string!("closePath"), JsValue::from(close_path), Attribute::all())
            .property(js_string!("moveTo"), JsValue::from(move_to), Attribute::all())
            .property(js_string!("lineTo"), JsValue::from(line_to), Attribute::all())
            .property(js_string!("bezierCurveTo"), JsValue::from(bezier_curve_to), Attribute::all())
            .property(js_string!("quadraticCurveTo"), JsValue::from(quadratic_curve_to), Attribute::all())
            .property(js_string!("arc"), JsValue::from(arc), Attribute::all())
            .property(js_string!("arcTo"), JsValue::from(arc_to), Attribute::all())
            .property(js_string!("ellipse"), JsValue::from(ellipse), Attribute::all())
            .property(js_string!("rect"), JsValue::from(rect), Attribute::all())
            .property(js_string!("roundRect"), JsValue::from(round_rect), Attribute::all())
            .build();
        Ok(JsValue::from(path))
    });
    context.register_global_builtin_callable(js_string!("Path2D"), 0, constructor)?;
    Ok(())
}

/// Register OffscreenCanvas constructor
fn register_offscreen_canvas(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let w = args.get_or_undefined(0).as_number().unwrap_or(300.0) as u32;
        let h = args.get_or_undefined(1).as_number().unwrap_or(150.0) as u32;

        let get_context = NativeFunction::from_copy_closure(move |_this, args, ctx| {
            let type_str = args.get_or_undefined(0).as_string().map(|s| s.to_std_string_escaped()).unwrap_or_else(|| "2d".to_string());
            if type_str == "2d" {
                let ctx_obj = create_context_2d_object(ctx)?;
                Ok(JsValue::from(ctx_obj))
            } else {
                Ok(JsValue::null())
            }
        }).to_js_function(ctx.realm());

        let convert_to_blob = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let blob = ObjectInitializer::new(ctx)
                .property(js_string!("size"), JsValue::from(0), Attribute::all())
                .property(js_string!("type"), JsValue::from(JsString::from("image/png")), Attribute::all())
                .build();
            let then_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
            let promise = ObjectInitializer::new(ctx)
                .property(js_string!("then"), JsValue::from(then_fn), Attribute::all())
                .build();
            Ok(JsValue::from(promise))
        }).to_js_function(ctx.realm());

        let transfer_to_image_bitmap = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let bitmap = ObjectInitializer::new(ctx)
                .property(js_string!("width"), JsValue::from(300), Attribute::all())
                .property(js_string!("height"), JsValue::from(150), Attribute::all())
                .build();
            Ok(JsValue::from(bitmap))
        }).to_js_function(ctx.realm());

        let canvas = ObjectInitializer::new(ctx)
            .property(js_string!("width"), JsValue::from(w), Attribute::all())
            .property(js_string!("height"), JsValue::from(h), Attribute::all())
            .property(js_string!("getContext"), JsValue::from(get_context), Attribute::all())
            .property(js_string!("convertToBlob"), JsValue::from(convert_to_blob), Attribute::all())
            .property(js_string!("transferToImageBitmap"), JsValue::from(transfer_to_image_bitmap), Attribute::all())
            .build();
        Ok(JsValue::from(canvas))
    });
    context.register_global_builtin_callable(js_string!("OffscreenCanvas"), 2, constructor)?;
    Ok(())
}

/// Register CanvasGradient
fn register_canvas_gradient(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let add_color_stop = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let gradient = ObjectInitializer::new(ctx)
            .property(js_string!("addColorStop"), JsValue::from(add_color_stop), Attribute::all())
            .build();
        Ok(JsValue::from(gradient))
    });
    context.register_global_builtin_callable(js_string!("CanvasGradient"), 0, constructor)?;
    Ok(())
}

/// Register CanvasPattern
fn register_canvas_pattern(context: &mut Context) -> JsResult<()> {
    let constructor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let set_transform = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())).to_js_function(ctx.realm());
        let pattern = ObjectInitializer::new(ctx)
            .property(js_string!("setTransform"), JsValue::from(set_transform), Attribute::all())
            .build();
        Ok(JsValue::from(pattern))
    });
    context.register_global_builtin_callable(js_string!("CanvasPattern"), 0, constructor)?;
    Ok(())
}

/// Parse a CSS font string to extract font size and detect monospace
/// Format: "[style] [variant] [weight] [stretch] size[/lineHeight] family[, family]*"
fn parse_font_string(font: &str) -> (f64, bool) {
    let font_lower = font.to_lowercase();

    // Check if it's a monospace font
    let is_monospace = font_lower.contains("monospace")
        || font_lower.contains("courier")
        || font_lower.contains("consolas")
        || font_lower.contains("monaco")
        || font_lower.contains("menlo")
        || font_lower.contains("source code")
        || font_lower.contains("fira code")
        || font_lower.contains("jetbrains");

    // Parse font size - look for patterns like "16px", "1.5em", "12pt", "100%"
    let size = parse_font_size_from_string(font);

    (size, is_monospace)
}

/// Extract font size from font string
fn parse_font_size_from_string(font: &str) -> f64 {
    // Common patterns: "16px", "1.5em", "12pt", "100%", "medium", "large", etc.
    let parts: Vec<&str> = font.split_whitespace().collect();

    for part in &parts {
        // Try px
        if let Some(num) = part.strip_suffix("px") {
            if let Ok(size) = num.parse::<f64>() {
                return size;
            }
        }
        // Try em (relative to 16px base)
        if let Some(num) = part.strip_suffix("em") {
            if let Ok(size) = num.parse::<f64>() {
                return size * 16.0;
            }
        }
        // Try rem (relative to 16px base)
        if let Some(num) = part.strip_suffix("rem") {
            if let Ok(size) = num.parse::<f64>() {
                return size * 16.0;
            }
        }
        // Try pt (points, ~1.33px per pt)
        if let Some(num) = part.strip_suffix("pt") {
            if let Ok(size) = num.parse::<f64>() {
                return size * 1.333;
            }
        }
        // Try % (percentage of 16px base)
        if let Some(num) = part.strip_suffix('%') {
            if let Ok(size) = num.parse::<f64>() {
                return size / 100.0 * 16.0;
            }
        }
    }

    // Handle keyword sizes
    let font_lower = font.to_lowercase();
    if font_lower.contains("xx-small") { return 9.0; }
    if font_lower.contains("x-small") { return 10.0; }
    if font_lower.contains("small") { return 13.0; }
    if font_lower.contains("medium") { return 16.0; }
    if font_lower.contains("large") && font_lower.contains("x-large") { return 24.0; }
    if font_lower.contains("large") && font_lower.contains("xx-large") { return 32.0; }
    if font_lower.contains("large") { return 18.0; }

    // Default font size
    10.0
}

/// Calculate estimated text width based on character content and font properties
fn calculate_text_width(text: &str, font_size: f64, is_monospace: bool) -> f64 {
    if text.is_empty() {
        return 0.0;
    }

    if is_monospace {
        // Monospace fonts: all characters have the same width (~0.6 em)
        text.chars().count() as f64 * font_size * 0.6
    } else {
        // Proportional fonts: estimate based on character categories
        let mut total_width = 0.0;

        for ch in text.chars() {
            let char_width = match ch {
                // Narrow characters
                'i' | 'j' | 'l' | '!' | '|' | '\'' | '`' | '.' | ',' | ':' | ';' => 0.3,
                'f' | 't' | 'r' => 0.35,
                'I' | '1' => 0.35,

                // Medium-narrow characters
                'a' | 'c' | 'e' | 'g' | 'n' | 'o' | 'p' | 's' | 'u' | 'v' | 'x' | 'y' | 'z' => 0.5,
                'b' | 'd' | 'h' | 'k' | 'q' => 0.55,

                // Medium characters
                '0'..='9' => 0.55,
                'A'..='H' | 'J'..='N' | 'P'..='Z' => 0.65,

                // Wide characters
                'm' | 'w' => 0.75,
                'M' | 'W' => 0.85,

                // Spaces
                ' ' => 0.3,
                '\t' => 1.2,

                // Other punctuation and symbols
                '-' | '_' => 0.5,
                '/' | '\\' => 0.4,
                '(' | ')' | '[' | ']' | '{' | '}' => 0.35,
                '@' => 0.9,
                '#' | '$' | '%' | '&' | '*' | '+' | '=' => 0.6,

                // CJK and wide characters
                '\u{4E00}'..='\u{9FFF}' => 1.0,   // CJK Unified Ideographs
                '\u{3040}'..='\u{309F}' => 1.0,   // Hiragana
                '\u{30A0}'..='\u{30FF}' => 1.0,   // Katakana
                '\u{AC00}'..='\u{D7AF}' => 1.0,   // Hangul Syllables

                // Emoji and other wide chars
                '\u{1F300}'..='\u{1F9FF}' => 1.0,

                // Default for other characters
                _ => 0.55,
            };

            total_width += char_width * font_size;
        }

        total_width
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canvas_rendering_context_2d_exists() {
        let mut context = Context::default();
        register_all_canvas_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof CanvasRenderingContext2D")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "function");
    }

    #[test]
    fn test_image_data_exists() {
        let mut context = Context::default();
        register_all_canvas_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof ImageData")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "function");
    }

    #[test]
    fn test_path_2d_exists() {
        let mut context = Context::default();
        register_all_canvas_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof Path2D")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "function");
    }

    #[test]
    fn test_offscreen_canvas_exists() {
        let mut context = Context::default();
        register_all_canvas_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof OffscreenCanvas")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "function");
    }

    #[test]
    fn test_canvas_gradient_exists() {
        let mut context = Context::default();
        register_all_canvas_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof CanvasGradient")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "function");
    }

    #[test]
    fn test_canvas_pattern_exists() {
        let mut context = Context::default();
        register_all_canvas_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(b"typeof CanvasPattern")).unwrap();
        assert_eq!(result.as_string().unwrap().to_std_string_escaped(), "function");
    }

    #[test]
    fn test_parse_font_string_px() {
        let (size, is_mono) = parse_font_string("16px Arial");
        assert!((size - 16.0).abs() < 0.01);
        assert!(!is_mono);
    }

    #[test]
    fn test_parse_font_string_monospace() {
        let (size, is_mono) = parse_font_string("14px monospace");
        assert!((size - 14.0).abs() < 0.01);
        assert!(is_mono);
    }

    #[test]
    fn test_parse_font_string_em() {
        let (size, _) = parse_font_string("1.5em sans-serif");
        assert!((size - 24.0).abs() < 0.01); // 1.5 * 16 = 24
    }

    #[test]
    fn test_calculate_text_width_proportional() {
        // "Hello" with varying character widths
        let width = calculate_text_width("Hello", 16.0, false);
        // H=0.65, e=0.5, l=0.3, l=0.3, o=0.5 = 2.25 * 16 = 36
        assert!(width > 30.0 && width < 40.0);
    }

    #[test]
    fn test_calculate_text_width_monospace() {
        // Monospace: 5 chars * 0.6 * 16 = 48
        let width = calculate_text_width("Hello", 16.0, true);
        assert!((width - 48.0).abs() < 0.01);
    }

    #[test]
    fn test_calculate_text_width_wide_chars() {
        // Wide characters should be wider
        let narrow = calculate_text_width("iiii", 16.0, false);
        let wide = calculate_text_width("MMMM", 16.0, false);
        assert!(wide > narrow * 2.0);
    }

    #[test]
    fn test_measure_text_returns_all_properties() {
        let mut context = Context::default();
        register_all_canvas_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(br#"
            var ctx = new CanvasRenderingContext2D();
            var metrics = ctx.measureText('Hello');
            [
                typeof metrics.width,
                typeof metrics.actualBoundingBoxAscent,
                typeof metrics.actualBoundingBoxDescent,
                typeof metrics.fontBoundingBoxAscent,
                typeof metrics.fontBoundingBoxDescent,
                typeof metrics.actualBoundingBoxLeft,
                typeof metrics.actualBoundingBoxRight
            ].join(',');
        "#)).unwrap();
        let types = result.as_string().unwrap().to_std_string_escaped();
        assert_eq!(types, "number,number,number,number,number,number,number");
    }

    #[test]
    fn test_measure_text_width_varies_with_font_size() {
        let mut context = Context::default();
        register_all_canvas_apis(&mut context).unwrap();
        let result = context.eval(Source::from_bytes(br#"
            var ctx = new CanvasRenderingContext2D();
            ctx.font = '10px sans-serif';
            var w1 = ctx.measureText('Hello').width;
            ctx.font = '20px sans-serif';
            var w2 = ctx.measureText('Hello').width;
            w2 > w1;
        "#)).unwrap();
        assert_eq!(result.as_boolean().unwrap(), true);
    }
}
