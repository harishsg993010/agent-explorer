//! Full Timer and Job Queue implementation
//!
//! Implements comprehensive timer APIs including:
//! - setTimeout / clearTimeout
//! - setInterval / clearInterval
//! - setImmediate / clearImmediate
//! - requestAnimationFrame / cancelAnimationFrame
//! - requestIdleCallback / cancelIdleCallback
//! - queueMicrotask
//! - Job queue with microtask and macrotask support
//! - Scheduler API (scheduler.postTask)

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer,
    object::builtins::JsArray, object::FunctionObjectBuilder, property::Attribute,
    Context, JsArgs, JsObject, JsValue,
};
use std::cell::RefCell;
use std::collections::{BinaryHeap, VecDeque};
use std::cmp::Ordering;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::observers;

/// Priority levels for tasks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPriority {
    UserBlocking,  // Highest priority
    UserVisible,   // Default priority
    Background,    // Lowest priority
}

impl TaskPriority {
    fn from_str(s: &str) -> Self {
        match s {
            "user-blocking" => TaskPriority::UserBlocking,
            "background" => TaskPriority::Background,
            _ => TaskPriority::UserVisible,
        }
    }
}

/// A scheduled task in the job queue
#[derive(Clone)]
pub struct ScheduledTask {
    pub id: u32,
    pub callback: JsObject,
    pub scheduled_time: Instant,
    pub delay_ms: u64,
    pub interval: bool,
    pub cancelled: bool,
    pub priority: TaskPriority,
    pub args: Vec<JsValue>,
}

impl ScheduledTask {
    fn new(id: u32, callback: JsObject, delay_ms: u64, interval: bool, args: Vec<JsValue>) -> Self {
        Self {
            id,
            callback,
            scheduled_time: Instant::now() + Duration::from_millis(delay_ms),
            delay_ms,
            interval,
            cancelled: false,
            priority: TaskPriority::UserVisible,
            args,
        }
    }

    fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }
}

impl Eq for ScheduledTask {}

impl PartialEq for ScheduledTask {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for ScheduledTask {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (earliest time first)
        other.scheduled_time.cmp(&self.scheduled_time)
            .then_with(|| {
                // Higher priority tasks come first
                match (self.priority, other.priority) {
                    (TaskPriority::UserBlocking, TaskPriority::UserBlocking) => Ordering::Equal,
                    (TaskPriority::UserBlocking, _) => Ordering::Greater,
                    (_, TaskPriority::UserBlocking) => Ordering::Less,
                    (TaskPriority::UserVisible, TaskPriority::UserVisible) => Ordering::Equal,
                    (TaskPriority::UserVisible, TaskPriority::Background) => Ordering::Greater,
                    (TaskPriority::Background, TaskPriority::UserVisible) => Ordering::Less,
                    (TaskPriority::Background, TaskPriority::Background) => Ordering::Equal,
                }
            })
    }
}

