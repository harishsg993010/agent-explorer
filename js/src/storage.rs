// Full Storage APIs with SQLite backing
// IndexedDB, localStorage, sessionStorage

use boa_engine::{
    Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction,
    object::ObjectInitializer, property::Attribute, JsObject,
    js_string,
};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

// ============================================================================
// SQLite Database Manager
// ============================================================================

lazy_static::lazy_static! {
    static ref DB_PATH: Mutex<PathBuf> = Mutex::new(
        std::env::temp_dir().join("semantic_browser_storage.db")
    );
}

fn get_connection() -> rusqlite::Result<Connection> {
    let path = DB_PATH.lock().unwrap().clone();
    let conn = Connection::open(&path)?;

    // Initialize schema
    conn.execute_batch(r#"
        -- LocalStorage table
        CREATE TABLE IF NOT EXISTS local_storage (
            origin TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY (origin, key)
        );

        -- SessionStorage table (same schema, different table)
        CREATE TABLE IF NOT EXISTS session_storage (
            origin TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            PRIMARY KEY (origin, key)
        );

        -- IndexedDB databases
        CREATE TABLE IF NOT EXISTS idb_databases (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            origin TEXT NOT NULL,
            name TEXT NOT NULL,
            version INTEGER NOT NULL DEFAULT 1,
            UNIQUE(origin, name)
        );

        -- IndexedDB object stores
        CREATE TABLE IF NOT EXISTS idb_object_stores (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            database_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            key_path TEXT,
            auto_increment INTEGER NOT NULL DEFAULT 0,
            current_key INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (database_id) REFERENCES idb_databases(id) ON DELETE CASCADE,
            UNIQUE(database_id, name)
        );

        -- IndexedDB indexes
        CREATE TABLE IF NOT EXISTS idb_indexes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            object_store_id INTEGER NOT NULL,
            name TEXT NOT NULL,
            key_path TEXT NOT NULL,
            unique_index INTEGER NOT NULL DEFAULT 0,
            multi_entry INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (object_store_id) REFERENCES idb_object_stores(id) ON DELETE CASCADE,
            UNIQUE(object_store_id, name)
        );

        -- IndexedDB records
        CREATE TABLE IF NOT EXISTS idb_records (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            object_store_id INTEGER NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            FOREIGN KEY (object_store_id) REFERENCES idb_object_stores(id) ON DELETE CASCADE,
            UNIQUE(object_store_id, key)
        );
    "#)?;

    Ok(conn)
}

// ============================================================================
// Storage Value Serialization
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum StorageValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<StorageValue>),
    Object(HashMap<String, StorageValue>),
}

impl StorageValue {
    fn from_js_value(value: &JsValue, context: &mut Context) -> Self {
        if value.is_null() || value.is_undefined() {
            StorageValue::Null
        } else if let Some(b) = value.as_boolean() {
            StorageValue::Bool(b)
        } else if let Ok(n) = value.to_number(context) {
            StorageValue::Number(n)
        } else if let Some(s) = value.as_string() {
            StorageValue::String(s.to_std_string_escaped())
        } else if let Some(obj) = value.as_object() {
            if obj.is_array() {
                let length = obj.get(js_string!("length"), context)
                    .ok()
                    .and_then(|v| v.to_u32(context).ok())
                    .unwrap_or(0);
                let mut arr = Vec::new();
                for i in 0..length {
                    if let Ok(item) = obj.get(i, context) {
                        arr.push(StorageValue::from_js_value(&item, context));
                    }
                }
                StorageValue::Array(arr)
            } else {
                // Simplified object serialization - just store as string
                StorageValue::String("[object Object]".to_string())
            }
        } else {
            StorageValue::String(value.to_string(context)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default())
        }
    }

    fn to_js_value(&self, context: &mut Context) -> JsResult<JsValue> {
        match self {
            StorageValue::Null => Ok(JsValue::null()),
            StorageValue::Bool(b) => Ok(JsValue::from(*b)),
            StorageValue::Number(n) => Ok(JsValue::from(*n)),
            StorageValue::String(s) => Ok(JsValue::from(js_string!(s.clone()))),
            StorageValue::Array(arr) => {
                let js_arr = boa_engine::object::builtins::JsArray::new(context);
                for (i, item) in arr.iter().enumerate() {
                    let val = item.to_js_value(context)?;
                    js_arr.set(i as u32, val, false, context)?;
                }
                Ok(JsValue::from(js_arr))
            }
            StorageValue::Object(map) => {
                let obj = ObjectInitializer::new(context).build();
                for (key, val) in map {
                    let js_val = val.to_js_value(context)?;
                    obj.set(js_string!(key.clone()), js_val, false, context)?;
                }
                Ok(JsValue::from(obj))
            }
        }
    }
}

// ============================================================================
// Thread-Local State for Current Origin
// ============================================================================

thread_local! {
    static CURRENT_ORIGIN: RefCell<String> = RefCell::new("https://localhost".to_string());
}

pub fn set_current_origin(origin: &str) {
    CURRENT_ORIGIN.with(|o| {
        *o.borrow_mut() = origin.to_string();
    });
}

fn get_current_origin() -> String {
    CURRENT_ORIGIN.with(|o| o.borrow().clone())
}

// ============================================================================
// IDBKeyRange
// ============================================================================

