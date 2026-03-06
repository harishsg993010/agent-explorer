//! Web Animations API
//!
//! Implements:
//! - Animation
//! - KeyframeEffect
//! - AnimationTimeline
//! - DocumentTimeline
//! - Element.animate()

use boa_engine::{
    Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue,
    NativeFunction, js_string, object::ObjectInitializer, object::builtins::JsArray,
    object::FunctionObjectBuilder, property::Attribute,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

lazy_static::lazy_static! {
    /// Animation storage
    static ref ANIMATIONS: Arc<Mutex<HashMap<u32, AnimationData>>> =
        Arc::new(Mutex::new(HashMap::new()));

    /// Animation ID counter
    static ref ANIMATION_COUNTER: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));

    /// Document timeline
    static ref DOCUMENT_TIMELINE: Arc<Mutex<TimelineData>> =
        Arc::new(Mutex::new(TimelineData::default()));
}

/// Animation data structure
#[derive(Debug, Clone)]
struct AnimationData {
    id: u32,
    play_state: String, // "idle", "running", "paused", "finished"
    playback_rate: f64,
    start_time: Option<f64>,
    current_time: Option<f64>,
    effect: Option<EffectData>,
    pending_play: bool,
    pending_pause: bool,
}

impl Default for AnimationData {
    fn default() -> Self {
        Self {
            id: 0,
            play_state: "idle".to_string(),
            playback_rate: 1.0,
            start_time: None,
            current_time: None,
            effect: None,
            pending_play: false,
            pending_pause: false,
        }
    }
}

/// Effect data structure
#[derive(Debug, Clone)]
struct EffectData {
    keyframes: Vec<HashMap<String, String>>,
    duration: f64,
    iterations: f64,
    delay: f64,
    end_delay: f64,
    direction: String,
    easing: String,
    fill: String,
    iteration_start: f64,
}

impl Default for EffectData {
    fn default() -> Self {
        Self {
            keyframes: Vec::new(),
            duration: 0.0,
            iterations: 1.0,
            delay: 0.0,
            end_delay: 0.0,
            direction: "normal".to_string(),
            easing: "linear".to_string(),
            fill: "auto".to_string(),
            iteration_start: 0.0,
        }
    }
}

/// Timeline data
#[derive(Debug, Clone, Default)]
struct TimelineData {
    current_time: f64,
}

/// Register all animation APIs
pub fn register_all_animation_apis(context: &mut Context) -> JsResult<()> {
    register_animation(context)?;
    register_keyframe_effect(context)?;
    register_animation_timeline(context)?;
    register_document_timeline(context)?;
    register_animation_effect(context)?;
    register_element_animate_helper(context)?;
    Ok(())
}