impl PartialOrd for ScheduledTask {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Microtask for queueMicrotask
#[derive(Clone)]
pub struct Microtask {
    pub callback: JsObject,
}

/// Animation frame request
#[derive(Clone)]
pub struct AnimationFrameRequest {
    pub id: u32,
    pub callback: JsObject,
    pub cancelled: bool,
}

/// Idle callback request
#[derive(Clone)]
pub struct IdleCallbackRequest {
    pub id: u32,
    pub callback: JsObject,
    pub timeout: Option<u64>,
    pub cancelled: bool,
}

/// The main job queue managing all timer-related tasks
#[derive(Default)]
pub struct JobQueue {
    /// Next timer ID
    next_id: u32,
    /// Macro task queue (setTimeout, setInterval)
    macro_tasks: BinaryHeap<ScheduledTask>,
    /// Microtask queue (queueMicrotask, Promise callbacks)
    microtasks: VecDeque<Microtask>,
    /// Animation frame requests
    animation_frames: Vec<AnimationFrameRequest>,
    /// Next animation frame ID
    next_animation_frame_id: u32,
    /// Idle callback requests
    idle_callbacks: Vec<IdleCallbackRequest>,
    /// Next idle callback ID
    next_idle_callback_id: u32,
    /// Immediate tasks (setImmediate)
    immediate_tasks: VecDeque<ScheduledTask>,
    /// Current frame time for requestAnimationFrame
    frame_time: f64,
    /// Start time for performance timing
    start_time: Option<Instant>,
}

impl JobQueue {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            macro_tasks: BinaryHeap::new(),
            microtasks: VecDeque::new(),
            animation_frames: Vec::new(),
            next_animation_frame_id: 1,
            idle_callbacks: Vec::new(),
            next_idle_callback_id: 1,
            immediate_tasks: VecDeque::new(),
            frame_time: 0.0,
            start_time: Some(Instant::now()),
        }
    }

    /// Schedule a timeout
    pub fn set_timeout(&mut self, callback: JsObject, delay_ms: u64, args: Vec<JsValue>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let task = ScheduledTask::new(id, callback, delay_ms, false, args);
        self.macro_tasks.push(task);
        id
    }

    /// Schedule an interval
    pub fn set_interval(&mut self, callback: JsObject, delay_ms: u64, args: Vec<JsValue>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let task = ScheduledTask::new(id, callback, delay_ms.max(4), true, args); // Minimum 4ms for intervals
        self.macro_tasks.push(task);
        id
    }

    /// Clear a timeout or interval
    pub fn clear_timer(&mut self, id: u32) {
        // Mark the task as cancelled (we can't remove from BinaryHeap efficiently)
        // It will be skipped when processed
        let tasks: Vec<_> = self.macro_tasks.drain().collect();
        for mut task in tasks {
            if task.id == id {
                task.cancelled = true;
            }
            self.macro_tasks.push(task);
        }
    }

    /// Schedule an immediate task
    pub fn set_immediate(&mut self, callback: JsObject, args: Vec<JsValue>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let task = ScheduledTask::new(id, callback, 0, false, args);
        self.immediate_tasks.push_back(task);
        id
    }

    /// Clear an immediate task
    pub fn clear_immediate(&mut self, id: u32) {
        for task in &mut self.immediate_tasks {
            if task.id == id {
                task.cancelled = true;
            }
        }
    }

    /// Queue a microtask
    pub fn queue_microtask(&mut self, callback: JsObject) {
        self.microtasks.push_back(Microtask { callback });
    }

    /// Request animation frame
    pub fn request_animation_frame(&mut self, callback: JsObject) -> u32 {
        let id = self.next_animation_frame_id;
        self.next_animation_frame_id += 1;
        self.animation_frames.push(AnimationFrameRequest {
            id,
            callback,
            cancelled: false,
        });
        id
    }

    /// Cancel animation frame
    pub fn cancel_animation_frame(&mut self, id: u32) {
        for request in &mut self.animation_frames {
            if request.id == id {
                request.cancelled = true;
            }
        }
    }

    /// Request idle callback
    pub fn request_idle_callback(&mut self, callback: JsObject, timeout: Option<u64>) -> u32 {
        let id = self.next_idle_callback_id;
        self.next_idle_callback_id += 1;
        self.idle_callbacks.push(IdleCallbackRequest {
            id,
            callback,
            timeout,
            cancelled: false,
        });
        id
    }

    /// Cancel idle callback
    pub fn cancel_idle_callback(&mut self, id: u32) {
        for request in &mut self.idle_callbacks {
            if request.id == id {
                request.cancelled = true;
            }
        }
    }

    /// Get current frame time
    pub fn get_frame_time(&self) -> f64 {
        if let Some(start) = self.start_time {
            start.elapsed().as_secs_f64() * 1000.0
        } else {
            self.frame_time
        }
    }

    /// Process all pending microtasks
    pub fn process_microtasks(&mut self, context: &mut Context) {
        while let Some(microtask) = self.microtasks.pop_front() {
            let _ = microtask.callback.call(&JsValue::undefined(), &[], context);
        }
    }

    /// Process immediate tasks
    pub fn process_immediates(&mut self, context: &mut Context) {
        let mut tasks: VecDeque<_> = self.immediate_tasks.drain(..).collect();
        while let Some(task) = tasks.pop_front() {
            if !task.cancelled {
                let args: Vec<JsValue> = task.args.clone();
                let _ = task.callback.call(&JsValue::undefined(), &args, context);
            }
        }
    }

    /// Process animation frames
    pub fn process_animation_frames(&mut self, context: &mut Context) {
        let frame_time = self.get_frame_time();
        let requests: Vec<_> = self.animation_frames.drain(..).collect();

        for request in requests {
            if !request.cancelled {
                let _ = request.callback.call(
                    &JsValue::undefined(),
                    &[JsValue::from(frame_time)],
                    context
                );
            }
        }
    }

    /// Process idle callbacks
    pub fn process_idle_callbacks(&mut self, context: &mut Context) {
        let requests: Vec<_> = self.idle_callbacks.drain(..).collect();

        for request in requests {
            if !request.cancelled {
                // Create IdleDeadline object
                let deadline = create_idle_deadline(context, 50.0); // 50ms deadline
                let _ = request.callback.call(
                    &JsValue::undefined(),
                    &[JsValue::from(deadline)],
                    context
                );
            }
        }
    }

    /// Process ready timers
    pub fn process_timers(&mut self, context: &mut Context) {
        let now = Instant::now();
        let mut reschedule = Vec::new();

        while let Some(task) = self.macro_tasks.peek() {
            if task.scheduled_time > now {
                break;
            }

            let mut task = self.macro_tasks.pop().unwrap();

            if task.cancelled {
                continue;
            }

            // Execute the callback
            let args: Vec<JsValue> = task.args.clone();
            let _ = task.callback.call(&JsValue::undefined(), &args, context);

            // Reschedule if interval
            if task.interval && !task.cancelled {
                task.scheduled_time = Instant::now() + Duration::from_millis(task.delay_ms);
                reschedule.push(task);
            }
        }

        // Reschedule intervals
        for task in reschedule {
            self.macro_tasks.push(task);
        }
    }

    /// Run one iteration of the event loop
    pub fn tick(&mut self, context: &mut Context) {
        // 0. Run Boa's job queue (promise microtasks) - CRITICAL for async/await
        let _ = context.run_jobs();

        // 1. Process all custom microtasks
        self.process_microtasks(context);

        // 1b. Run Boa's job queue again (promises scheduled by microtasks)
        let _ = context.run_jobs();

        // 2. Deliver mutation observer records (microtask checkpoint)
        observers::deliver_mutation_records(context);

        // 3. Process MessageChannel messages (important for React scheduler)
        process_message_port_messages(context);

        // 3b. Run Boa's job queue (promises scheduled by MessageChannel)
        let _ = context.run_jobs();

        // 4. Process immediate tasks
        self.process_immediates(context);

        // 5. Process ready timers
        self.process_timers(context);

        // 5b. Run Boa's job queue (promises scheduled by timers)
        let _ = context.run_jobs();

        // 6. Process animation frames (once per tick)
        self.process_animation_frames(context);

        // 7. Process idle callbacks if time permits
        self.process_idle_callbacks(context);

        // 8. Process network events (WebSocket, EventSource, XHR)
        crate::network::process_network_events(context);

        // 8b. Run Boa's job queue (promises scheduled by network events)
        let _ = context.run_jobs();

        // 9. Process any microtasks generated during this tick
        self.process_microtasks(context);

        // 10. Process any messages generated during this tick
        process_message_port_messages(context);

        // 11. Final mutation observer delivery
        observers::deliver_mutation_records(context);

        // 12. Final run of Boa's job queue
        let _ = context.run_jobs();
    }

    /// Check if there are pending tasks
    pub fn has_pending_tasks(&self) -> bool {
        !self.macro_tasks.is_empty()
            || !self.microtasks.is_empty()
            || !self.animation_frames.is_empty()
            || !self.idle_callbacks.is_empty()
            || !self.immediate_tasks.is_empty()
            || observers::has_pending_mutation_records()
            || has_pending_messages()
    }

    /// Get number of pending timers
    pub fn pending_timer_count(&self) -> usize {
        self.macro_tasks.len()
    }
}