pub fn register_idb_key_range(context: &mut Context) -> JsResult<()> {
    let only = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let value = args.get_or_undefined(0);
        let range = ObjectInitializer::new(ctx)
            .property(js_string!("lower"), value.clone(), Attribute::READONLY)
            .property(js_string!("upper"), value.clone(), Attribute::READONLY)
            .property(js_string!("lowerOpen"), false, Attribute::READONLY)
            .property(js_string!("upperOpen"), false, Attribute::READONLY)
            .build();
        Ok(JsValue::from(range))
    });

    let lower_bound = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let lower = args.get_or_undefined(0);
        let open = args.get(1).map(|v| v.to_boolean()).unwrap_or(false);
        let range = ObjectInitializer::new(ctx)
            .property(js_string!("lower"), lower.clone(), Attribute::READONLY)
            .property(js_string!("upper"), JsValue::undefined(), Attribute::READONLY)
            .property(js_string!("lowerOpen"), open, Attribute::READONLY)
            .property(js_string!("upperOpen"), true, Attribute::READONLY)
            .build();
        Ok(JsValue::from(range))
    });

    let upper_bound = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let upper = args.get_or_undefined(0);
        let open = args.get(1).map(|v| v.to_boolean()).unwrap_or(false);
        let range = ObjectInitializer::new(ctx)
            .property(js_string!("lower"), JsValue::undefined(), Attribute::READONLY)
            .property(js_string!("upper"), upper.clone(), Attribute::READONLY)
            .property(js_string!("lowerOpen"), true, Attribute::READONLY)
            .property(js_string!("upperOpen"), open, Attribute::READONLY)
            .build();
        Ok(JsValue::from(range))
    });

    let bound = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let lower = args.get_or_undefined(0);
        let upper = args.get_or_undefined(1);
        let lower_open = args.get(2).map(|v| v.to_boolean()).unwrap_or(false);
        let upper_open = args.get(3).map(|v| v.to_boolean()).unwrap_or(false);
        let range = ObjectInitializer::new(ctx)
            .property(js_string!("lower"), lower.clone(), Attribute::READONLY)
            .property(js_string!("upper"), upper.clone(), Attribute::READONLY)
            .property(js_string!("lowerOpen"), lower_open, Attribute::READONLY)
            .property(js_string!("upperOpen"), upper_open, Attribute::READONLY)
            .build();
        Ok(JsValue::from(range))
    });

    let includes = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], _: &mut Context| {
        Ok(JsValue::from(true))
    });

    let idb_key_range = ObjectInitializer::new(context)
        .function(only, js_string!("only"), 1)
        .function(lower_bound, js_string!("lowerBound"), 2)
        .function(upper_bound, js_string!("upperBound"), 2)
        .function(bound, js_string!("bound"), 4)
        .function(includes, js_string!("includes"), 1)
        .build();

    context.global_object().set(
        js_string!("IDBKeyRange"),
        idb_key_range,
        false,
        context
    )?;

    Ok(())
}

// ============================================================================
// IDBRequest Helper
// ============================================================================

fn create_idb_request(context: &mut Context) -> JsResult<JsObject> {
    let request = ObjectInitializer::new(context)
        .property(js_string!("result"), JsValue::undefined(), Attribute::all())
        .property(js_string!("error"), JsValue::null(), Attribute::all())
        .property(js_string!("source"), JsValue::null(), Attribute::all())
        .property(js_string!("transaction"), JsValue::null(), Attribute::all())
        .property(js_string!("readyState"), js_string!("pending"), Attribute::all())
        .property(js_string!("onsuccess"), JsValue::null(), Attribute::all())
        .property(js_string!("onerror"), JsValue::null(), Attribute::all())
        .build();
    Ok(request)
}

fn create_idb_open_db_request(context: &mut Context) -> JsResult<JsObject> {
    let request = create_idb_request(context)?;
    request.set(js_string!("onupgradeneeded"), JsValue::null(), false, context)?;
    request.set(js_string!("onblocked"), JsValue::null(), false, context)?;
    Ok(request)
}

// ============================================================================
// IDBObjectStore
// ============================================================================