/// Register Animation constructor
fn register_animation(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let mut id = ANIMATION_COUNTER.lock().unwrap();
        *id += 1;
        let anim_id = *id;
        drop(id);

        let mut anim_data = AnimationData::default();
        anim_data.id = anim_id;

        // Get effect if provided
        if let Some(effect) = args.get(0) {
            if let Some(obj) = effect.as_object() {
                // Try to extract effect data
                if let Ok(timing) = obj.get(js_string!("getTiming"), ctx) {
                    if let Some(timing_fn) = timing.as_callable() {
                        if let Ok(timing_result) = timing_fn.call(&JsValue::from(obj.clone()), &[], ctx) {
                            if let Some(timing_obj) = timing_result.as_object() {
                                anim_data.effect = Some(EffectData {
                                    duration: timing_obj.get(js_string!("duration"), ctx)
                                        .ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(0.0),
                                    iterations: timing_obj.get(js_string!("iterations"), ctx)
                                        .ok().and_then(|v| v.to_number(ctx).ok()).unwrap_or(1.0),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
            }
        }

        ANIMATIONS.lock().unwrap().insert(anim_id, anim_data);

        let animation = create_animation_object(ctx, anim_id)?;
        Ok(JsValue::from(animation))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("Animation"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("Animation"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register Animation: {}", e)))?;

    Ok(())
}

/// Create Animation object with all methods
fn create_animation_object(context: &mut Context, id: u32) -> JsResult<JsObject> {
    // play()
    let id_play = id;
    let play = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        if let Ok(mut animations) = ANIMATIONS.lock() {
            if let Some(anim) = animations.get_mut(&id_play) {
                anim.play_state = "running".to_string();
                anim.pending_play = true;
            }
        }
        Ok(JsValue::undefined())
    });

    // pause()
    let id_pause = id;
    let pause = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        if let Ok(mut animations) = ANIMATIONS.lock() {
            if let Some(anim) = animations.get_mut(&id_pause) {
                anim.play_state = "paused".to_string();
                anim.pending_pause = true;
            }
        }
        Ok(JsValue::undefined())
    });

    // cancel()
    let id_cancel = id;
    let cancel = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        if let Ok(mut animations) = ANIMATIONS.lock() {
            if let Some(anim) = animations.get_mut(&id_cancel) {
                anim.play_state = "idle".to_string();
                anim.current_time = None;
                anim.start_time = None;
            }
        }
        Ok(JsValue::undefined())
    });

    // finish()
    let id_finish = id;
    let finish = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        if let Ok(mut animations) = ANIMATIONS.lock() {
            if let Some(anim) = animations.get_mut(&id_finish) {
                anim.play_state = "finished".to_string();
                if let Some(ref effect) = anim.effect {
                    anim.current_time = Some(effect.duration);
                }
            }
        }
        Ok(JsValue::undefined())
    });

    // reverse()
    let id_reverse = id;
    let reverse = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        if let Ok(mut animations) = ANIMATIONS.lock() {
            if let Some(anim) = animations.get_mut(&id_reverse) {
                anim.playback_rate = -anim.playback_rate;
            }
        }
        Ok(JsValue::undefined())
    });

    // updatePlaybackRate(rate)
    let id_update_rate = id;
    let update_playback_rate = NativeFunction::from_copy_closure(move |_this, args, ctx| {
        let rate = args.get_or_undefined(0).to_number(ctx).unwrap_or(1.0);
        if let Ok(mut animations) = ANIMATIONS.lock() {
            if let Some(anim) = animations.get_mut(&id_update_rate) {
                anim.playback_rate = rate;
            }
        }
        Ok(JsValue::undefined())
    });

    // persist()
    let persist = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // commitStyles()
    let commit_styles = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // Convert all to js_function
    let play_fn = play.to_js_function(context.realm());
    let pause_fn = pause.to_js_function(context.realm());
    let cancel_fn = cancel.to_js_function(context.realm());
    let finish_fn = finish.to_js_function(context.realm());
    let reverse_fn = reverse.to_js_function(context.realm());
    let update_rate_fn = update_playback_rate.to_js_function(context.realm());
    let persist_fn = persist.to_js_function(context.realm());
    let commit_fn = commit_styles.to_js_function(context.realm());

    // Create finished promise
    let finished_promise = create_resolved_promise(context)?;
    let ready_promise = create_resolved_promise(context)?;

    // Get current state
    let (play_state, playback_rate, current_time, start_time) = ANIMATIONS.lock()
        .map(|a| a.get(&id).map(|anim| (
            anim.play_state.clone(),
            anim.playback_rate,
            anim.current_time,
            anim.start_time,
        )).unwrap_or(("idle".to_string(), 1.0, None, None)))
        .unwrap_or(("idle".to_string(), 1.0, None, None));

    let animation = ObjectInitializer::new(context)
        .property(js_string!("id"), JsValue::from(js_string!("")), Attribute::all())
        .property(js_string!("playState"), JsValue::from(js_string!(play_state.as_str())), Attribute::all())
        .property(js_string!("playbackRate"), JsValue::from(playback_rate), Attribute::all())
        .property(js_string!("currentTime"), current_time.map(JsValue::from).unwrap_or(JsValue::null()), Attribute::all())
        .property(js_string!("startTime"), start_time.map(JsValue::from).unwrap_or(JsValue::null()), Attribute::all())
        .property(js_string!("effect"), JsValue::null(), Attribute::all())
        .property(js_string!("timeline"), JsValue::null(), Attribute::all())
        .property(js_string!("pending"), JsValue::from(false), Attribute::all())
        .property(js_string!("replaceState"), JsValue::from(js_string!("active")), Attribute::all())
        .property(js_string!("finished"), JsValue::from(finished_promise), Attribute::all())
        .property(js_string!("ready"), JsValue::from(ready_promise), Attribute::all())
        // Methods
        .property(js_string!("play"), JsValue::from(play_fn), Attribute::all())
        .property(js_string!("pause"), JsValue::from(pause_fn), Attribute::all())
        .property(js_string!("cancel"), JsValue::from(cancel_fn), Attribute::all())
        .property(js_string!("finish"), JsValue::from(finish_fn), Attribute::all())
        .property(js_string!("reverse"), JsValue::from(reverse_fn), Attribute::all())
        .property(js_string!("updatePlaybackRate"), JsValue::from(update_rate_fn), Attribute::all())
        .property(js_string!("persist"), JsValue::from(persist_fn), Attribute::all())
        .property(js_string!("commitStyles"), JsValue::from(commit_fn), Attribute::all())
        // Event handlers
        .property(js_string!("onfinish"), JsValue::null(), Attribute::all())
        .property(js_string!("oncancel"), JsValue::null(), Attribute::all())
        .property(js_string!("onremove"), JsValue::null(), Attribute::all())
        .build();

    Ok(animation)
}