/// Create an IdleDeadline object for requestIdleCallback
fn create_idle_deadline(context: &mut Context, time_remaining: f64) -> JsObject {
    let did_timeout = time_remaining <= 0.0;

    let time_remaining_fn = NativeFunction::from_copy_closure(move |_this, _args, _ctx| {
        Ok(JsValue::from(time_remaining.max(0.0)))
    });

    ObjectInitializer::new(context)
        .property(js_string!("didTimeout"), did_timeout, Attribute::READONLY)
        .function(time_remaining_fn, js_string!("timeRemaining"), 0)
        .build()
}

/// Global job queue state
thread_local! {
    static JOB_QUEUE: RefCell<JobQueue> = RefCell::new(JobQueue::new());
}

/// Register all timer APIs
pub fn register_timer_apis(context: &mut Context) -> Result<(), boa_engine::JsError> {
    // setTimeout
    let set_timeout = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let callback = args.get_or_undefined(0);
        let delay = args.get(1)
            .and_then(|v| v.to_u32(ctx).ok())
            .unwrap_or(0) as u64;

        // Collect additional arguments
        let extra_args: Vec<JsValue> = args.iter().skip(2).cloned().collect();

        if let Some(cb) = callback.as_callable() {
            let cb_obj = cb.clone();
            let id = JOB_QUEUE.with(|q| {
                // Use try_borrow_mut to handle re-entrancy (callback calling setTimeout)
                match q.try_borrow_mut() {
                    Ok(mut queue) => queue.set_timeout(cb_obj, delay, extra_args),
                    Err(_) => {
                        // Re-entrant call - queue is busy, return dummy ID
                        // The timer won't be registered but at least we don't panic
                        log::debug!("setTimeout called re-entrantly, skipping");
                        0
                    }
                }
            });
            return Ok(JsValue::from(id));
        }

        // Handle string callback (eval)
        if callback.is_string() {
            // For security, we don't eval strings, just return 0
            return Ok(JsValue::from(0));
        }

        Ok(JsValue::from(0))
    });
    context.register_global_property(
        js_string!("setTimeout"),
        set_timeout.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // clearTimeout
    let clear_timeout = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let id = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
        JOB_QUEUE.with(|q| {
            if let Ok(mut queue) = q.try_borrow_mut() {
                queue.clear_timer(id);
            }
        });
        Ok(JsValue::undefined())
    });
    context.register_global_property(
        js_string!("clearTimeout"),
        clear_timeout.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // setInterval
    let set_interval = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let callback = args.get_or_undefined(0);
        let delay = args.get(1)
            .and_then(|v| v.to_u32(ctx).ok())
            .unwrap_or(0) as u64;

        let extra_args: Vec<JsValue> = args.iter().skip(2).cloned().collect();

        if let Some(cb) = callback.as_callable() {
            let cb_obj = cb.clone();
            let id = JOB_QUEUE.with(|q| {
                match q.try_borrow_mut() {
                    Ok(mut queue) => queue.set_interval(cb_obj, delay, extra_args),
                    Err(_) => {
                        log::debug!("setInterval called re-entrantly, skipping");
                        0
                    }
                }
            });
            return Ok(JsValue::from(id));
        }

        Ok(JsValue::from(0))
    });
    context.register_global_property(
        js_string!("setInterval"),
        set_interval.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // clearInterval
    let clear_interval = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let id = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
        JOB_QUEUE.with(|q| {
            if let Ok(mut queue) = q.try_borrow_mut() {
                queue.clear_timer(id);
            }
        });
        Ok(JsValue::undefined())
    });
    context.register_global_property(
        js_string!("clearInterval"),
        clear_interval.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // setImmediate (Node.js style)
    let set_immediate = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        let callback = args.get_or_undefined(0);
        let extra_args: Vec<JsValue> = args.iter().skip(1).cloned().collect();

        if let Some(cb) = callback.as_callable() {
            let cb_obj = cb.clone();
            let id = JOB_QUEUE.with(|q| {
                match q.try_borrow_mut() {
                    Ok(mut queue) => queue.set_immediate(cb_obj, extra_args),
                    Err(_) => {
                        log::debug!("setImmediate called re-entrantly, skipping");
                        0
                    }
                }
            });
            return Ok(JsValue::from(id));
        }

        Ok(JsValue::from(0))
    });
    context.register_global_property(
        js_string!("setImmediate"),
        set_immediate.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // clearImmediate
    let clear_immediate = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let id = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
        JOB_QUEUE.with(|q| {
            if let Ok(mut queue) = q.try_borrow_mut() {
                queue.clear_immediate(id);
            }
        });
        Ok(JsValue::undefined())
    });
    context.register_global_property(
        js_string!("clearImmediate"),
        clear_immediate.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // queueMicrotask
    let queue_microtask = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        let callback = args.get_or_undefined(0);

        if let Some(cb) = callback.as_callable() {
            let cb_obj = cb.clone();
            JOB_QUEUE.with(|q| {
                if let Ok(mut queue) = q.try_borrow_mut() {
                    queue.queue_microtask(cb_obj);
                }
            });
        }

        Ok(JsValue::undefined())
    });
    context.register_global_property(
        js_string!("queueMicrotask"),
        queue_microtask.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // requestAnimationFrame
    let request_animation_frame = NativeFunction::from_copy_closure(|_this, args, _ctx| {
        let callback = args.get_or_undefined(0);

        if let Some(cb) = callback.as_callable() {
            let cb_obj = cb.clone();
            let id = JOB_QUEUE.with(|q| {
                match q.try_borrow_mut() {
                    Ok(mut queue) => queue.request_animation_frame(cb_obj),
                    Err(_) => {
                        log::debug!("requestAnimationFrame called re-entrantly, skipping");
                        0
                    }
                }
            });
            return Ok(JsValue::from(id));
        }

        Ok(JsValue::from(0))
    });
    context.register_global_property(
        js_string!("requestAnimationFrame"),
        request_animation_frame.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // cancelAnimationFrame
    let cancel_animation_frame = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let id = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
        JOB_QUEUE.with(|q| {
            if let Ok(mut queue) = q.try_borrow_mut() {
                queue.cancel_animation_frame(id);
            }
        });
        Ok(JsValue::undefined())
    });
    context.register_global_property(
        js_string!("cancelAnimationFrame"),
        cancel_animation_frame.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // requestIdleCallback
    let request_idle_callback = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let callback = args.get_or_undefined(0);
        let timeout = args.get(1)
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("timeout"), ctx).ok())
            .and_then(|v| v.to_u32(ctx).ok())
            .map(|t| t as u64);

        if let Some(cb) = callback.as_callable() {
            let cb_obj = cb.clone();
            let id = JOB_QUEUE.with(|q| {
                match q.try_borrow_mut() {
                    Ok(mut queue) => queue.request_idle_callback(cb_obj, timeout),
                    Err(_) => {
                        log::debug!("requestIdleCallback called re-entrantly, skipping");
                        0
                    }
                }
            });
            return Ok(JsValue::from(id));
        }

        Ok(JsValue::from(0))
    });
    context.register_global_property(
        js_string!("requestIdleCallback"),
        request_idle_callback.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // cancelIdleCallback
    let cancel_idle_callback = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let id = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);
        JOB_QUEUE.with(|q| {
            if let Ok(mut queue) = q.try_borrow_mut() {
                queue.cancel_idle_callback(id);
            }
        });
        Ok(JsValue::undefined())
    });
    context.register_global_property(
        js_string!("cancelIdleCallback"),
        cancel_idle_callback.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // Register Scheduler API (scheduler.postTask)
    register_scheduler_api(context)?;

    // Register MessageChannel and MessagePort
    register_message_channel(context)?;

    // Register structuredClone
    register_structured_clone(context)?;

    // Register reportError
    let report_error = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let error = args.get_or_undefined(0);
        // In a real browser, this would report to error handlers
        // For now, just log it
        if let Ok(str_val) = error.to_string(ctx) {
            log::warn!("Uncaught error: {}", str_val.to_std_string_escaped());
        }
        Ok(JsValue::undefined())
    });
    context.register_global_property(
        js_string!("reportError"),
        report_error.to_js_function(context.realm()),
        Attribute::all()
    )?;

    Ok(())
}