fn create_idb_object_store(
    context: &mut Context,
    store_id: i64,
    name: &str,
    key_path: Option<&str>,
    auto_increment: bool,
) -> JsResult<JsObject> {
    let index_names = boa_engine::object::builtins::JsArray::new(context);

    let store = ObjectInitializer::new(context)
        .property(js_string!("name"), js_string!(name.to_string()), Attribute::READONLY)
        .property(
            js_string!("keyPath"),
            key_path.map(|s| JsValue::from(js_string!(s.to_string()))).unwrap_or(JsValue::null()),
            Attribute::READONLY
        )
        .property(js_string!("autoIncrement"), auto_increment, Attribute::READONLY)
        .property(js_string!("indexNames"), index_names, Attribute::READONLY)
        .property(js_string!("_storeId"), store_id as f64, Attribute::empty())
        .build();

    // put(value, key?)
    let store_id_put = store_id;
    let put = NativeFunction::from_copy_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let value = args.get_or_undefined(0);
        let key_arg = args.get(1);

        let value_storage = StorageValue::from_js_value(value, ctx);
        let value_json = serde_json::to_string(&value_storage).unwrap_or_default();

        let key_json = if let Some(k) = key_arg {
            if !k.is_undefined() {
                serde_json::to_string(&StorageValue::from_js_value(k, ctx)).unwrap_or_default()
            } else {
                generate_auto_key(store_id_put)
            }
        } else {
            generate_auto_key(store_id_put)
        };

        let request = create_idb_request(ctx)?;

        if let Ok(conn) = get_connection() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO idb_records (object_store_id, key, value) VALUES (?, ?, ?)",
                params![store_id_put, key_json, value_json]
            );
            let key_val: StorageValue = serde_json::from_str(&key_json).unwrap_or(StorageValue::Null);
            request.set(js_string!("result"), key_val.to_js_value(ctx)?, false, ctx)?;
            request.set(js_string!("readyState"), js_string!("done"), false, ctx)?;
        }

        Ok(JsValue::from(request))
    });

    // add(value, key?)
    let store_id_add = store_id;
    let add = NativeFunction::from_copy_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let value = args.get_or_undefined(0);
        let key_arg = args.get(1);

        let value_storage = StorageValue::from_js_value(value, ctx);
        let value_json = serde_json::to_string(&value_storage).unwrap_or_default();

        let key_json = if let Some(k) = key_arg {
            if !k.is_undefined() {
                serde_json::to_string(&StorageValue::from_js_value(k, ctx)).unwrap_or_default()
            } else {
                generate_auto_key(store_id_add)
            }
        } else {
            generate_auto_key(store_id_add)
        };

        let request = create_idb_request(ctx)?;

        if let Ok(conn) = get_connection() {
            let exists: bool = conn.query_row(
                "SELECT 1 FROM idb_records WHERE object_store_id = ? AND key = ?",
                params![store_id_add, key_json],
                |_| Ok(true)
            ).unwrap_or(false);

            if exists {
                request.set(js_string!("error"), js_string!("Key already exists"), false, ctx)?;
            } else {
                let _ = conn.execute(
                    "INSERT INTO idb_records (object_store_id, key, value) VALUES (?, ?, ?)",
                    params![store_id_add, key_json, value_json]
                );
                let key_val: StorageValue = serde_json::from_str(&key_json).unwrap_or(StorageValue::Null);
                request.set(js_string!("result"), key_val.to_js_value(ctx)?, false, ctx)?;
            }
            request.set(js_string!("readyState"), js_string!("done"), false, ctx)?;
        }

        Ok(JsValue::from(request))
    });

    // get(key)
    let store_id_get = store_id;
    let get = NativeFunction::from_copy_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let key = args.get_or_undefined(0);
        let key_json = serde_json::to_string(&StorageValue::from_js_value(key, ctx)).unwrap_or_default();

        let request = create_idb_request(ctx)?;

        if let Ok(conn) = get_connection() {
            match conn.query_row(
                "SELECT value FROM idb_records WHERE object_store_id = ? AND key = ?",
                params![store_id_get, key_json],
                |row| row.get::<_, String>(0)
            ) {
                Ok(value_json) => {
                    let value: StorageValue = serde_json::from_str(&value_json).unwrap_or(StorageValue::Null);
                    request.set(js_string!("result"), value.to_js_value(ctx)?, false, ctx)?;
                }
                Err(_) => {
                    request.set(js_string!("result"), JsValue::undefined(), false, ctx)?;
                }
            }
            request.set(js_string!("readyState"), js_string!("done"), false, ctx)?;
        }

        Ok(JsValue::from(request))
    });

    // getAll(query?, count?)
    let store_id_get_all = store_id;
    let get_all = NativeFunction::from_copy_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let count = args.get(1).and_then(|v| v.to_u32(ctx).ok()).unwrap_or(u32::MAX);
        let request = create_idb_request(ctx)?;

        if let Ok(conn) = get_connection() {
            let mut stmt = conn.prepare(
                "SELECT value FROM idb_records WHERE object_store_id = ? LIMIT ?"
            ).ok();

            if let Some(ref mut stmt) = stmt {
                let values: Vec<String> = stmt.query_map(params![store_id_get_all, count], |row| row.get(0))
                    .ok()
                    .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    .unwrap_or_default();

                let arr = boa_engine::object::builtins::JsArray::new(ctx);
                for (i, value_json) in values.iter().enumerate() {
                    let value: StorageValue = serde_json::from_str(value_json).unwrap_or(StorageValue::Null);
                    arr.set(i as u32, value.to_js_value(ctx)?, false, ctx)?;
                }
                request.set(js_string!("result"), JsValue::from(arr), false, ctx)?;
            }
            request.set(js_string!("readyState"), js_string!("done"), false, ctx)?;
        }

        Ok(JsValue::from(request))
    });

    // delete(key)
    let store_id_delete = store_id;
    let delete = NativeFunction::from_copy_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let key = args.get_or_undefined(0);
        let key_json = serde_json::to_string(&StorageValue::from_js_value(key, ctx)).unwrap_or_default();

        let request = create_idb_request(ctx)?;

        if let Ok(conn) = get_connection() {
            let _ = conn.execute(
                "DELETE FROM idb_records WHERE object_store_id = ? AND key = ?",
                params![store_id_delete, key_json]
            );
            request.set(js_string!("readyState"), js_string!("done"), false, ctx)?;
        }

        Ok(JsValue::from(request))
    });

    // clear()
    let store_id_clear = store_id;
    let clear = NativeFunction::from_copy_closure(move |_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        let request = create_idb_request(ctx)?;

        if let Ok(conn) = get_connection() {
            let _ = conn.execute(
                "DELETE FROM idb_records WHERE object_store_id = ?",
                params![store_id_clear]
            );
            request.set(js_string!("readyState"), js_string!("done"), false, ctx)?;
        }

        Ok(JsValue::from(request))
    });

    // count(key?)
    let store_id_count = store_id;
    let count = NativeFunction::from_copy_closure(move |_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        let request = create_idb_request(ctx)?;

        if let Ok(conn) = get_connection() {
            match conn.query_row(
                "SELECT COUNT(*) FROM idb_records WHERE object_store_id = ?",
                params![store_id_count],
                |row| row.get::<_, i64>(0)
            ) {
                Ok(cnt) => {
                    request.set(js_string!("result"), JsValue::from(cnt as i32), false, ctx)?;
                }
                Err(_) => {
                    request.set(js_string!("result"), JsValue::from(0), false, ctx)?;
                }
            }
            request.set(js_string!("readyState"), js_string!("done"), false, ctx)?;
        }

        Ok(JsValue::from(request))
    });

    // openCursor - simplified
    let open_cursor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        let request = create_idb_request(ctx)?;
        request.set(js_string!("result"), JsValue::null(), false, ctx)?;
        request.set(js_string!("readyState"), js_string!("done"), false, ctx)?;
        Ok(JsValue::from(request))
    });

    // createIndex
    let store_id_idx = store_id;
    let create_index = NativeFunction::from_copy_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let key_path = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

        let options = args.get(2);
        let unique = options
            .and_then(|o| o.as_object())
            .and_then(|o| o.get(js_string!("unique"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);
        let multi_entry = options
            .and_then(|o| o.as_object())
            .and_then(|o| o.get(js_string!("multiEntry"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);

        if let Ok(conn) = get_connection() {
            let _ = conn.execute(
                "INSERT INTO idb_indexes (object_store_id, name, key_path, unique_index, multi_entry) VALUES (?, ?, ?, ?, ?)",
                params![store_id_idx, name, key_path, unique as i32, multi_entry as i32]
            );
        }

        let index = ObjectInitializer::new(ctx)
            .property(js_string!("name"), js_string!(name), Attribute::READONLY)
            .property(js_string!("keyPath"), js_string!(key_path), Attribute::READONLY)
            .property(js_string!("unique"), unique, Attribute::READONLY)
            .property(js_string!("multiEntry"), multi_entry, Attribute::READONLY)
            .build();

        Ok(JsValue::from(index))
    });

    store.set(js_string!("put"), put.to_js_function(context.realm()), false, context)?;
    store.set(js_string!("add"), add.to_js_function(context.realm()), false, context)?;
    store.set(js_string!("get"), get.to_js_function(context.realm()), false, context)?;
    store.set(js_string!("getAll"), get_all.to_js_function(context.realm()), false, context)?;
    store.set(js_string!("delete"), delete.to_js_function(context.realm()), false, context)?;
    store.set(js_string!("clear"), clear.to_js_function(context.realm()), false, context)?;
    store.set(js_string!("count"), count.to_js_function(context.realm()), false, context)?;
    store.set(js_string!("openCursor"), open_cursor.to_js_function(context.realm()), false, context)?;
    store.set(js_string!("createIndex"), create_index.to_js_function(context.realm()), false, context)?;

    Ok(store)
}

fn generate_auto_key(store_id: i64) -> String {
    if let Ok(conn) = get_connection() {
        let current: i64 = conn.query_row(
            "SELECT current_key FROM idb_object_stores WHERE id = ?",
            [store_id],
            |row| row.get(0)
        ).unwrap_or(0);

        let _ = conn.execute(
            "UPDATE idb_object_stores SET current_key = ? WHERE id = ?",
            params![current + 1, store_id]
        );

        return serde_json::to_string(&(current + 1)).unwrap_or_default();
    }
    "1".to_string()
}

// ============================================================================
// IDBTransaction
// ============================================================================

fn create_idb_transaction(
    context: &mut Context,
    db_id: i64,
    store_names: Vec<String>,
    mode: &str,
) -> JsResult<JsObject> {
    let store_names_arr = boa_engine::object::builtins::JsArray::new(context);
    for (i, name) in store_names.iter().enumerate() {
        store_names_arr.set(i as u32, js_string!(name.clone()), false, context)?;
    }

    let transaction = ObjectInitializer::new(context)
        .property(js_string!("mode"), js_string!(mode.to_string()), Attribute::READONLY)
        .property(js_string!("objectStoreNames"), store_names_arr, Attribute::READONLY)
        .property(js_string!("error"), JsValue::null(), Attribute::all())
        .property(js_string!("durability"), js_string!("default"), Attribute::READONLY)
        .property(js_string!("onabort"), JsValue::null(), Attribute::all())
        .property(js_string!("oncomplete"), JsValue::null(), Attribute::all())
        .property(js_string!("onerror"), JsValue::null(), Attribute::all())
        .property(js_string!("_dbId"), db_id as f64, Attribute::empty())
        .build();

    // objectStore(name)
    let object_store = NativeFunction::from_copy_closure(move |this: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let db_id = if let Some(obj) = this.as_object() {
            obj.get(js_string!("_dbId"), ctx)?
                .to_number(ctx)
                .unwrap_or(0.0) as i64
        } else {
            return Err(JsNativeError::error().with_message("Invalid transaction").into());
        };

        if let Ok(conn) = get_connection() {
            match conn.query_row(
                "SELECT id, key_path, auto_increment FROM idb_object_stores WHERE database_id = ? AND name = ?",
                params![db_id, name],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, i32>(2)?))
            ) {
                Ok((store_id, key_path, auto_inc)) => {
                    let store = create_idb_object_store(ctx, store_id, &name, key_path.as_deref(), auto_inc != 0)?;
                    return Ok(JsValue::from(store));
                }
                Err(_) => {
                    return Err(JsNativeError::error()
                        .with_message(format!("Object store '{}' not found", name))
                        .into());
                }
            }
        }

        Err(JsNativeError::error().with_message("Database connection failed").into())
    });

    let abort = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], _: &mut Context| {
        Ok(JsValue::undefined())
    });

    let commit = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], _: &mut Context| {
        Ok(JsValue::undefined())
    });

    transaction.set(js_string!("objectStore"), object_store.to_js_function(context.realm()), false, context)?;
    transaction.set(js_string!("abort"), abort.to_js_function(context.realm()), false, context)?;
    transaction.set(js_string!("commit"), commit.to_js_function(context.realm()), false, context)?;

    Ok(transaction)
}

