//! Minimal jQuery implementation for browser compatibility
//!
//! Provides core jQuery functionality that many websites depend on:
//! - $() selector and wrapper
//! - DOM manipulation methods
//! - Event handling
//! - AJAX requests
//! - Utility functions

use boa_engine::{
    js_string, object::ObjectInitializer, property::Attribute, Context, JsArgs, JsResult,
    JsValue, NativeFunction, JsObject,
};

/// Register jQuery and $ globals
pub fn register_jquery(context: &mut Context) -> JsResult<()> {
    // Create the main jQuery function
    let jquery_code = r#"
(function(window) {
    'use strict';

    // jQuery object constructor
    function jQuery(selector, context) {
        return new jQuery.fn.init(selector, context);
    }

    // Prototype methods
    jQuery.fn = jQuery.prototype = {
        constructor: jQuery,
        length: 0,

        // Make it array-like
        push: Array.prototype.push,
        splice: Array.prototype.splice,
        indexOf: Array.prototype.indexOf,

        // Initialize with selector
        init: function(selector, context) {
            if (!selector) {
                return this;
            }

            // Handle $(DOMElement)
            if (selector.nodeType) {
                this[0] = selector;
                this.length = 1;
                return this;
            }

            // Handle $(function) - DOM ready
            if (typeof selector === 'function') {
                if (document.readyState === 'loading') {
                    document.addEventListener('DOMContentLoaded', selector);
                } else {
                    selector();
                }
                return this;
            }

            // Handle HTML strings
            if (typeof selector === 'string') {
                if (selector[0] === '<' && selector[selector.length - 1] === '>' && selector.length >= 3) {
                    // Create elements from HTML string
                    var temp = document.createElement('div');
                    temp.innerHTML = selector;
                    var nodes = temp.childNodes;
                    for (var i = 0; i < nodes.length; i++) {
                        this[i] = nodes[i];
                    }
                    this.length = nodes.length;
                    return this;
                }

                // CSS selector
                var ctx = context ? (context.nodeType ? context : document.querySelector(context)) : document;
                if (!ctx) ctx = document;

                try {
                    var elements = ctx.querySelectorAll(selector);
                    for (var i = 0; i < elements.length; i++) {
                        this[i] = elements[i];
                    }
                    this.length = elements.length;
                } catch (e) {
                    this.length = 0;
                }
                return this;
            }

            // Handle array-like objects
            if (selector.length !== undefined) {
                for (var i = 0; i < selector.length; i++) {
                    this[i] = selector[i];
                }
                this.length = selector.length;
                return this;
            }

            return this;
        },

        // Iterate over elements
        each: function(callback) {
            for (var i = 0; i < this.length; i++) {
                if (callback.call(this[i], i, this[i]) === false) break;
            }
            return this;
        },

        // Get element at index
        get: function(index) {
            if (index === undefined) {
                return Array.prototype.slice.call(this);
            }
            return index < 0 ? this[this.length + index] : this[index];
        },

        // Get element wrapped in jQuery
        eq: function(index) {
            return jQuery(this.get(index));
        },

        // Get first element
        first: function() {
            return this.eq(0);
        },

        // Get last element
        last: function() {
            return this.eq(-1);
        },

        // Convert to array
        toArray: function() {
            return Array.prototype.slice.call(this);
        },

        // DOM Ready
        ready: function(fn) {
            if (document.readyState === 'loading') {
                document.addEventListener('DOMContentLoaded', fn);
            } else {
                fn();
            }
            return this;
        },

        // Get/set HTML content
        html: function(value) {
            if (value === undefined) {
                return this[0] ? this[0].innerHTML : '';
            }
            return this.each(function() {
                this.innerHTML = value;
            });
        },

        // Get/set text content
        text: function(value) {
            if (value === undefined) {
                return this[0] ? this[0].textContent : '';
            }
            return this.each(function() {
                this.textContent = value;
            });
        },

        // Get/set value
        val: function(value) {
            if (value === undefined) {
                return this[0] ? this[0].value : '';
            }
            return this.each(function() {
                this.value = value;
            });
        },

        // Get/set attribute
        attr: function(name, value) {
            if (typeof name === 'object') {
                for (var key in name) {
                    this.attr(key, name[key]);
                }
                return this;
            }
            if (value === undefined) {
                return this[0] ? this[0].getAttribute(name) : undefined;
            }
            return this.each(function() {
                this.setAttribute(name, value);
            });
        },

        // Remove attribute
        removeAttr: function(name) {
            return this.each(function() {
                this.removeAttribute(name);
            });
        },

        // Get/set property
        prop: function(name, value) {
            if (value === undefined) {
                return this[0] ? this[0][name] : undefined;
            }
            return this.each(function() {
                this[name] = value;
            });
        },

        // Get/set data attribute
        data: function(key, value) {
            if (key === undefined) {
                if (!this[0]) return {};
                var data = {};
                var attrs = this[0].attributes;
                for (var i = 0; i < attrs.length; i++) {
                    if (attrs[i].name.indexOf('data-') === 0) {
                        var name = attrs[i].name.slice(5).replace(/-([a-z])/g, function(m, l) { return l.toUpperCase(); });
                        data[name] = attrs[i].value;
                    }
                }
                return data;
            }
            if (value === undefined) {
                return this[0] ? this[0].getAttribute('data-' + key.replace(/([A-Z])/g, '-$1').toLowerCase()) : undefined;
            }
            return this.each(function() {
                this.setAttribute('data-' + key.replace(/([A-Z])/g, '-$1').toLowerCase(), value);
            });
        },

        // Add class
        addClass: function(className) {
            var classes = className.split(/\s+/);
            return this.each(function() {
                for (var i = 0; i < classes.length; i++) {
                    if (classes[i]) this.classList.add(classes[i]);
                }
            });
        },

        // Remove class
        removeClass: function(className) {
            if (!className) {
                return this.each(function() {
                    this.className = '';
                });
            }
            var classes = className.split(/\s+/);
            return this.each(function() {
                for (var i = 0; i < classes.length; i++) {
                    if (classes[i]) this.classList.remove(classes[i]);
                }
            });
        },

        // Toggle class
        toggleClass: function(className, state) {
            var classes = className.split(/\s+/);
            return this.each(function() {
                for (var i = 0; i < classes.length; i++) {
                    if (classes[i]) {
                        if (state === undefined) {
                            this.classList.toggle(classes[i]);
                        } else if (state) {
                            this.classList.add(classes[i]);
                        } else {
                            this.classList.remove(classes[i]);
                        }
                    }
                }
            });
        },

        // Check if has class
        hasClass: function(className) {
            return this[0] ? this[0].classList.contains(className) : false;
        },

        // Get/set CSS
        css: function(name, value) {
            if (typeof name === 'object') {
                for (var key in name) {
                    this.css(key, name[key]);
                }
                return this;
            }
            if (value === undefined) {
                if (!this[0]) return undefined;
                var style = window.getComputedStyle(this[0]);
                return style ? style[name] : undefined;
            }
            return this.each(function() {
                this.style[name] = value;
            });
        },

        // Show element
        show: function() {
            return this.each(function() {
                this.style.display = '';
                if (window.getComputedStyle(this).display === 'none') {
                    this.style.display = 'block';
                }
            });
        },

        // Hide element
        hide: function() {
            return this.each(function() {
                this.style.display = 'none';
            });
        },

        // Toggle visibility
        toggle: function(state) {
            return this.each(function() {
                var isHidden = window.getComputedStyle(this).display === 'none';
                if (state === undefined ? isHidden : state) {
                    jQuery(this).show();
                } else {
                    jQuery(this).hide();
                }
            });
        },

        // Fade effects (simplified - no animation)
        fadeIn: function(duration, callback) {
            this.show();
            if (typeof duration === 'function') callback = duration;
            if (callback) callback.call(this);
            return this;
        },

        fadeOut: function(duration, callback) {
            this.hide();
            if (typeof duration === 'function') callback = duration;
            if (callback) callback.call(this);
            return this;
        },

        fadeTo: function(duration, opacity, callback) {
            this.css('opacity', opacity);
            if (callback) callback.call(this);
            return this;
        },

        // Slide effects (simplified - no animation)
        slideDown: function(duration, callback) {
            this.show();
            if (typeof duration === 'function') callback = duration;
            if (callback) callback.call(this);
            return this;
        },

        slideUp: function(duration, callback) {
            this.hide();
            if (typeof duration === 'function') callback = duration;
            if (callback) callback.call(this);
            return this;
        },

        slideToggle: function(duration, callback) {
            this.toggle();
            if (typeof duration === 'function') callback = duration;
            if (callback) callback.call(this);
            return this;
        },

        // Animate (stub - no real animation)
        animate: function(properties, duration, easing, callback) {
            if (typeof duration === 'function') {
                callback = duration;
                duration = undefined;
            }
            if (typeof easing === 'function') {
                callback = easing;
                easing = undefined;
            }
            this.css(properties);
            if (callback) callback.call(this);
            return this;
        },

        // Stop animation (stub)
        stop: function() {
            return this;
        },

        // Append content
        append: function(content) {
            return this.each(function() {
                if (typeof content === 'string') {
                    this.insertAdjacentHTML('beforeend', content);
                } else if (content.nodeType) {
                    this.appendChild(content);
                } else if (content.length !== undefined) {
                    for (var i = 0; i < content.length; i++) {
                        if (content[i] && content[i].nodeType) {
                            this.appendChild(content[i]);
                        }
                    }
                }
            });
        },

        // Prepend content
        prepend: function(content) {
            return this.each(function() {
                if (typeof content === 'string') {
                    this.insertAdjacentHTML('afterbegin', content);
                } else if (content.nodeType) {
                    this.insertBefore(content, this.firstChild);
                } else if (content.length !== undefined) {
                    for (var i = content.length - 1; i >= 0; i--) {
                        if (content[i] && content[i].nodeType) {
                            this.insertBefore(content[i], this.firstChild);
                        }
                    }
                }
            });
        },

        // Insert after
        after: function(content) {
            return this.each(function() {
                if (typeof content === 'string') {
                    this.insertAdjacentHTML('afterend', content);
                } else if (content.nodeType && this.parentNode) {
                    this.parentNode.insertBefore(content, this.nextSibling);
                }
            });
        },

        // Insert before
        before: function(content) {
            return this.each(function() {
                if (typeof content === 'string') {
                    this.insertAdjacentHTML('beforebegin', content);
                } else if (content.nodeType && this.parentNode) {
                    this.parentNode.insertBefore(content, this);
                }
            });
        },

        // Append to target
        appendTo: function(target) {
            jQuery(target).append(this);
            return this;
        },

        // Prepend to target
        prependTo: function(target) {
            jQuery(target).prepend(this);
            return this;
        },

        // Remove elements
        remove: function() {
            return this.each(function() {
                if (this.parentNode) {
                    this.parentNode.removeChild(this);
                }
            });
        },

        // Empty contents
        empty: function() {
            return this.each(function() {
                this.innerHTML = '';
            });
        },

        // Clone elements
        clone: function(deep) {
            var clones = [];
            this.each(function() {
                clones.push(this.cloneNode(deep !== false));
            });
            return jQuery(clones);
        },

        // Wrap element
        wrap: function(wrapper) {
            return this.each(function() {
                var wrap = typeof wrapper === 'string' ? jQuery(wrapper)[0] : wrapper;
                if (wrap && this.parentNode) {
                    var clone = wrap.cloneNode(true);
                    this.parentNode.insertBefore(clone, this);
                    clone.appendChild(this);
                }
            });
        },

        // Unwrap element
        unwrap: function() {
            return this.parent().each(function() {
                jQuery(this).replaceWith(this.childNodes);
            });
        },

        // Replace with content
        replaceWith: function(content) {
            return this.each(function() {
                if (typeof content === 'string') {
                    this.outerHTML = content;
                } else if (content.nodeType && this.parentNode) {
                    this.parentNode.replaceChild(content, this);
                }
            });
        },

        // Find descendants
        find: function(selector) {
            var result = [];
            this.each(function() {
                var found = this.querySelectorAll(selector);
                for (var i = 0; i < found.length; i++) {
                    if (result.indexOf(found[i]) === -1) {
                        result.push(found[i]);
                    }
                }
            });
            return jQuery(result);
        },

        // Filter elements
        filter: function(selector) {
            var result = [];
            this.each(function() {
                if (typeof selector === 'function') {
                    if (selector.call(this, result.length, this)) {
                        result.push(this);
                    }
                } else if (this.matches && this.matches(selector)) {
                    result.push(this);
                }
            });
            return jQuery(result);
        },

        // Exclude elements
        not: function(selector) {
            var result = [];
            this.each(function() {
                if (typeof selector === 'function') {
                    if (!selector.call(this, result.length, this)) {
                        result.push(this);
                    }
                } else if (!this.matches || !this.matches(selector)) {
                    result.push(this);
                }
            });
            return jQuery(result);
        },

        // Check if matches selector
        is: function(selector) {
            if (!this[0]) return false;
            if (typeof selector === 'function') {
                return !!selector.call(this[0], 0, this[0]);
            }
            return this[0].matches ? this[0].matches(selector) : false;
        },

        // Get parent
        parent: function(selector) {
            var result = [];
            this.each(function() {
                if (this.parentNode && result.indexOf(this.parentNode) === -1) {
                    if (!selector || (this.parentNode.matches && this.parentNode.matches(selector))) {
                        result.push(this.parentNode);
                    }
                }
            });
            return jQuery(result);
        },

        // Get all parents
        parents: function(selector) {
            var result = [];
            this.each(function() {
                var parent = this.parentNode;
                while (parent && parent !== document) {
                    if (!selector || (parent.matches && parent.matches(selector))) {
                        if (result.indexOf(parent) === -1) {
                            result.push(parent);
                        }
                    }
                    parent = parent.parentNode;
                }
            });
            return jQuery(result);
        },

        // Get closest ancestor
        closest: function(selector) {
            var result = [];
            this.each(function() {
                var el = this;
                while (el && el !== document) {
                    if (el.matches && el.matches(selector)) {
                        if (result.indexOf(el) === -1) {
                            result.push(el);
                        }
                        break;
                    }
                    el = el.parentNode;
                }
            });
            return jQuery(result);
        },

        // Get children
        children: function(selector) {
            var result = [];
            this.each(function() {
                var children = this.children;
                for (var i = 0; i < children.length; i++) {
                    if (!selector || (children[i].matches && children[i].matches(selector))) {
                        if (result.indexOf(children[i]) === -1) {
                            result.push(children[i]);
                        }
                    }
                }
            });
            return jQuery(result);
        },

        // Get siblings
        siblings: function(selector) {
            var result = [];
            this.each(function() {
                var parent = this.parentNode;
                if (parent) {
                    var children = parent.children;
                    for (var i = 0; i < children.length; i++) {
                        if (children[i] !== this) {
                            if (!selector || (children[i].matches && children[i].matches(selector))) {
                                if (result.indexOf(children[i]) === -1) {
                                    result.push(children[i]);
                                }
                            }
                        }
                    }
                }
            });
            return jQuery(result);
        },

        // Get next sibling
        next: function(selector) {
            var result = [];
            this.each(function() {
                var next = this.nextElementSibling;
                if (next && (!selector || (next.matches && next.matches(selector)))) {
                    if (result.indexOf(next) === -1) {
                        result.push(next);
                    }
                }
            });
            return jQuery(result);
        },

        // Get previous sibling
        prev: function(selector) {
            var result = [];
            this.each(function() {
                var prev = this.previousElementSibling;
                if (prev && (!selector || (prev.matches && prev.matches(selector)))) {
                    if (result.indexOf(prev) === -1) {
                        result.push(prev);
                    }
                }
            });
            return jQuery(result);
        },

        // Get all next siblings
        nextAll: function(selector) {
            var result = [];
            this.each(function() {
                var next = this.nextElementSibling;
                while (next) {
                    if (!selector || (next.matches && next.matches(selector))) {
                        if (result.indexOf(next) === -1) {
                            result.push(next);
                        }
                    }
                    next = next.nextElementSibling;
                }
            });
            return jQuery(result);
        },

        // Get all previous siblings
        prevAll: function(selector) {
            var result = [];
            this.each(function() {
                var prev = this.previousElementSibling;
                while (prev) {
                    if (!selector || (prev.matches && prev.matches(selector))) {
                        if (result.indexOf(prev) === -1) {
                            result.push(prev);
                        }
                    }
                    prev = prev.previousElementSibling;
                }
            });
            return jQuery(result);
        },

        // Get offset
        offset: function() {
            if (!this[0]) return { top: 0, left: 0 };
            var rect = this[0].getBoundingClientRect();
            return {
                top: rect.top + window.pageYOffset,
                left: rect.left + window.pageXOffset
            };
        },

        // Get position relative to offset parent
        position: function() {
            if (!this[0]) return { top: 0, left: 0 };
            return {
                top: this[0].offsetTop,
                left: this[0].offsetLeft
            };
        },

        // Get/set scroll position
        scrollTop: function(value) {
            if (value === undefined) {
                return this[0] ? this[0].scrollTop : 0;
            }
            return this.each(function() {
                this.scrollTop = value;
            });
        },

        scrollLeft: function(value) {
            if (value === undefined) {
                return this[0] ? this[0].scrollLeft : 0;
            }
            return this.each(function() {
                this.scrollLeft = value;
            });
        },

        // Get dimensions
        width: function(value) {
            if (value === undefined) {
                return this[0] ? this[0].offsetWidth : 0;
            }
            return this.css('width', typeof value === 'number' ? value + 'px' : value);
        },

        height: function(value) {
            if (value === undefined) {
                return this[0] ? this[0].offsetHeight : 0;
            }
            return this.css('height', typeof value === 'number' ? value + 'px' : value);
        },

        innerWidth: function() {
            return this[0] ? this[0].clientWidth : 0;
        },

        innerHeight: function() {
            return this[0] ? this[0].clientHeight : 0;
        },

        outerWidth: function(includeMargin) {
            if (!this[0]) return 0;
            var width = this[0].offsetWidth;
            if (includeMargin) {
                var style = window.getComputedStyle(this[0]);
                width += parseInt(style.marginLeft) + parseInt(style.marginRight);
            }
            return width;
        },

        outerHeight: function(includeMargin) {
            if (!this[0]) return 0;
            var height = this[0].offsetHeight;
            if (includeMargin) {
                var style = window.getComputedStyle(this[0]);
                height += parseInt(style.marginTop) + parseInt(style.marginBottom);
            }
            return height;
        },

        // Event handling
        on: function(events, selector, data, handler) {
            if (typeof selector === 'function') {
                handler = selector;
                selector = undefined;
                data = undefined;
            } else if (typeof data === 'function') {
                handler = data;
                data = undefined;
            }

            var eventList = events.split(/\s+/);
            return this.each(function() {
                var el = this;
                for (var i = 0; i < eventList.length; i++) {
                    var eventName = eventList[i].split('.')[0];
                    var namespace = eventList[i].split('.')[1];

                    var wrappedHandler = function(e) {
                        if (selector) {
                            var target = e.target;
                            while (target && target !== el) {
                                if (target.matches && target.matches(selector)) {
                                    e.delegateTarget = el;
                                    handler.call(target, e);
                                    return;
                                }
                                target = target.parentNode;
                            }
                        } else {
                            handler.call(el, e);
                        }
                    };

                    wrappedHandler._original = handler;
                    wrappedHandler._namespace = namespace;
                    wrappedHandler._selector = selector;

                    el._jqEvents = el._jqEvents || {};
                    el._jqEvents[eventName] = el._jqEvents[eventName] || [];
                    el._jqEvents[eventName].push(wrappedHandler);

                    el.addEventListener(eventName, wrappedHandler);
                }
            });
        },

        off: function(events, selector, handler) {
            if (typeof selector === 'function') {
                handler = selector;
                selector = undefined;
            }

            var eventList = events ? events.split(/\s+/) : [];
            return this.each(function() {
                var el = this;
                if (!el._jqEvents) return;

                if (eventList.length === 0) {
                    // Remove all events
                    for (var eventName in el._jqEvents) {
                        var handlers = el._jqEvents[eventName];
                        for (var i = 0; i < handlers.length; i++) {
                            el.removeEventListener(eventName, handlers[i]);
                        }
                    }
                    el._jqEvents = {};
                } else {
                    for (var i = 0; i < eventList.length; i++) {
                        var eventName = eventList[i].split('.')[0];
                        var namespace = eventList[i].split('.')[1];

                        if (el._jqEvents[eventName]) {
                            el._jqEvents[eventName] = el._jqEvents[eventName].filter(function(h) {
                                var remove = true;
                                if (handler && h._original !== handler) remove = false;
                                if (namespace && h._namespace !== namespace) remove = false;
                                if (selector && h._selector !== selector) remove = false;
                                if (remove) {
                                    el.removeEventListener(eventName, h);
                                }
                                return !remove;
                            });
                        }
                    }
                }
            });
        },

        one: function(events, selector, data, handler) {
            if (typeof selector === 'function') {
                handler = selector;
                selector = undefined;
                data = undefined;
            } else if (typeof data === 'function') {
                handler = data;
                data = undefined;
            }

            var self = this;
            var oneHandler = function(e) {
                jQuery(this).off(events, selector, oneHandler);
                handler.call(this, e);
            };
            return this.on(events, selector, data, oneHandler);
        },

        trigger: function(eventType, data) {
            return this.each(function() {
                var event;
                if (typeof eventType === 'string') {
                    event = new CustomEvent(eventType, { bubbles: true, cancelable: true, detail: data });
                } else {
                    event = eventType;
                }
                this.dispatchEvent(event);
            });
        },

        triggerHandler: function(eventType, data) {
            if (this[0] && this[0]._jqEvents && this[0]._jqEvents[eventType]) {
                var handlers = this[0]._jqEvents[eventType];
                var event = { type: eventType, data: data, preventDefault: function() {}, stopPropagation: function() {} };
                for (var i = 0; i < handlers.length; i++) {
                    handlers[i].call(this[0], event);
                }
            }
            return this;
        },

        // Focus/blur
        focus: function() {
            if (this[0] && this[0].focus) this[0].focus();
            return this;
        },

        blur: function() {
            if (this[0] && this[0].blur) this[0].blur();
            return this;
        },

        // Serialize form
        serialize: function() {
            var result = [];
            this.find('input, select, textarea').each(function() {
                if (this.name && !this.disabled) {
                    if (this.type === 'checkbox' || this.type === 'radio') {
                        if (this.checked) {
                            result.push(encodeURIComponent(this.name) + '=' + encodeURIComponent(this.value));
                        }
                    } else {
                        result.push(encodeURIComponent(this.name) + '=' + encodeURIComponent(this.value));
                    }
                }
            });
            return result.join('&');
        },

        serializeArray: function() {
            var result = [];
            this.find('input, select, textarea').each(function() {
                if (this.name && !this.disabled) {
                    if (this.type === 'checkbox' || this.type === 'radio') {
                        if (this.checked) {
                            result.push({ name: this.name, value: this.value });
                        }
                    } else {
                        result.push({ name: this.name, value: this.value });
                    }
                }
            });
            return result;
        },

        // Add elements to set
        add: function(selector) {
            var result = this.toArray();
            jQuery(selector).each(function() {
                if (result.indexOf(this) === -1) {
                    result.push(this);
                }
            });
            return jQuery(result);
        },

        // Get contents including text nodes
        contents: function() {
            var result = [];
            this.each(function() {
                var nodes = this.childNodes;
                for (var i = 0; i < nodes.length; i++) {
                    result.push(nodes[i]);
                }
            });
            return jQuery(result);
        },

        // Get index
        index: function(selector) {
            if (!this[0]) return -1;
            if (selector === undefined) {
                var i = 0;
                var el = this[0];
                while ((el = el.previousElementSibling)) i++;
                return i;
            }
            return jQuery(selector).toArray().indexOf(this[0]);
        },

        // Map elements
        map: function(callback) {
            var result = [];
            this.each(function(i) {
                var value = callback.call(this, i, this);
                if (value != null) {
                    if (Array.isArray(value)) {
                        result = result.concat(value);
                    } else {
                        result.push(value);
                    }
                }
            });
            return jQuery(result);
        },

        // Slice
        slice: function(start, end) {
            return jQuery(Array.prototype.slice.call(this, start, end));
        },

        // End chain
        end: function() {
            return this.prevObject || jQuery();
        }
    };

    // Make init use jQuery prototype
    jQuery.fn.init.prototype = jQuery.fn;

    // Event shorthand methods
    var events = ['click', 'dblclick', 'mousedown', 'mouseup', 'mousemove', 'mouseover', 'mouseout', 'mouseenter', 'mouseleave',
                  'keydown', 'keyup', 'keypress', 'submit', 'change', 'focus', 'blur', 'load', 'unload', 'resize', 'scroll',
                  'select', 'error', 'contextmenu', 'focusin', 'focusout', 'input', 'invalid', 'reset', 'search',
                  'drag', 'dragend', 'dragenter', 'dragleave', 'dragover', 'dragstart', 'drop',
                  'touchstart', 'touchmove', 'touchend', 'touchcancel'];

    events.forEach(function(eventName) {
        jQuery.fn[eventName] = function(data, handler) {
            if (handler === undefined && typeof data === 'function') {
                handler = data;
                data = undefined;
            }
            if (handler) {
                return this.on(eventName, handler);
            }
            return this.trigger(eventName);
        };
    });

    // Hover shorthand
    jQuery.fn.hover = function(fnOver, fnOut) {
        return this.mouseenter(fnOver).mouseleave(fnOut || fnOver);
    };

    // Bind/unbind/delegate/undelegate (deprecated but still used)
    jQuery.fn.bind = jQuery.fn.on;
    jQuery.fn.unbind = jQuery.fn.off;
    jQuery.fn.delegate = function(selector, events, handler) {
        return this.on(events, selector, handler);
    };
    jQuery.fn.undelegate = function(selector, events, handler) {
        return this.off(events, selector, handler);
    };
    jQuery.fn.live = function(events, handler) {
        jQuery(document).on(events, this.selector, handler);
        return this;
    };
    jQuery.fn.die = function(events, handler) {
        jQuery(document).off(events, this.selector, handler);
        return this;
    };

    // Static methods
    jQuery.extend = jQuery.fn.extend = function() {
        var target = arguments[0] || {};
        var i = 1;
        var deep = false;

        if (typeof target === 'boolean') {
            deep = target;
            target = arguments[1] || {};
            i = 2;
        }

        if (typeof target !== 'object' && typeof target !== 'function') {
            target = {};
        }

        if (i === arguments.length) {
            target = this;
            i--;
        }

        for (; i < arguments.length; i++) {
            var source = arguments[i];
            if (source != null) {
                for (var key in source) {
                    var src = target[key];
                    var copy = source[key];

                    if (target === copy) continue;

                    if (deep && copy && (jQuery.isPlainObject(copy) || Array.isArray(copy))) {
                        var clone = src && (jQuery.isPlainObject(src) || Array.isArray(src)) ? src : (Array.isArray(copy) ? [] : {});
                        target[key] = jQuery.extend(deep, clone, copy);
                    } else if (copy !== undefined) {
                        target[key] = copy;
                    }
                }
            }
        }

        return target;
    };

    // Utility methods
    jQuery.extend({
        // Type checking
        isFunction: function(obj) {
            return typeof obj === 'function';
        },

        isArray: Array.isArray,

        isPlainObject: function(obj) {
            return obj !== null && typeof obj === 'object' && Object.getPrototypeOf(obj) === Object.prototype;
        },

        isEmptyObject: function(obj) {
            for (var key in obj) {
                return false;
            }
            return true;
        },

        isNumeric: function(obj) {
            return !isNaN(parseFloat(obj)) && isFinite(obj);
        },

        isWindow: function(obj) {
            return obj != null && obj === obj.window;
        },

        type: function(obj) {
            if (obj == null) return obj + '';
            return typeof obj === 'object' || typeof obj === 'function' ?
                Object.prototype.toString.call(obj).slice(8, -1).toLowerCase() : typeof obj;
        },

        // Array/object utilities
        each: function(obj, callback) {
            if (Array.isArray(obj) || obj.length !== undefined) {
                for (var i = 0; i < obj.length; i++) {
                    if (callback.call(obj[i], i, obj[i]) === false) break;
                }
            } else {
                for (var key in obj) {
                    if (callback.call(obj[key], key, obj[key]) === false) break;
                }
            }
            return obj;
        },

        map: function(arr, callback) {
            var result = [];
            if (Array.isArray(arr) || arr.length !== undefined) {
                for (var i = 0; i < arr.length; i++) {
                    var value = callback(arr[i], i);
                    if (value != null) result.push(value);
                }
            } else {
                for (var key in arr) {
                    var value = callback(arr[key], key);
                    if (value != null) result.push(value);
                }
            }
            return result;
        },

        grep: function(arr, callback, invert) {
            var result = [];
            for (var i = 0; i < arr.length; i++) {
                if (callback(arr[i], i) !== !!invert) {
                    result.push(arr[i]);
                }
            }
            return result;
        },

        inArray: function(elem, arr, fromIndex) {
            return arr.indexOf(elem, fromIndex);
        },

        merge: function(first, second) {
            var len = second.length;
            var j = 0;
            var i = first.length;
            for (; j < len; j++) {
                first[i++] = second[j];
            }
            first.length = i;
            return first;
        },

        makeArray: function(arr) {
            return Array.prototype.slice.call(arr);
        },

        unique: function(arr) {
            var result = [];
            for (var i = 0; i < arr.length; i++) {
                if (result.indexOf(arr[i]) === -1) {
                    result.push(arr[i]);
                }
            }
            return result;
        },

        // String utilities
        trim: function(str) {
            return str == null ? '' : String(str).trim();
        },

        // No conflict
        noConflict: function(deep) {
            if (window.$ === jQuery) {
                window.$ = _$;
            }
            if (deep && window.jQuery === jQuery) {
                window.jQuery = _jQuery;
            }
            return jQuery;
        },

        // Ready
        ready: function(fn) {
            if (document.readyState === 'loading') {
                document.addEventListener('DOMContentLoaded', fn);
            } else {
                fn();
            }
        },

        // Deferred (simplified)
        Deferred: function() {
            var callbacks = { done: [], fail: [], always: [] };
            var state = 'pending';
            var result;

            var deferred = {
                state: function() { return state; },
                done: function(fn) {
                    if (state === 'resolved') fn(result);
                    else callbacks.done.push(fn);
                    return this;
                },
                fail: function(fn) {
                    if (state === 'rejected') fn(result);
                    else callbacks.fail.push(fn);
                    return this;
                },
                always: function(fn) {
                    if (state !== 'pending') fn(result);
                    else callbacks.always.push(fn);
                    return this;
                },
                then: function(done, fail) {
                    if (done) this.done(done);
                    if (fail) this.fail(fail);
                    return this;
                },
                promise: function() { return this; },
                resolve: function(value) {
                    if (state !== 'pending') return this;
                    state = 'resolved';
                    result = value;
                    callbacks.done.forEach(function(fn) { fn(value); });
                    callbacks.always.forEach(function(fn) { fn(value); });
                    return this;
                },
                reject: function(value) {
                    if (state !== 'pending') return this;
                    state = 'rejected';
                    result = value;
                    callbacks.fail.forEach(function(fn) { fn(value); });
                    callbacks.always.forEach(function(fn) { fn(value); });
                    return this;
                }
            };

            return deferred;
        },

        when: function() {
            var args = Array.prototype.slice.call(arguments);
            var deferred = jQuery.Deferred();

            if (args.length === 0) {
                deferred.resolve();
                return deferred;
            }

            var remaining = args.length;
            var results = new Array(args.length);

            args.forEach(function(arg, i) {
                if (arg && typeof arg.then === 'function') {
                    arg.then(function(value) {
                        results[i] = value;
                        if (--remaining === 0) {
                            deferred.resolve.apply(deferred, results);
                        }
                    }, function(err) {
                        deferred.reject(err);
                    });
                } else {
                    results[i] = arg;
                    if (--remaining === 0) {
                        deferred.resolve.apply(deferred, results);
                    }
                }
            });

            return deferred;
        },

        // Callbacks (simplified)
        Callbacks: function(options) {
            var list = [];
            var fired = false;
            var lastArgs;

            var self = {
                add: function(fn) {
                    list.push(fn);
                    if (fired && options && options.indexOf('memory') !== -1) {
                        fn.apply(null, lastArgs);
                    }
                    return this;
                },
                remove: function(fn) {
                    var i = list.indexOf(fn);
                    if (i !== -1) list.splice(i, 1);
                    return this;
                },
                fire: function() {
                    fired = true;
                    lastArgs = arguments;
                    list.forEach(function(fn) {
                        fn.apply(null, lastArgs);
                    });
                    return this;
                },
                empty: function() {
                    list = [];
                    return this;
                },
                has: function(fn) {
                    return fn ? list.indexOf(fn) !== -1 : list.length > 0;
                }
            };

            return self;
        },

        // Global eval
        globalEval: function(code) {
            var script = document.createElement('script');
            script.text = code;
            document.head.appendChild(script).parentNode.removeChild(script);
        },

        // Parse HTML
        parseHTML: function(str, context, keepScripts) {
            var div = document.createElement('div');
            div.innerHTML = str;
            var result = [];
            var nodes = div.childNodes;
            for (var i = 0; i < nodes.length; i++) {
                if (keepScripts || nodes[i].nodeName !== 'SCRIPT') {
                    result.push(nodes[i]);
                }
            }
            return result;
        },

        // Parse JSON
        parseJSON: JSON.parse,

        // Noop
        noop: function() {},

        // Now
        now: Date.now,

        // Proxy
        proxy: function(fn, context) {
            if (typeof context === 'string') {
                var tmp = fn[context];
                context = fn;
                fn = tmp;
            }
            return function() {
                return fn.apply(context, arguments);
            };
        },

        // Contains
        contains: function(container, contained) {
            return container !== contained && container.contains(contained);
        },

        // Data (simplified)
        data: function(elem, key, value) {
            elem._jqData = elem._jqData || {};
            if (value === undefined) {
                return key === undefined ? elem._jqData : elem._jqData[key];
            }
            elem._jqData[key] = value;
        },

        removeData: function(elem, key) {
            if (elem._jqData) {
                if (key === undefined) {
                    elem._jqData = {};
                } else {
                    delete elem._jqData[key];
                }
            }
        },

        // Queue (simplified)
        queue: function(elem, type, data) {
            type = (type || 'fx') + 'queue';
            elem._jqQueue = elem._jqQueue || {};
            elem._jqQueue[type] = elem._jqQueue[type] || [];
            if (data) elem._jqQueue[type].push(data);
            return elem._jqQueue[type];
        },

        dequeue: function(elem, type) {
            type = (type || 'fx') + 'queue';
            if (elem._jqQueue && elem._jqQueue[type]) {
                var fn = elem._jqQueue[type].shift();
                if (fn) fn();
            }
        },

        // Support object (for feature detection)
        support: {},

        // Expr for Sizzle compatibility
        expr: {
            ':': {},
            match: {},
            filter: {}
        },

        // Version
        fn: { jquery: '3.7.1' }
    });

    // AJAX
    jQuery.ajaxSettings = {
        type: 'GET',
        contentType: 'application/x-www-form-urlencoded; charset=UTF-8',
        processData: true,
        async: true,
        timeout: 0,
        dataType: 'text',
        accepts: {
            '*': '*/*',
            text: 'text/plain',
            html: 'text/html',
            json: 'application/json',
            xml: 'application/xml'
        },
        converters: {
            'text json': JSON.parse
        }
    };

    jQuery.ajax = function(url, options) {
        if (typeof url === 'object') {
            options = url;
            url = options.url;
        }

        options = jQuery.extend({}, jQuery.ajaxSettings, options);
        options.url = url || options.url || location.href;
        options.type = (options.type || options.method || 'GET').toUpperCase();

        var deferred = jQuery.Deferred();
        var xhr = new XMLHttpRequest();

        // Build URL with data for GET requests
        if (options.data && options.type === 'GET') {
            var params = typeof options.data === 'string' ? options.data : jQuery.param(options.data);
            options.url += (options.url.indexOf('?') === -1 ? '?' : '&') + params;
        }

        xhr.open(options.type, options.url, options.async !== false);

        // Set headers
        if (options.contentType !== false) {
            xhr.setRequestHeader('Content-Type', options.contentType);
        }
        xhr.setRequestHeader('X-Requested-With', 'XMLHttpRequest');

        if (options.headers) {
            for (var header in options.headers) {
                xhr.setRequestHeader(header, options.headers[header]);
            }
        }

        // Handle timeout
        if (options.timeout > 0) {
            xhr.timeout = options.timeout;
        }

        xhr.onreadystatechange = function() {
            if (xhr.readyState === 4) {
                var status = xhr.status;
                var response = xhr.responseText;

                // Try to parse response based on dataType
                if (options.dataType === 'json' || (options.dataType === 'auto' && xhr.getResponseHeader('Content-Type') && xhr.getResponseHeader('Content-Type').indexOf('json') !== -1)) {
                    try {
                        response = JSON.parse(response);
                    } catch (e) {}
                }

                if (status >= 200 && status < 300 || status === 304) {
                    if (options.success) options.success(response, 'success', xhr);
                    deferred.resolve(response, 'success', xhr);
                } else {
                    if (options.error) options.error(xhr, 'error', xhr.statusText);
                    deferred.reject(xhr, 'error', xhr.statusText);
                }

                if (options.complete) options.complete(xhr, status >= 200 && status < 300 ? 'success' : 'error');
            }
        };

        xhr.onerror = function() {
            if (options.error) options.error(xhr, 'error', 'Network error');
            deferred.reject(xhr, 'error', 'Network error');
            if (options.complete) options.complete(xhr, 'error');
        };

        xhr.ontimeout = function() {
            if (options.error) options.error(xhr, 'timeout', 'Timeout');
            deferred.reject(xhr, 'timeout', 'Timeout');
            if (options.complete) options.complete(xhr, 'timeout');
        };

        // Before send
        if (options.beforeSend && options.beforeSend(xhr, options) === false) {
            return deferred;
        }

        // Send request
        var data = null;
        if (options.data && options.type !== 'GET') {
            data = options.processData && typeof options.data === 'object' ? jQuery.param(options.data) : options.data;
        }

        try {
            xhr.send(data);
        } catch (e) {
            if (options.error) options.error(xhr, 'error', e.message);
            deferred.reject(xhr, 'error', e.message);
        }

        // Add promise methods to xhr
        deferred.promise(xhr);

        return xhr;
    };

    // AJAX shortcuts
    jQuery.get = function(url, data, callback, type) {
        if (typeof data === 'function') {
            type = type || callback;
            callback = data;
            data = undefined;
        }
        return jQuery.ajax({
            url: url,
            type: 'GET',
            data: data,
            success: callback,
            dataType: type
        });
    };

    jQuery.post = function(url, data, callback, type) {
        if (typeof data === 'function') {
            type = type || callback;
            callback = data;
            data = undefined;
        }
        return jQuery.ajax({
            url: url,
            type: 'POST',
            data: data,
            success: callback,
            dataType: type
        });
    };

    jQuery.getJSON = function(url, data, callback) {
        if (typeof data === 'function') {
            callback = data;
            data = undefined;
        }
        return jQuery.ajax({
            url: url,
            type: 'GET',
            data: data,
            success: callback,
            dataType: 'json'
        });
    };

    jQuery.getScript = function(url, callback) {
        return jQuery.ajax({
            url: url,
            type: 'GET',
            dataType: 'script',
            success: function(script) {
                jQuery.globalEval(script);
                if (callback) callback();
            }
        });
    };

    // Serialize object to query string
    jQuery.param = function(obj, traditional) {
        var pairs = [];

        function add(key, value) {
            value = typeof value === 'function' ? value() : (value == null ? '' : value);
            pairs.push(encodeURIComponent(key) + '=' + encodeURIComponent(value));
        }

        function buildParams(prefix, obj) {
            if (Array.isArray(obj)) {
                obj.forEach(function(item, i) {
                    if (traditional || /\[\]$/.test(prefix)) {
                        add(prefix, item);
                    } else {
                        buildParams(prefix + '[' + (typeof item === 'object' ? i : '') + ']', item);
                    }
                });
            } else if (typeof obj === 'object') {
                for (var key in obj) {
                    buildParams(prefix + '[' + key + ']', obj[key]);
                }
            } else {
                add(prefix, obj);
            }
        }

        if (Array.isArray(obj)) {
            obj.forEach(function(item) {
                add(item.name, item.value);
            });
        } else {
            for (var key in obj) {
                buildParams(key, obj[key]);
            }
        }

        return pairs.join('&');
    };

    // AJAX events
    jQuery.ajaxSetup = function(options) {
        jQuery.extend(jQuery.ajaxSettings, options);
    };

    // Event trigger for AJAX events
    jQuery.event = jQuery.event || {};
    jQuery.event.trigger = function(event, data, elem) {
        jQuery(elem || document).trigger(event, data);
    };

    // Store previous values for noConflict
    var _jQuery = window.jQuery;
    var _$ = window.$;

    // Export
    window.jQuery = window.$ = jQuery;

})(window);
"#;

    // Execute the jQuery initialization code
    context.eval(boa_engine::Source::from_bytes(jquery_code.as_bytes()))
        .map_err(|e| {
            boa_engine::JsError::from_opaque(JsValue::from(js_string!(format!(
                "Failed to initialize jQuery: {}",
                e
            ))))
        })?;

    log::debug!("jQuery initialized successfully");

    Ok(())
}