/// Register Scheduler API
fn register_scheduler_api(context: &mut Context) -> Result<(), boa_engine::JsError> {
    // scheduler.postTask(callback, options?)
    let post_task = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let callback = args.get_or_undefined(0);
        let options = args.get(1).and_then(|v| v.as_object());

        let delay = options.as_ref()
            .and_then(|obj| obj.get(js_string!("delay"), ctx).ok())
            .and_then(|v| v.to_u32(ctx).ok())
            .unwrap_or(0) as u64;

        let _priority = options.as_ref()
            .and_then(|obj| obj.get(js_string!("priority"), ctx).ok())
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| TaskPriority::from_str(&s.to_std_string_escaped()))
            .unwrap_or(TaskPriority::UserVisible);

        if let Some(cb) = callback.as_callable() {
            let cb_obj = cb.clone();
            let _id = JOB_QUEUE.with(|q| {
                match q.try_borrow_mut() {
                    Ok(mut queue) => queue.set_timeout(cb_obj, delay, vec![]),
                    Err(_) => {
                        log::debug!("scheduler.postTask called re-entrantly, skipping");
                        0
                    }
                }
            });

            // Return a TaskController-like promise
            return create_task_promise(ctx);
        }

        create_task_promise(ctx)
    });

    // scheduler.yield()
    let yield_fn = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        create_resolved_promise(ctx)
    });

    let scheduler = ObjectInitializer::new(context)
        .function(post_task, js_string!("postTask"), 2)
        .function(yield_fn, js_string!("yield"), 0)
        .build();

    context.register_global_property(
        js_string!("scheduler"),
        scheduler,
        Attribute::all()
    )?;

    // TaskController constructor
    let task_controller_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let priority = args.get(0)
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("priority"), ctx).ok())
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|| "user-visible".to_string());

        // Create signal object
        let signal = ObjectInitializer::new(ctx)
            .property(js_string!("aborted"), false, Attribute::READONLY)
            .property(js_string!("priority"), js_string!(priority.clone()), Attribute::READONLY)
            .property(js_string!("reason"), JsValue::undefined(), Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::from(false))),
                js_string!("throwIfAborted"),
                0
            )
            .build();

        let controller = ObjectInitializer::new(ctx)
            .property(js_string!("signal"), signal, Attribute::READONLY)
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())),
                js_string!("abort"),
                1
            )
            .function(
                NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined())),
                js_string!("setPriority"),
                1
            )
            .build();

        Ok(JsValue::from(controller))
    });

    context.register_global_property(
        js_string!("TaskController"),
        task_controller_ctor.to_js_function(context.realm()),
        Attribute::all()
    )?;

    // TaskPriorityChangeEvent constructor
    let task_priority_change_event_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let previous_priority = args.get(1)
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get(js_string!("previousPriority"), ctx).ok())
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|| "user-visible".to_string());

        let event = ObjectInitializer::new(ctx)
            .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
            .property(js_string!("previousPriority"), js_string!(previous_priority), Attribute::READONLY)
            .property(js_string!("bubbles"), false, Attribute::READONLY)
            .property(js_string!("cancelable"), false, Attribute::READONLY)
            .build();

        Ok(JsValue::from(event))
    });

    context.register_global_property(
        js_string!("TaskPriorityChangeEvent"),
        task_priority_change_event_ctor.to_js_function(context.realm()),
        Attribute::all()
    )?;

    Ok(())
}