// ============================================================================
// IDBDatabase
// ============================================================================

fn create_idb_database(
    context: &mut Context,
    db_id: i64,
    name: &str,
    version: u32,
) -> JsResult<JsObject> {
    let object_store_names = boa_engine::object::builtins::JsArray::new(context);
    let mut store_names_vec = Vec::new();

    if let Ok(conn) = get_connection() {
        let mut stmt = conn.prepare("SELECT name FROM idb_object_stores WHERE database_id = ?").ok();
        if let Some(ref mut stmt) = stmt {
            let names: Vec<String> = stmt.query_map([db_id], |row| row.get(0))
                .ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default();

            for (i, store_name) in names.iter().enumerate() {
                object_store_names.set(i as u32, js_string!(store_name.clone()), false, context)?;
                store_names_vec.push(store_name.clone());
            }
        }
    }

    let database = ObjectInitializer::new(context)
        .property(js_string!("name"), js_string!(name.to_string()), Attribute::READONLY)
        .property(js_string!("version"), version, Attribute::READONLY)
        .property(js_string!("objectStoreNames"), object_store_names, Attribute::READONLY)
        .property(js_string!("onabort"), JsValue::null(), Attribute::all())
        .property(js_string!("onclose"), JsValue::null(), Attribute::all())
        .property(js_string!("onerror"), JsValue::null(), Attribute::all())
        .property(js_string!("onversionchange"), JsValue::null(), Attribute::all())
        .property(js_string!("_dbId"), db_id as f64, Attribute::empty())
        .build();

    // createObjectStore(name, options?)
    let db_id_create = db_id;
    let create_object_store = NativeFunction::from_copy_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        let options = args.get(1);
        let key_path = options
            .and_then(|o| o.as_object())
            .and_then(|o| o.get(js_string!("keyPath"), ctx).ok())
            .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()));
        let auto_increment = options
            .and_then(|o| o.as_object())
            .and_then(|o| o.get(js_string!("autoIncrement"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);

        if let Ok(conn) = get_connection() {
            match conn.execute(
                "INSERT INTO idb_object_stores (database_id, name, key_path, auto_increment) VALUES (?, ?, ?, ?)",
                params![db_id_create, name, key_path, auto_increment as i32]
            ) {
                Ok(_) => {
                    let store_id = conn.last_insert_rowid();
                    let store = create_idb_object_store(ctx, store_id, &name, key_path.as_deref(), auto_increment)?;
                    return Ok(JsValue::from(store));
                }
                Err(e) => {
                    return Err(JsNativeError::error().with_message(e.to_string()).into());
                }
            }
        }

        Err(JsNativeError::error().with_message("Database connection failed").into())
    });

    // deleteObjectStore(name)
    let db_id_delete = db_id;
    let delete_object_store = NativeFunction::from_copy_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        if let Ok(conn) = get_connection() {
            let _ = conn.execute(
                "DELETE FROM idb_object_stores WHERE database_id = ? AND name = ?",
                params![db_id_delete, name]
            );
        }

        Ok(JsValue::undefined())
    });

    // transaction(storeNames, mode?)
    let transaction = NativeFunction::from_copy_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let store_names_arg = args.get_or_undefined(0);
        let mode = args.get(1)
            .and_then(|v| v.as_string())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_else(|| "readonly".to_string());

        let store_names: Vec<String> = if let Some(s) = store_names_arg.as_string() {
            vec![s.to_std_string_escaped()]
        } else if let Some(obj) = store_names_arg.as_object() {
            if obj.is_array() {
                let len = obj.get(js_string!("length"), ctx)?.to_u32(ctx).unwrap_or(0);
                let mut names = Vec::new();
                for i in 0..len {
                    if let Ok(val) = obj.get(i, ctx) {
                        if let Some(s) = val.as_string() {
                            names.push(s.to_std_string_escaped());
                        }
                    }
                }
                names
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let tx = create_idb_transaction(ctx, db_id, store_names, &mode)?;
        Ok(JsValue::from(tx))
    });

    let close = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], _: &mut Context| {
        Ok(JsValue::undefined())
    });

    database.set(js_string!("createObjectStore"), create_object_store.to_js_function(context.realm()), false, context)?;
    database.set(js_string!("deleteObjectStore"), delete_object_store.to_js_function(context.realm()), false, context)?;
    database.set(js_string!("transaction"), transaction.to_js_function(context.realm()), false, context)?;
    database.set(js_string!("close"), close.to_js_function(context.realm()), false, context)?;

    Ok(database)
}