/// Create a resolved promise
fn create_resolved_promise(context: &mut Context) -> JsResult<JsObject> {
    let then = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(cb) = args.get_or_undefined(0).as_callable() {
            let _ = cb.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let catch = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let finally = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(cb) = args.get_or_undefined(0).as_callable() {
            let _ = cb.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(JsValue::undefined())
    }).to_js_function(context.realm());

    let promise = ObjectInitializer::new(context)
        .property(js_string!("then"), JsValue::from(then), Attribute::all())
        .property(js_string!("catch"), JsValue::from(catch), Attribute::all())
        .property(js_string!("finally"), JsValue::from(finally), Attribute::all())
        .build();

    Ok(promise)
}

/// Register KeyframeEffect constructor
fn register_keyframe_effect(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let target = args.get_or_undefined(0);
        let keyframes = args.get_or_undefined(1);
        let options = args.get_or_undefined(2);

        let effect = create_keyframe_effect_object(ctx, target, keyframes, options)?;
        Ok(JsValue::from(effect))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("KeyframeEffect"))
        .length(1)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("KeyframeEffect"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register KeyframeEffect: {}", e)))?;

    Ok(())
}

/// Create KeyframeEffect object
fn create_keyframe_effect_object(
    context: &mut Context,
    _target: &JsValue,
    _keyframes: &JsValue,
    options: &JsValue,
) -> JsResult<JsObject> {
    // Parse options
    let (duration, iterations, delay, end_delay, direction, easing, fill) =
        if let Some(obj) = options.as_object() {
            (
                obj.get(js_string!("duration"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
                obj.get(js_string!("iterations"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(1.0),
                obj.get(js_string!("delay"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
                obj.get(js_string!("endDelay"), context).ok().and_then(|v| v.to_number(context).ok()).unwrap_or(0.0),
                obj.get(js_string!("direction"), context).ok()
                    .and_then(|v| v.to_string(context).ok())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|| "normal".to_string()),
                obj.get(js_string!("easing"), context).ok()
                    .and_then(|v| v.to_string(context).ok())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|| "linear".to_string()),
                obj.get(js_string!("fill"), context).ok()
                    .and_then(|v| v.to_string(context).ok())
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_else(|| "auto".to_string()),
            )
        } else if let Ok(num) = options.to_number(context) {
            (num, 1.0, 0.0, 0.0, "normal".to_string(), "linear".to_string(), "auto".to_string())
        } else {
            (0.0, 1.0, 0.0, 0.0, "normal".to_string(), "linear".to_string(), "auto".to_string())
        };

    // Clone strings for closures
    let direction1 = direction.clone();
    let easing1 = easing.clone();
    let fill1 = fill.clone();

    // getTiming()
    let get_timing = unsafe { NativeFunction::from_closure(move |_this, _args, ctx| {
        let timing = ObjectInitializer::new(ctx)
            .property(js_string!("duration"), JsValue::from(duration), Attribute::all())
            .property(js_string!("iterations"), JsValue::from(iterations), Attribute::all())
            .property(js_string!("delay"), JsValue::from(delay), Attribute::all())
            .property(js_string!("endDelay"), JsValue::from(end_delay), Attribute::all())
            .property(js_string!("direction"), JsValue::from(js_string!(direction1.as_str())), Attribute::all())
            .property(js_string!("easing"), JsValue::from(js_string!(easing1.as_str())), Attribute::all())
            .property(js_string!("fill"), JsValue::from(js_string!(fill1.as_str())), Attribute::all())
            .property(js_string!("iterationStart"), JsValue::from(0.0), Attribute::all())
            .build();
        Ok(JsValue::from(timing))
    }) };

    // getComputedTiming()
    let get_computed = NativeFunction::from_copy_closure(move |_this, _args, ctx| {
        let timing = ObjectInitializer::new(ctx)
            .property(js_string!("duration"), JsValue::from(duration), Attribute::all())
            .property(js_string!("iterations"), JsValue::from(iterations), Attribute::all())
            .property(js_string!("delay"), JsValue::from(delay), Attribute::all())
            .property(js_string!("endDelay"), JsValue::from(end_delay), Attribute::all())
            .property(js_string!("activeDuration"), JsValue::from(duration * iterations), Attribute::all())
            .property(js_string!("endTime"), JsValue::from(delay + duration * iterations + end_delay), Attribute::all())
            .property(js_string!("localTime"), JsValue::null(), Attribute::all())
            .property(js_string!("progress"), JsValue::null(), Attribute::all())
            .property(js_string!("currentIteration"), JsValue::null(), Attribute::all())
            .build();
        Ok(JsValue::from(timing))
    });

    // updateTiming(options)
    let update_timing = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    // getKeyframes()
    let get_keyframes = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        Ok(JsValue::from(JsArray::new(ctx)))
    });

    // setKeyframes(keyframes)
    let set_keyframes = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let get_timing_fn = get_timing.to_js_function(context.realm());
    let get_computed_fn = get_computed.to_js_function(context.realm());
    let update_timing_fn = update_timing.to_js_function(context.realm());
    let get_keyframes_fn = get_keyframes.to_js_function(context.realm());
    let set_keyframes_fn = set_keyframes.to_js_function(context.realm());

    let effect = ObjectInitializer::new(context)
        .property(js_string!("target"), JsValue::null(), Attribute::all())
        .property(js_string!("pseudoElement"), JsValue::null(), Attribute::all())
        .property(js_string!("composite"), JsValue::from(js_string!("replace")), Attribute::all())
        .property(js_string!("getTiming"), JsValue::from(get_timing_fn), Attribute::all())
        .property(js_string!("getComputedTiming"), JsValue::from(get_computed_fn), Attribute::all())
        .property(js_string!("updateTiming"), JsValue::from(update_timing_fn), Attribute::all())
        .property(js_string!("getKeyframes"), JsValue::from(get_keyframes_fn), Attribute::all())
        .property(js_string!("setKeyframes"), JsValue::from(set_keyframes_fn), Attribute::all())
        .build();

    Ok(effect)
}

/// Register AnimationTimeline
fn register_animation_timeline(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let timeline = ObjectInitializer::new(ctx)
            .property(js_string!("currentTime"), JsValue::from(0.0), Attribute::all())
            .build();
        Ok(JsValue::from(timeline))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("AnimationTimeline"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("AnimationTimeline"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register AnimationTimeline: {}", e)))?;

    Ok(())
}

/// Register DocumentTimeline
fn register_document_timeline(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let origin_time = args.get(0)
            .and_then(|o| o.as_object())
            .and_then(|obj| obj.get(js_string!("originTime"), ctx).ok())
            .and_then(|v| v.to_number(ctx).ok())
            .unwrap_or(0.0);

        let timeline = ObjectInitializer::new(ctx)
            .property(js_string!("currentTime"), JsValue::from(0.0), Attribute::all())
            .property(js_string!("originTime"), JsValue::from(origin_time), Attribute::all())
            .build();
        Ok(JsValue::from(timeline))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("DocumentTimeline"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("DocumentTimeline"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register DocumentTimeline: {}", e)))?;

    Ok(())
}

/// Register AnimationEffect base class
fn register_animation_effect(context: &mut Context) -> JsResult<()> {
    let constructor_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let get_timing = NativeFunction::from_copy_closure(|_this, _args, ctx| {
            let timing = ObjectInitializer::new(ctx)
                .property(js_string!("duration"), JsValue::from(0.0), Attribute::all())
                .property(js_string!("iterations"), JsValue::from(1.0), Attribute::all())
                .build();
            Ok(JsValue::from(timing))
        }).to_js_function(ctx.realm());

        let effect = ObjectInitializer::new(ctx)
            .property(js_string!("getTiming"), JsValue::from(get_timing), Attribute::all())
            .build();
        Ok(JsValue::from(effect))
    });

    let constructor = FunctionObjectBuilder::new(context.realm(), constructor_fn)
        .name(js_string!("AnimationEffect"))
        .length(0)
        .constructor(true)
        .build();

    context.register_global_property(js_string!("AnimationEffect"), constructor, Attribute::all())
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register AnimationEffect: {}", e)))?;

    Ok(())
}

/// Register helper for Element.animate()
fn register_element_animate_helper(context: &mut Context) -> JsResult<()> {
    // This creates the animate function that can be attached to elements
    let animate_helper = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let keyframes = args.get_or_undefined(0);
        let options = args.get_or_undefined(1);

        // Create KeyframeEffect
        let effect = create_keyframe_effect_object(ctx, &JsValue::undefined(), keyframes, options)?;

        // Create Animation with effect
        let mut id = ANIMATION_COUNTER.lock().unwrap();
        *id += 1;
        let anim_id = *id;
        drop(id);

        let mut anim_data = AnimationData::default();
        anim_data.id = anim_id;
        anim_data.play_state = "running".to_string();

        ANIMATIONS.lock().unwrap().insert(anim_id, anim_data);

        let animation = create_animation_object(ctx, anim_id)?;
        Ok(JsValue::from(animation))
    });

    context.register_global_builtin_callable(js_string!("__animateElement"), 2, animate_helper)
        .map_err(|e| JsNativeError::error().with_message(format!("Failed to register animate helper: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;

    fn create_test_context() -> Context {
        let mut ctx = Context::default();
        register_all_animation_apis(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn test_animation_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof Animation === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_keyframe_effect_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof KeyframeEffect === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_animation_methods() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            var anim = new Animation();
            typeof anim.play === 'function' &&
            typeof anim.pause === 'function' &&
            typeof anim.cancel === 'function' &&
            typeof anim.finish === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_document_timeline_exists() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            typeof DocumentTimeline === 'function'
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }

    #[test]
    fn test_keyframe_effect_timing() {
        let mut ctx = create_test_context();
        let result = ctx.eval(Source::from_bytes(r#"
            var effect = new KeyframeEffect(null, [], { duration: 1000, iterations: 2 });
            var timing = effect.getTiming();
            timing.duration === 1000 && timing.iterations === 2
        "#));
        assert!(result.is_ok());
        if let Ok(val) = result {
            assert!(val.to_boolean());
        }
    }
}