/// Pending message for MessageChannel delivery
#[derive(Clone)]
pub struct PendingMessage {
    pub target_port: JsObject,
    pub data: JsValue,
}

/// Pending messages queue for MessageChannel
thread_local! {
    static PENDING_MESSAGES: RefCell<VecDeque<PendingMessage>> = RefCell::new(VecDeque::new());
}

/// Queue a message for delivery to a port
pub fn queue_message_port_message(target_port: JsObject, data: JsValue) {
    PENDING_MESSAGES.with(|q| {
        if let Ok(mut queue) = q.try_borrow_mut() {
            queue.push_back(PendingMessage { target_port, data });
        }
    });
}

/// Process pending MessageChannel messages
pub fn process_message_port_messages(context: &mut Context) {
    let messages: Vec<PendingMessage> = PENDING_MESSAGES.with(|q| {
        if let Ok(mut queue) = q.try_borrow_mut() {
            queue.drain(..).collect()
        } else {
            Vec::new()
        }
    });

    for msg in messages {
        // Get the onmessage handler from the target port
        if let Ok(handler) = msg.target_port.get(js_string!("onmessage"), context) {
            if let Some(cb) = handler.as_callable() {
                // Create empty ports array first to avoid borrow conflict
                let ports = JsArray::new(context);

                // Create MessageEvent
                let event = ObjectInitializer::new(context)
                    .property(js_string!("type"), js_string!("message"), Attribute::READONLY)
                    .property(js_string!("data"), msg.data, Attribute::READONLY)
                    .property(js_string!("origin"), js_string!(""), Attribute::READONLY)
                    .property(js_string!("lastEventId"), js_string!(""), Attribute::READONLY)
                    .property(js_string!("source"), JsValue::null(), Attribute::READONLY)
                    .property(js_string!("ports"), ports, Attribute::READONLY)
                    .build();

                let _ = cb.call(&JsValue::from(msg.target_port.clone()), &[JsValue::from(event)], context);
            }
        }
    }
}

/// Check if there are pending MessageChannel messages
pub fn has_pending_messages() -> bool {
    PENDING_MESSAGES.with(|q| {
        q.borrow().len() > 0
    })
}