// ============================================================================
// IDBFactory (indexedDB)
// ============================================================================

pub fn register_indexed_db(context: &mut Context) -> JsResult<()> {
    let open = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let version = args.get(1).and_then(|v| v.to_u32(ctx).ok()).unwrap_or(1);

        let origin = get_current_origin();
        let request = create_idb_open_db_request(ctx)?;

        if let Ok(conn) = get_connection() {
            let existing: Option<(i64, u32)> = conn.query_row(
                "SELECT id, version FROM idb_databases WHERE origin = ? AND name = ?",
                params![origin, name],
                |row| Ok((row.get(0)?, row.get(1)?))
            ).ok();

            let (db_id, old_version, needs_upgrade) = match existing {
                Some((id, old_ver)) => {
                    if version > old_ver {
                        let _ = conn.execute(
                            "UPDATE idb_databases SET version = ? WHERE id = ?",
                            params![version, id]
                        );
                        (id, old_ver, true)
                    } else {
                        (id, old_ver, false)
                    }
                }
                None => {
                    let _ = conn.execute(
                        "INSERT INTO idb_databases (origin, name, version) VALUES (?, ?, ?)",
                        params![origin, name, version]
                    );
                    let id = conn.last_insert_rowid();
                    (id, 0, true)
                }
            };

            let database = create_idb_database(ctx, db_id, &name, version)?;
            request.set(js_string!("result"), database.clone(), false, ctx)?;

            if needs_upgrade {
                let onupgradeneeded = request.get(js_string!("onupgradeneeded"), ctx)?;
                if onupgradeneeded.is_callable() {
                    let event = ObjectInitializer::new(ctx)
                        .property(js_string!("type"), js_string!("upgradeneeded"), Attribute::READONLY)
                        .property(js_string!("oldVersion"), old_version, Attribute::READONLY)
                        .property(js_string!("newVersion"), version, Attribute::READONLY)
                        .property(js_string!("target"), request.clone(), Attribute::READONLY)
                        .build();

                    let _ = onupgradeneeded.as_callable().unwrap().call(
                        &JsValue::from(request.clone()),
                        &[JsValue::from(event)],
                        ctx
                    );
                }
            }

            request.set(js_string!("readyState"), js_string!("done"), false, ctx)?;

            // Fire onsuccess
            let onsuccess = request.get(js_string!("onsuccess"), ctx)?;
            if onsuccess.is_callable() {
                let event = ObjectInitializer::new(ctx)
                    .property(js_string!("type"), js_string!("success"), Attribute::READONLY)
                    .property(js_string!("target"), request.clone(), Attribute::READONLY)
                    .build();
                let _ = onsuccess.as_callable().unwrap().call(
                    &JsValue::from(request.clone()),
                    &[JsValue::from(event)],
                    ctx
                );
            }
        }

        Ok(JsValue::from(request))
    });

    let delete_database = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let name = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let origin = get_current_origin();
        let request = create_idb_open_db_request(ctx)?;

        if let Ok(conn) = get_connection() {
            let _ = conn.execute(
                "DELETE FROM idb_databases WHERE origin = ? AND name = ?",
                params![origin, name]
            );
            request.set(js_string!("readyState"), js_string!("done"), false, ctx)?;
        }

        Ok(JsValue::from(request))
    });

    let databases = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        let promise = ObjectInitializer::new(ctx).build();
        let then = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
            let callback = args.get_or_undefined(0);
            if callback.is_callable() {
                let arr = boa_engine::object::builtins::JsArray::new(ctx);
                let _ = callback.as_callable().unwrap().call(
                    &JsValue::undefined(),
                    &[JsValue::from(arr)],
                    ctx
                );
            }
            Ok(JsValue::undefined())
        });
        promise.set(js_string!("then"), then.to_js_function(ctx.realm()), false, ctx)?;
        Ok(JsValue::from(promise))
    });

    let cmp = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let first = args.get_or_undefined(0);
        let second = args.get_or_undefined(1);

        let first_json = serde_json::to_string(&StorageValue::from_js_value(first, ctx)).unwrap_or_default();
        let second_json = serde_json::to_string(&StorageValue::from_js_value(second, ctx)).unwrap_or_default();

        let result = first_json.cmp(&second_json);
        Ok(JsValue::from(match result {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }))
    });

    let indexed_db = ObjectInitializer::new(context)
        .function(open, js_string!("open"), 2)
        .function(delete_database, js_string!("deleteDatabase"), 1)
        .function(databases, js_string!("databases"), 0)
        .function(cmp, js_string!("cmp"), 2)
        .build();

    context.global_object().set(js_string!("indexedDB"), indexed_db, false, context)?;

    // Register global IDB* constructors for instanceof checks and feature detection
    // IDBRequest
    let idb_request_ctor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        create_idb_request(ctx).map(JsValue::from)
    });
    context.global_object().set(
        js_string!("IDBRequest"),
        idb_request_ctor.to_js_function(context.realm()),
        false,
        context
    )?;

    // IDBOpenDBRequest
    let idb_open_db_request_ctor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        create_idb_open_db_request(ctx).map(JsValue::from)
    });
    context.global_object().set(
        js_string!("IDBOpenDBRequest"),
        idb_open_db_request_ctor.to_js_function(context.realm()),
        false,
        context
    )?;

    // IDBDatabase
    let idb_database_ctor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
    });
    context.global_object().set(
        js_string!("IDBDatabase"),
        idb_database_ctor.to_js_function(context.realm()),
        false,
        context
    )?;

    // IDBTransaction
    let idb_transaction_ctor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
    });
    context.global_object().set(
        js_string!("IDBTransaction"),
        idb_transaction_ctor.to_js_function(context.realm()),
        false,
        context
    )?;

    // IDBObjectStore
    let idb_object_store_ctor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
    });
    context.global_object().set(
        js_string!("IDBObjectStore"),
        idb_object_store_ctor.to_js_function(context.realm()),
        false,
        context
    )?;

    // IDBIndex
    let idb_index_ctor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
    });
    context.global_object().set(
        js_string!("IDBIndex"),
        idb_index_ctor.to_js_function(context.realm()),
        false,
        context
    )?;

    // IDBCursor
    let idb_cursor_ctor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
    });
    context.global_object().set(
        js_string!("IDBCursor"),
        idb_cursor_ctor.to_js_function(context.realm()),
        false,
        context
    )?;

    // IDBCursorWithValue
    let idb_cursor_with_value_ctor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
    });
    context.global_object().set(
        js_string!("IDBCursorWithValue"),
        idb_cursor_with_value_ctor.to_js_function(context.realm()),
        false,
        context
    )?;

    // IDBKeyRange
    let idb_key_range_ctor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
    });
    // Add static methods to IDBKeyRange
    let key_range_fn = idb_key_range_ctor.to_js_function(context.realm());
    let only = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let value = args.get_or_undefined(0).clone();
        let range = ObjectInitializer::new(ctx)
            .property(js_string!("lower"), value.clone(), Attribute::READONLY)
            .property(js_string!("upper"), value, Attribute::READONLY)
            .property(js_string!("lowerOpen"), false, Attribute::READONLY)
            .property(js_string!("upperOpen"), false, Attribute::READONLY)
            .build();
        Ok(JsValue::from(range))
    });
    let bound = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let lower = args.get_or_undefined(0).clone();
        let upper = args.get_or_undefined(1).clone();
        let lower_open = args.get(2).map(|v| v.to_boolean()).unwrap_or(false);
        let upper_open = args.get(3).map(|v| v.to_boolean()).unwrap_or(false);
        let range = ObjectInitializer::new(ctx)
            .property(js_string!("lower"), lower, Attribute::READONLY)
            .property(js_string!("upper"), upper, Attribute::READONLY)
            .property(js_string!("lowerOpen"), lower_open, Attribute::READONLY)
            .property(js_string!("upperOpen"), upper_open, Attribute::READONLY)
            .build();
        Ok(JsValue::from(range))
    });
    let lower_bound = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let lower = args.get_or_undefined(0).clone();
        let open = args.get(1).map(|v| v.to_boolean()).unwrap_or(false);
        let range = ObjectInitializer::new(ctx)
            .property(js_string!("lower"), lower, Attribute::READONLY)
            .property(js_string!("upper"), JsValue::undefined(), Attribute::READONLY)
            .property(js_string!("lowerOpen"), open, Attribute::READONLY)
            .property(js_string!("upperOpen"), true, Attribute::READONLY)
            .build();
        Ok(JsValue::from(range))
    });
    let upper_bound = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let upper = args.get_or_undefined(0).clone();
        let open = args.get(1).map(|v| v.to_boolean()).unwrap_or(false);
        let range = ObjectInitializer::new(ctx)
            .property(js_string!("lower"), JsValue::undefined(), Attribute::READONLY)
            .property(js_string!("upper"), upper, Attribute::READONLY)
            .property(js_string!("lowerOpen"), true, Attribute::READONLY)
            .property(js_string!("upperOpen"), open, Attribute::READONLY)
            .build();
        Ok(JsValue::from(range))
    });
    key_range_fn.set(js_string!("only"), only.to_js_function(context.realm()), false, context)?;
    key_range_fn.set(js_string!("bound"), bound.to_js_function(context.realm()), false, context)?;
    key_range_fn.set(js_string!("lowerBound"), lower_bound.to_js_function(context.realm()), false, context)?;
    key_range_fn.set(js_string!("upperBound"), upper_bound.to_js_function(context.realm()), false, context)?;
    context.global_object().set(js_string!("IDBKeyRange"), key_range_fn, false, context)?;

    // IDBVersionChangeEvent
    let idb_version_change_event_ctor = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let event_type = args.get_or_undefined(0).to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default();
        let event = ObjectInitializer::new(ctx)
            .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
            .property(js_string!("oldVersion"), 0, Attribute::READONLY)
            .property(js_string!("newVersion"), 1, Attribute::READONLY)
            .build();
        Ok(JsValue::from(event))
    });
    context.global_object().set(
        js_string!("IDBVersionChangeEvent"),
        idb_version_change_event_ctor.to_js_function(context.realm()),
        false,
        context
    )?;

    // IDBFactory (same as indexedDB but as a constructor for instanceof)
    let idb_factory_ctor = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        Ok(JsValue::from(ObjectInitializer::new(ctx).build()))
    });
    context.global_object().set(
        js_string!("IDBFactory"),
        idb_factory_ctor.to_js_function(context.realm()),
        false,
        context
    )?;

    Ok(())
}

// ============================================================================
// localStorage and sessionStorage with SQLite
// ============================================================================

fn create_storage(context: &mut Context, table_name: &'static str) -> JsResult<JsObject> {
    let origin = get_current_origin();

    let origin_get = origin.clone();
    let get_item = unsafe { NativeFunction::from_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        if let Ok(conn) = get_connection() {
            let sql = format!("SELECT value FROM {} WHERE origin = ? AND key = ?", table_name);
            match conn.query_row(&sql, params![origin_get, key], |row| row.get::<_, String>(0)) {
                Ok(value) => return Ok(JsValue::from(js_string!(value))),
                Err(_) => return Ok(JsValue::null()),
            }
        }
        Ok(JsValue::null())
    }) };

    let origin_set = origin.clone();
    let set_item = unsafe { NativeFunction::from_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let value = args.get_or_undefined(1).to_string(ctx)?.to_std_string_escaped();

        if let Ok(conn) = get_connection() {
            let sql = format!(
                "INSERT OR REPLACE INTO {} (origin, key, value) VALUES (?, ?, ?)",
                table_name
            );
            let _ = conn.execute(&sql, params![origin_set, key, value]);
        }
        Ok(JsValue::undefined())
    }) };

    let origin_remove = origin.clone();
    let remove_item = unsafe { NativeFunction::from_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let key = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

        if let Ok(conn) = get_connection() {
            let sql = format!("DELETE FROM {} WHERE origin = ? AND key = ?", table_name);
            let _ = conn.execute(&sql, params![origin_remove, key]);
        }
        Ok(JsValue::undefined())
    }) };

    let origin_clear = origin.clone();
    let clear = unsafe { NativeFunction::from_closure(move |_: &JsValue, _: &[JsValue], _: &mut Context| {
        if let Ok(conn) = get_connection() {
            let sql = format!("DELETE FROM {} WHERE origin = ?", table_name);
            let _ = conn.execute(&sql, params![origin_clear]);
        }
        Ok(JsValue::undefined())
    }) };

    let origin_key = origin.clone();
    let key = unsafe { NativeFunction::from_closure(move |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
        let index = args.get_or_undefined(0).to_u32(ctx).unwrap_or(0);

        if let Ok(conn) = get_connection() {
            let sql = format!(
                "SELECT key FROM {} WHERE origin = ? ORDER BY key LIMIT 1 OFFSET ?",
                table_name
            );
            match conn.query_row(&sql, params![origin_key, index], |row| row.get::<_, String>(0)) {
                Ok(key) => return Ok(JsValue::from(js_string!(key))),
                Err(_) => return Ok(JsValue::null()),
            }
        }
        Ok(JsValue::null())
    }) };

    let length = if let Ok(conn) = get_connection() {
        let sql = format!("SELECT COUNT(*) FROM {} WHERE origin = ?", table_name);
        conn.query_row(&sql, params![origin], |row| row.get::<_, u32>(0)).unwrap_or(0)
    } else {
        0
    };

    let storage = ObjectInitializer::new(context).build();
    storage.set(js_string!("length"), length, false, context)?;
    storage.set(js_string!("getItem"), get_item.to_js_function(context.realm()), false, context)?;
    storage.set(js_string!("setItem"), set_item.to_js_function(context.realm()), false, context)?;
    storage.set(js_string!("removeItem"), remove_item.to_js_function(context.realm()), false, context)?;
    storage.set(js_string!("clear"), clear.to_js_function(context.realm()), false, context)?;
    storage.set(js_string!("key"), key.to_js_function(context.realm()), false, context)?;

    Ok(storage)
}