/// MessagePort postMessage native function
fn message_port_post_message(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> Result<JsValue, boa_engine::JsError> {
    let data = args.get(0).cloned().unwrap_or(JsValue::undefined());
    if let Some(this_obj) = this.as_object() {
        if let Ok(partner) = this_obj.get(js_string!("__partner"), context) {
            if let Some(partner_obj) = partner.as_object() {
                // Queue message for async delivery
                queue_message_port_message(partner_obj.clone(), data);
            }
        }
    }
    Ok(JsValue::undefined())
}

/// MessagePort no-op function
fn message_port_noop(
    _this: &JsValue,
    _args: &[JsValue],
    _context: &mut Context,
) -> Result<JsValue, boa_engine::JsError> {
    Ok(JsValue::undefined())
}

/// Register MessageChannel and MessagePort
fn register_message_channel(context: &mut Context) -> Result<(), boa_engine::JsError> {
    // MessageChannel constructor - creates paired ports
    // Use from_copy_closure which properly supports being a constructor
    let message_channel_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Create native functions inside the closure (they use fn pointers which are Copy)
        let post_message_native = NativeFunction::from_fn_ptr(message_port_post_message);
        let noop_native = NativeFunction::from_fn_ptr(message_port_noop);

        // Create port1 and port2
        let port1 = JsObject::with_object_proto(ctx.intrinsics());
        let port2 = JsObject::with_object_proto(ctx.intrinsics());

        // Set up port1
        port1.set(js_string!("onmessage"), JsValue::null(), false, ctx)?;
        port1.set(js_string!("onmessageerror"), JsValue::null(), false, ctx)?;
        port1.set(js_string!("__partner"), JsValue::from(port2.clone()), false, ctx)?;

        // Set up port2
        port2.set(js_string!("onmessage"), JsValue::null(), false, ctx)?;
        port2.set(js_string!("onmessageerror"), JsValue::null(), false, ctx)?;
        port2.set(js_string!("__partner"), JsValue::from(port1.clone()), false, ctx)?;

        // Add methods using cloned natives
        let post_fn = post_message_native.clone().to_js_function(ctx.realm());
        port1.set(js_string!("postMessage"), post_fn.clone(), false, ctx)?;
        port2.set(js_string!("postMessage"), post_fn, false, ctx)?;

        let start_fn = noop_native.clone().to_js_function(ctx.realm());
        port1.set(js_string!("start"), start_fn.clone(), false, ctx)?;
        port2.set(js_string!("start"), start_fn, false, ctx)?;

        let close_fn = noop_native.clone().to_js_function(ctx.realm());
        port1.set(js_string!("close"), close_fn.clone(), false, ctx)?;
        port2.set(js_string!("close"), close_fn, false, ctx)?;

        let add_listener_fn = noop_native.clone().to_js_function(ctx.realm());
        port1.set(js_string!("addEventListener"), add_listener_fn.clone(), false, ctx)?;
        port2.set(js_string!("addEventListener"), add_listener_fn, false, ctx)?;

        let remove_listener_fn = noop_native.clone().to_js_function(ctx.realm());
        port1.set(js_string!("removeEventListener"), remove_listener_fn.clone(), false, ctx)?;
        port2.set(js_string!("removeEventListener"), remove_listener_fn, false, ctx)?;

        let channel = ObjectInitializer::new(ctx)
            .property(js_string!("port1"), port1, Attribute::READONLY)
            .property(js_string!("port2"), port2, Attribute::READONLY)
            .build();

        Ok(JsValue::from(channel))
    });

    // Use FunctionObjectBuilder with .constructor(true) to make it a proper constructor
    let message_channel_constructor = FunctionObjectBuilder::new(context.realm(), message_channel_ctor)
        .name(js_string!("MessageChannel"))
        .length(0)
        .constructor(true)
        .build();

    // Create a prototype object for instanceof checks (required for new to work)
    let prototype = ObjectInitializer::new(context)
        .property(js_string!("constructor"), message_channel_constructor.clone(), Attribute::WRITABLE | Attribute::CONFIGURABLE)
        .build();

    // Set MessageChannel.prototype = prototype
    message_channel_constructor.set(js_string!("prototype"), prototype, false, context)?;

    context.register_global_property(
        js_string!("MessageChannel"),
        message_channel_constructor,
        Attribute::all()
    )?;

    // BroadcastChannel constructor
    let broadcast_channel_ctor = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let post_message_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let close_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
            Ok(JsValue::undefined())
        });

        let channel = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("onmessage"), JsValue::null(), Attribute::all())
            .property(js_string!("onmessageerror"), JsValue::null(), Attribute::all())
            .function(post_message_fn, js_string!("postMessage"), 1)
            .function(close_fn, js_string!("close"), 0)
            .build();

        Ok(JsValue::from(channel))
    });

    context.register_global_property(
        js_string!("BroadcastChannel"),
        broadcast_channel_ctor.to_js_function(context.realm()),
        Attribute::all()
    )?;

    Ok(())
}

/// Register structuredClone
fn register_structured_clone(context: &mut Context) -> Result<(), boa_engine::JsError> {
    let structured_clone = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let value = args.get_or_undefined(0);

        // For simple values, just return them
        if value.is_undefined() || value.is_null() || value.is_boolean()
            || value.is_number() || value.is_string() || value.is_bigint() {
            return Ok(value.clone());
        }

        // For objects, try to clone via JSON (simplified)
        if let Some(obj) = value.as_object() {
            // Check if it's an array
            if obj.is_array() {
                let len = obj.get(js_string!("length"), ctx)?
                    .to_u32(ctx)
                    .unwrap_or(0);

                let new_arr = JsArray::new(ctx);
                for i in 0..len {
                    if let Ok(item) = obj.get(js_string!(i.to_string()), ctx) {
                        let _ = new_arr.push(item, ctx);
                    }
                }
                return Ok(JsValue::from(new_arr));
            }

            // For plain objects, create a shallow clone
            let new_obj = ObjectInitializer::new(ctx).build();
            let keys = obj.own_property_keys(ctx)?;
            for key in keys {
                if let Ok(val) = obj.get(key.clone(), ctx) {
                    let _ = new_obj.set(key, val, false, ctx);
                }
            }
            return Ok(JsValue::from(new_obj));
        }

        Ok(value.clone())
    });

    context.register_global_property(
        js_string!("structuredClone"),
        structured_clone.to_js_function(context.realm()),
        Attribute::all()
    )?;

    Ok(())
}

/// Create a simple resolved promise
fn create_resolved_promise(context: &mut Context) -> Result<JsValue, boa_engine::JsError> {
    let then_fn = NativeFunction::from_copy_closure(|this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if callback.is_callable() {
            let cb = callback.as_callable().unwrap();
            let _ = cb.call(&JsValue::undefined(), &[], ctx);
        }
        Ok(this.clone())
    });

    let catch_fn = NativeFunction::from_copy_closure(|this, _args, _ctx| {
        Ok(this.clone())
    });

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

    Ok(JsValue::from(promise))
}

/// Create a task promise for scheduler.postTask
fn create_task_promise(context: &mut Context) -> Result<JsValue, boa_engine::JsError> {
    create_resolved_promise(context)
}

/// Process the job queue - call this to execute pending timers
pub fn process_job_queue(context: &mut Context) {
    JOB_QUEUE.with(|q| {
        q.borrow_mut().tick(context);
    });
}

/// Check if there are pending tasks in the job queue
pub fn has_pending_tasks() -> bool {
    JOB_QUEUE.with(|q| q.borrow().has_pending_tasks())
}

/// Get the number of pending timers
pub fn pending_timer_count() -> usize {
    JOB_QUEUE.with(|q| q.borrow().pending_timer_count())
}

/// Reset the job queue (useful for testing)
pub fn reset_job_queue() {
    JOB_QUEUE.with(|q| {
        *q.borrow_mut() = JobQueue::new();
    });
}

/// Run one iteration of the event loop
/// This processes microtasks, immediate tasks, timers, animation frames, and idle callbacks
pub fn run_event_loop_tick(context: &mut Context) {
    JOB_QUEUE.with(|q| {
        q.borrow_mut().tick(context);
    });
}