pub fn register_web_storage(context: &mut Context) -> JsResult<()> {
    let local_storage = create_storage(context, "local_storage")?;
    let session_storage = create_storage(context, "session_storage")?;

    context.global_object().set(js_string!("localStorage"), local_storage, false, context)?;
    context.global_object().set(js_string!("sessionStorage"), session_storage, false, context)?;

    Ok(())
}

// ============================================================================
// Cache API (basic stub)
// ============================================================================

pub fn register_cache_api(context: &mut Context) -> JsResult<()> {
    let open = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        let cache = ObjectInitializer::new(ctx)
            .property(js_string!("match"), JsValue::undefined(), Attribute::all())
            .build();

        let promise = ObjectInitializer::new(ctx).build();
        let then = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
            let callback = args.get_or_undefined(0);
            if callback.is_callable() {
                let cache = ObjectInitializer::new(ctx).build();
                let _ = callback.as_callable().unwrap().call(&JsValue::undefined(), &[JsValue::from(cache)], ctx);
            }
            Ok(JsValue::undefined())
        });
        promise.set(js_string!("then"), then.to_js_function(ctx.realm()), false, ctx)?;
        Ok(JsValue::from(promise))
    });

    let has = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        let promise = ObjectInitializer::new(ctx).build();
        let then = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
            let callback = args.get_or_undefined(0);
            if callback.is_callable() {
                let _ = callback.as_callable().unwrap().call(&JsValue::undefined(), &[JsValue::from(false)], ctx);
            }
            Ok(JsValue::undefined())
        });
        promise.set(js_string!("then"), then.to_js_function(ctx.realm()), false, ctx)?;
        Ok(JsValue::from(promise))
    });

    let keys = NativeFunction::from_copy_closure(|_: &JsValue, _: &[JsValue], ctx: &mut Context| {
        let promise = ObjectInitializer::new(ctx).build();
        let then = NativeFunction::from_copy_closure(|_: &JsValue, args: &[JsValue], ctx: &mut Context| {
            let callback = args.get_or_undefined(0);
            if callback.is_callable() {
                let arr = boa_engine::object::builtins::JsArray::new(ctx);
                let _ = callback.as_callable().unwrap().call(&JsValue::undefined(), &[JsValue::from(arr)], ctx);
            }
            Ok(JsValue::undefined())
        });
        promise.set(js_string!("then"), then.to_js_function(ctx.realm()), false, ctx)?;
        Ok(JsValue::from(promise))
    });

    let caches = ObjectInitializer::new(context)
        .function(open, js_string!("open"), 1)
        .function(has, js_string!("has"), 1)
        .function(keys, js_string!("keys"), 0)
        .build();

    context.global_object().set(js_string!("caches"), caches, false, context)?;

    Ok(())
}