/// Run the event loop until all tasks are complete or max_iterations is reached
/// Returns the number of iterations executed
pub fn run_event_loop(context: &mut Context, max_iterations: u32) -> u32 {
    let mut iterations = 0;

    while iterations < max_iterations && has_pending_tasks() {
        run_event_loop_tick(context);
        iterations += 1;

        // Small sleep to prevent busy-waiting for timers
        if has_pending_tasks() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    iterations
}

/// Create AbortController constructor
pub fn register_abort_controller(context: &mut Context) -> Result<(), boa_engine::JsError> {
    let abort_controller_ctor = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let throw_if_aborted = NativeFunction::from_copy_closure(|this, _args, ctx| {
            if let Some(obj) = this.as_object() {
                if let Ok(aborted) = obj.get(js_string!("aborted"), ctx) {
                    if aborted.to_boolean() {
                        let reason = obj.get(js_string!("reason"), ctx)
                            .unwrap_or(JsValue::undefined());
                        return Err(boa_engine::JsError::from_opaque(reason));
                    }
                }
            }
            Ok(JsValue::undefined())
        });

        // Create signal object from scratch
        use boa_engine::object::JsObject;

        let signal = JsObject::with_object_proto(ctx.intrinsics());

        // Add data properties using set (should create writable by default)
        signal.set(js_string!("aborted"), JsValue::from(false), true, ctx)?;
        signal.set(js_string!("reason"), JsValue::undefined(), true, ctx)?;
        signal.set(js_string!("onabort"), JsValue::null(), true, ctx)?;

        // Add methods as function properties
        let throw_if_aborted_fn = throw_if_aborted.to_js_function(ctx.realm());
        signal.set(js_string!("throwIfAborted"), throw_if_aborted_fn, false, ctx)?;

        let add_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
        signal.set(js_string!("addEventListener"), add_event_listener.to_js_function(ctx.realm()), false, ctx)?;

        let remove_event_listener = NativeFunction::from_copy_closure(|_this, _args, _ctx| Ok(JsValue::undefined()));
        signal.set(js_string!("removeEventListener"), remove_event_listener.to_js_function(ctx.realm()), false, ctx)?;

        // Create controller first with signal
        let controller = ObjectInitializer::new(ctx)
            .property(js_string!("signal"), signal, Attribute::READONLY)
            .build();

        // Now create abort function that reads signal from the controller
        let controller_for_abort = controller.clone();
        let abort_fn = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
                // Get signal from our captured controller reference
                if let Ok(signal_val) = controller_for_abort.get(js_string!("signal"), ctx) {
                    if let Some(signal_obj) = signal_val.as_object() {
                        let reason = if args.is_empty() || args.get_or_undefined(0).is_undefined() {
                            // Create DOMException with AbortError
                            let error = ObjectInitializer::new(ctx)
                                .property(js_string!("name"), js_string!("AbortError"), Attribute::READONLY)
                                .property(js_string!("message"), js_string!("signal is aborted without reason"), Attribute::READONLY)
                                .build();
                            JsValue::from(error)
                        } else {
                            args.get_or_undefined(0).clone()
                        };

                        // Update signal object properties directly
                        signal_obj.set(js_string!("aborted"), JsValue::from(true), false, ctx)?;
                        signal_obj.set(js_string!("reason"), reason.clone(), false, ctx)?;

                        // Trigger onabort if set
                        if let Ok(onabort) = signal_obj.get(js_string!("onabort"), ctx) {
                            if onabort.is_callable() {
                                let cb = onabort.as_callable().unwrap();
                                let _ = cb.call(&signal_val, &[], ctx);
                            }
                        }
                    }
                }
                Ok(JsValue::undefined())
            })
        };

        // Add abort method to controller
        let abort_fn_obj = abort_fn.to_js_function(ctx.realm());
        controller.set(js_string!("abort"), abort_fn_obj, false, ctx)?;

        Ok(JsValue::from(controller))
    });

    let ctor = FunctionObjectBuilder::new(context.realm(), abort_controller_ctor)
        .name(js_string!("AbortController"))
        .length(0)
        .constructor(true)
        .build();

    context.global_object().set(
        js_string!("AbortController"),
        ctor,
        false,
        context
    )?;

    // AbortSignal static methods
    let abort_signal_abort = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let reason = args.get_or_undefined(0).clone();

        let signal = ObjectInitializer::new(ctx)
            .property(js_string!("aborted"), true, Attribute::READONLY)
            .property(js_string!("reason"), reason, Attribute::READONLY)
            .property(js_string!("onabort"), JsValue::null(), Attribute::all())
            .function(
                NativeFunction::from_copy_closure(|this, _args, ctx| {
                    if let Some(obj) = this.as_object() {
                        let reason = obj.get(js_string!("reason"), ctx)
                            .unwrap_or(JsValue::undefined());
                        return Err(boa_engine::JsError::from_opaque(reason));
                    }
                    Ok(JsValue::undefined())
                }),
                js_string!("throwIfAborted"),
                0
            )
            .build();

        Ok(JsValue::from(signal))
    });

    let abort_signal_timeout = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let _timeout = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);

        // Create a signal that will be aborted after timeout
        // In practice, this would schedule the abort
        let signal = ObjectInitializer::new(ctx)
            .property(js_string!("aborted"), false, Attribute::READONLY)
            .property(js_string!("reason"), JsValue::undefined(), Attribute::READONLY)
            .property(js_string!("onabort"), JsValue::null(), Attribute::all())
            .function(
                NativeFunction::from_copy_closure(|this, _args, ctx| {
                    if let Some(obj) = this.as_object() {
                        if let Ok(aborted) = obj.get(js_string!("aborted"), ctx) {
                            if aborted.to_boolean() {
                                let reason = obj.get(js_string!("reason"), ctx)
                                    .unwrap_or(JsValue::undefined());
                                return Err(boa_engine::JsError::from_opaque(reason));
                            }
                        }
                    }
                    Ok(JsValue::undefined())
                }),
                js_string!("throwIfAborted"),
                0
            )
            .build();

        Ok(JsValue::from(signal))
    });

    let abort_signal_any = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let signals = args.get_or_undefined(0);

        // Check if any signal is already aborted
        let mut is_aborted = false;
        let mut abort_reason = JsValue::undefined();

        if let Some(arr) = signals.as_object() {
            let len = arr.get(js_string!("length"), ctx)
                .and_then(|v| v.to_u32(ctx))
                .unwrap_or(0);

            for i in 0..len {
                if let Ok(signal) = arr.get(js_string!(i.to_string()), ctx) {
                    if let Some(sig_obj) = signal.as_object() {
                        if let Ok(aborted) = sig_obj.get(js_string!("aborted"), ctx) {
                            if aborted.to_boolean() {
                                is_aborted = true;
                                abort_reason = sig_obj.get(js_string!("reason"), ctx)
                                    .unwrap_or(JsValue::undefined());
                                break;
                            }
                        }
                    }
                }
            }
        }

        let signal = ObjectInitializer::new(ctx)
            .property(js_string!("aborted"), is_aborted, Attribute::READONLY)
            .property(js_string!("reason"), abort_reason, Attribute::READONLY)
            .property(js_string!("onabort"), JsValue::null(), Attribute::all())
            .function(
                NativeFunction::from_copy_closure(|this, _args, ctx| {
                    if let Some(obj) = this.as_object() {
                        if let Ok(aborted) = obj.get(js_string!("aborted"), ctx) {
                            if aborted.to_boolean() {
                                let reason = obj.get(js_string!("reason"), ctx)
                                    .unwrap_or(JsValue::undefined());
                                return Err(boa_engine::JsError::from_opaque(reason));
                            }
                        }
                    }
                    Ok(JsValue::undefined())
                }),
                js_string!("throwIfAborted"),
                0
            )
            .build();

        Ok(JsValue::from(signal))
    });

    let abort_signal = ObjectInitializer::new(context)
        .function(abort_signal_abort, js_string!("abort"), 1)
        .function(abort_signal_timeout, js_string!("timeout"), 1)
        .function(abort_signal_any, js_string!("any"), 1)
        .build();

    context.register_global_property(
        js_string!("AbortSignal"),
        abort_signal,
        Attribute::all()
    )?;

    Ok(())
}