// ============================================================================
// StorageEvent
// ============================================================================

pub fn register_storage_event(context: &mut Context) -> JsResult<()> {
    let storage_event_constructor = NativeFunction::from_copy_closure(
        |_: &JsValue, args: &[JsValue], ctx: &mut Context| {
            let event_type = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();

            let options = args.get(1);
            let key = options
                .and_then(|o| o.as_object())
                .and_then(|o| o.get(js_string!("key"), ctx).ok())
                .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()));
            let old_value = options
                .and_then(|o| o.as_object())
                .and_then(|o| o.get(js_string!("oldValue"), ctx).ok())
                .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()));
            let new_value = options
                .and_then(|o| o.as_object())
                .and_then(|o| o.get(js_string!("newValue"), ctx).ok())
                .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()));
            let url = options
                .and_then(|o| o.as_object())
                .and_then(|o| o.get(js_string!("url"), ctx).ok())
                .and_then(|v| v.as_string().map(|s| s.to_std_string_escaped()))
                .unwrap_or_default();

            let event = ObjectInitializer::new(ctx)
                .property(js_string!("type"), js_string!(event_type), Attribute::READONLY)
                .property(
                    js_string!("key"),
                    key.map(|k| JsValue::from(js_string!(k))).unwrap_or(JsValue::null()),
                    Attribute::READONLY
                )
                .property(
                    js_string!("oldValue"),
                    old_value.map(|v| JsValue::from(js_string!(v))).unwrap_or(JsValue::null()),
                    Attribute::READONLY
                )
                .property(
                    js_string!("newValue"),
                    new_value.map(|v| JsValue::from(js_string!(v))).unwrap_or(JsValue::null()),
                    Attribute::READONLY
                )
                .property(js_string!("url"), js_string!(url), Attribute::READONLY)
                .property(js_string!("storageArea"), JsValue::null(), Attribute::READONLY)
                .build();

            Ok(JsValue::from(event))
        }
    );

    context.global_object().set(
        js_string!("StorageEvent"),
        storage_event_constructor.to_js_function(context.realm()),
        false,
        context
    )?;

    Ok(())
}

// ============================================================================
// Main Registration
// ============================================================================

pub fn register_all_storage_apis(context: &mut Context) -> JsResult<()> {
    register_idb_key_range(context)?;
    register_indexed_db(context)?;
    register_web_storage(context)?;
    register_cache_api(context)?;
    register_storage_event(context)?;

    Ok(())
}
