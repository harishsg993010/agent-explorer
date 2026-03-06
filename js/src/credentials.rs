//! Credential Management API implementation
//!
//! Implements the full Credential Management APIs including:
//! - Credential (base interface)
//! - CredentialsContainer (navigator.credentials)
//! - PasswordCredential
//! - FederatedCredential
//! - PublicKeyCredential (WebAuthn stub)

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer,
    object::FunctionObjectBuilder, object::builtins::JsArray, property::Attribute,
    Context, JsArgs, JsObject, JsResult, JsValue,
};
use std::cell::RefCell;
use std::rc::Rc;

// =============================================================================
// Promise Helper
// =============================================================================

/// Create a resolved promise
fn create_credential_promise(ctx: &mut Context, value: JsValue) -> JsResult<JsValue> {
    let value_clone = value.clone();
    let then = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            if let Some(callback) = args.first() {
                if callback.is_callable() {
                    let callback_obj = callback.as_callable().unwrap();
                    return callback_obj.call(&JsValue::undefined(), &[value_clone.clone()], ctx);
                }
            }
            Ok(value_clone.clone())
        })
    };

    let catch_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let finally_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(callback) = args.first() {
            if callback.is_callable() {
                let _ = callback.as_callable().unwrap().call(&JsValue::undefined(), &[], ctx);
            }
        }
        Ok(JsValue::undefined())
    });

    let promise = ObjectInitializer::new(ctx)
        .function(then, js_string!("then"), 1)
        .function(catch_fn, js_string!("catch"), 1)
        .function(finally_fn, js_string!("finally"), 1)
        .build();

    Ok(promise.into())
}

// =============================================================================
// Credential (Base Interface)
// =============================================================================

/// Create Credential constructor (abstract base - throws on direct instantiation)
fn create_credential_constructor(context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(boa_engine::JsError::from_opaque(JsValue::from(js_string!(
            "Credential cannot be instantiated directly"
        ))))
    })
}

// =============================================================================
// PasswordCredential
// =============================================================================

/// Create PasswordCredential constructor
fn create_password_credential_constructor(context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        // PasswordCredential(data) or PasswordCredential(form)
        let data = args.get_or_undefined(0);

        let mut id = String::new();
        let mut password = String::new();
        let mut name = String::new();
        let mut icon_url = String::new();

        if let Some(obj) = data.as_object() {
            // Check if it's an HTMLFormElement (has elements property)
            let has_elements = obj.has_property(js_string!("elements"), ctx).unwrap_or(false);

            if has_elements {
                // It's a form element - extract from form fields
                // In a real browser, we'd look for autocomplete="username" and autocomplete="current-password"
                // For our stub, we'll look for common field names
                if let Ok(elements) = obj.get(js_string!("elements"), ctx) {
                    if let Some(elements_obj) = elements.as_object() {
                        // Try to get username/email field
                        for field_name in &["username", "email", "user", "login", "id"] {
                            if let Ok(field) = elements_obj.get(js_string!(*field_name), ctx) {
                                if let Some(field_obj) = field.as_object() {
                                    if let Ok(val) = field_obj.get(js_string!("value"), ctx) {
                                        if !val.is_undefined() && !val.is_null() {
                                            id = val.to_string(ctx)?.to_std_string_escaped();
                                            if !id.is_empty() { break; }
                                        }
                                    }
                                }
                            }
                        }
                        // Try to get password field
                        for field_name in &["password", "pass", "pwd"] {
                            if let Ok(field) = elements_obj.get(js_string!(*field_name), ctx) {
                                if let Some(field_obj) = field.as_object() {
                                    if let Ok(val) = field_obj.get(js_string!("value"), ctx) {
                                        if !val.is_undefined() && !val.is_null() {
                                            password = val.to_string(ctx)?.to_std_string_escaped();
                                            if !password.is_empty() { break; }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // It's a PasswordCredentialData object
                if let Ok(val) = obj.get(js_string!("id"), ctx) {
                    if !val.is_undefined() {
                        id = val.to_string(ctx)?.to_std_string_escaped();
                    }
                }
                if let Ok(val) = obj.get(js_string!("password"), ctx) {
                    if !val.is_undefined() {
                        password = val.to_string(ctx)?.to_std_string_escaped();
                    }
                }
                if let Ok(val) = obj.get(js_string!("name"), ctx) {
                    if !val.is_undefined() {
                        name = val.to_string(ctx)?.to_std_string_escaped();
                    }
                }
                if let Ok(val) = obj.get(js_string!("iconURL"), ctx) {
                    if !val.is_undefined() {
                        icon_url = val.to_string(ctx)?.to_std_string_escaped();
                    }
                }
            }
        }

        let credential = ObjectInitializer::new(ctx)
            // Credential base properties
            .property(js_string!("id"), js_string!(id.as_str()), Attribute::READONLY)
            .property(js_string!("type"), js_string!("password"), Attribute::READONLY)
            // PasswordCredential-specific properties
            .property(js_string!("password"), js_string!(password.as_str()), Attribute::READONLY)
            .property(js_string!("name"), js_string!(name.as_str()), Attribute::READONLY)
            .property(js_string!("iconURL"), js_string!(icon_url.as_str()), Attribute::READONLY)
            .build();

        Ok(JsValue::from(credential))
    })
}

// =============================================================================
// FederatedCredential
// =============================================================================

/// Create FederatedCredential constructor
fn create_federated_credential_constructor(context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, args, ctx| {
        // FederatedCredential(data)
        let data = args.get_or_undefined(0);

        let mut id = String::new();
        let mut provider = String::new();
        let mut protocol = String::new();
        let mut name = String::new();
        let mut icon_url = String::new();

        if let Some(obj) = data.as_object() {
            if let Ok(val) = obj.get(js_string!("id"), ctx) {
                if !val.is_undefined() {
                    id = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = obj.get(js_string!("provider"), ctx) {
                if !val.is_undefined() {
                    provider = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = obj.get(js_string!("protocol"), ctx) {
                if !val.is_undefined() {
                    protocol = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = obj.get(js_string!("name"), ctx) {
                if !val.is_undefined() {
                    name = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
            if let Ok(val) = obj.get(js_string!("iconURL"), ctx) {
                if !val.is_undefined() {
                    icon_url = val.to_string(ctx)?.to_std_string_escaped();
                }
            }
        }

        let credential = ObjectInitializer::new(ctx)
            // Credential base properties
            .property(js_string!("id"), js_string!(id.as_str()), Attribute::READONLY)
            .property(js_string!("type"), js_string!("federated"), Attribute::READONLY)
            // FederatedCredential-specific properties
            .property(js_string!("provider"), js_string!(provider.as_str()), Attribute::READONLY)
            .property(js_string!("protocol"), js_string!(protocol.as_str()), Attribute::READONLY)
            .property(js_string!("name"), js_string!(name.as_str()), Attribute::READONLY)
            .property(js_string!("iconURL"), js_string!(icon_url.as_str()), Attribute::READONLY)
            .build();

        Ok(JsValue::from(credential))
    })
}

// =============================================================================
// PublicKeyCredential (WebAuthn) - Full Implementation
// =============================================================================

/// Create an ArrayBuffer-like object from bytes
fn create_array_buffer_like(ctx: &mut Context, data: &[u8]) -> JsObject {
    let arr = JsArray::new(ctx);
    for (i, byte) in data.iter().enumerate() {
        let _ = arr.set(i, JsValue::from(*byte as i32), false, ctx);
    }
    let obj: JsObject = arr.into();
    let _ = obj.set(js_string!("byteLength"), JsValue::from(data.len() as i32), false, ctx);
    obj
}

/// Base64URL encode bytes (for credential IDs)
fn base64url_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        }
    }
    result
}

/// Generate mock credential ID bytes
fn generate_credential_id() -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut id = Vec::with_capacity(32);
    // Mix timestamp bytes for uniqueness
    for i in 0..32u32 {
        id.push(((timestamp >> (i * 2)) & 0xFF) as u8 ^ ((i.wrapping_mul(17)) & 0xFF) as u8);
    }
    id
}

/// Create AuthenticatorAttestationResponse (for credential creation)
fn create_attestation_response(ctx: &mut Context, rp_id: &str, user_id: &[u8]) -> JsObject {
    // Mock clientDataJSON - would contain challenge, origin, type
    let client_data = format!(
        r#"{{"type":"webauthn.create","challenge":"mock-challenge","origin":"https://{}","crossOrigin":false}}"#,
        rp_id
    );
    let client_data_json = create_array_buffer_like(ctx, client_data.as_bytes());

    // Mock attestationObject - CBOR encoded attestation
    let attestation_bytes: Vec<u8> = vec![
        0xa3, // map(3)
        0x63, 0x66, 0x6d, 0x74, // "fmt"
        0x64, 0x6e, 0x6f, 0x6e, 0x65, // "none"
        0x67, 0x61, 0x74, 0x74, 0x53, 0x74, 0x6d, 0x74, // "attStmt"
        0xa0, // empty map
        0x68, 0x61, 0x75, 0x74, 0x68, 0x44, 0x61, 0x74, 0x61, // "authData"
        0x58, 0x25, // bytes(37) - minimal authenticator data
    ];
    let attestation_object = create_array_buffer_like(ctx, &attestation_bytes);

    // Mock authenticator data (37 bytes minimum)
    let mut auth_data = vec![0u8; 37];
    // RP ID hash (32 bytes) - just zeros for mock
    // Flags byte at position 32: UP=1, UV=1, AT=1, ED=0 = 0x45
    auth_data[32] = 0x45;
    // Sign count (4 bytes big-endian)
    auth_data[33..37].copy_from_slice(&[0, 0, 0, 1]);

    let authenticator_data = create_array_buffer_like(ctx, &auth_data);

    // Mock public key (COSE format - simplified)
    let public_key_bytes: Vec<u8> = vec![
        0xa5, // map(5)
        0x01, 0x02, // kty: EC2
        0x03, 0x26, // alg: ES256 (-7)
        0x20, 0x01, // crv: P-256
        0x21, 0x58, 0x20, // x: bytes(32)
    ];
    let mut full_pk = public_key_bytes.clone();
    full_pk.extend(vec![0u8; 32]); // x coordinate
    full_pk.extend(vec![0x22, 0x58, 0x20]); // y: bytes(32)
    full_pk.extend(vec![0u8; 32]); // y coordinate

    // getTransports method
    let get_transports = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let arr = JsArray::new(ctx);
        arr.set(0_usize, js_string!("internal"), false, ctx)?;
        arr.set(1_usize, js_string!("hybrid"), false, ctx)?;
        Ok(JsValue::from(arr))
    });

    // getPublicKey method - returns ArrayBuffer
    let pk_data = full_pk.clone();
    let get_public_key = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            Ok(JsValue::from(create_array_buffer_like(ctx, &pk_data)))
        })
    };

    // getPublicKeyAlgorithm method
    let get_public_key_algorithm = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::from(-7)) // ES256 (ECDSA with SHA-256)
    });

    // getAuthenticatorData method
    let auth_data_clone = auth_data.clone();
    let get_authenticator_data = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            Ok(JsValue::from(create_array_buffer_like(ctx, &auth_data_clone)))
        })
    };

    ObjectInitializer::new(ctx)
        .property(js_string!("clientDataJSON"), JsValue::from(client_data_json), Attribute::READONLY)
        .property(js_string!("attestationObject"), JsValue::from(attestation_object), Attribute::READONLY)
        .property(js_string!("authenticatorData"), JsValue::from(authenticator_data), Attribute::READONLY)
        .function(get_transports, js_string!("getTransports"), 0)
        .function(get_public_key, js_string!("getPublicKey"), 0)
        .function(get_public_key_algorithm, js_string!("getPublicKeyAlgorithm"), 0)
        .function(get_authenticator_data, js_string!("getAuthenticatorData"), 0)
        .build()
}

/// Create AuthenticatorAssertionResponse (for credential get/assertion)
fn create_assertion_response(ctx: &mut Context, rp_id: &str) -> JsObject {
    // Mock clientDataJSON for assertion
    let client_data = format!(
        r#"{{"type":"webauthn.get","challenge":"mock-challenge","origin":"https://{}","crossOrigin":false}}"#,
        rp_id
    );
    let client_data_json = create_array_buffer_like(ctx, client_data.as_bytes());

    // Mock authenticator data (37 bytes)
    let mut auth_data = vec![0u8; 37];
    auth_data[32] = 0x05; // UP=1, UV=1
    auth_data[33..37].copy_from_slice(&[0, 0, 0, 2]); // sign count
    let authenticator_data = create_array_buffer_like(ctx, &auth_data);

    // Mock signature (DER-encoded ECDSA signature)
    let signature_bytes: Vec<u8> = vec![
        0x30, 0x44, // SEQUENCE, length 68
        0x02, 0x20, // INTEGER, length 32
    ];
    let mut full_sig = signature_bytes;
    full_sig.extend(vec![0x01u8; 32]); // r value
    full_sig.extend(vec![0x02, 0x20]); // INTEGER, length 32
    full_sig.extend(vec![0x02u8; 32]); // s value
    let signature = create_array_buffer_like(ctx, &full_sig);

    // Mock user handle (user.id from registration)
    let user_handle = create_array_buffer_like(ctx, b"mock-user-id");

    ObjectInitializer::new(ctx)
        .property(js_string!("clientDataJSON"), JsValue::from(client_data_json), Attribute::READONLY)
        .property(js_string!("authenticatorData"), JsValue::from(authenticator_data), Attribute::READONLY)
        .property(js_string!("signature"), JsValue::from(signature), Attribute::READONLY)
        .property(js_string!("userHandle"), JsValue::from(user_handle), Attribute::READONLY)
        .build()
}

/// Create PublicKeyCredential constructor (throws - can only be created via navigator.credentials)
fn create_public_key_credential_constructor(context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(boa_engine::JsError::from_opaque(JsValue::from(js_string!(
            "PublicKeyCredential cannot be instantiated directly. Use navigator.credentials.create() instead."
        ))))
    })
}

/// Create a PublicKeyCredential object for registration (attestation)
fn create_public_key_credential_attestation(
    ctx: &mut Context,
    rp_id: &str,
    user_id: &[u8],
    authenticator_attachment: &str,
) -> JsObject {
    let cred_id_bytes = generate_credential_id();
    let cred_id = base64url_encode(&cred_id_bytes);
    let raw_id = create_array_buffer_like(ctx, &cred_id_bytes);
    let response = create_attestation_response(ctx, rp_id, user_id);

    // getClientExtensionResults method
    let get_client_extension_results = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // Return empty object (no extensions)
        let results = ObjectInitializer::new(ctx).build();
        Ok(JsValue::from(results))
    });

    // toJSON method
    let to_json = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let id = obj.get(js_string!("id"), ctx).unwrap_or(JsValue::undefined());
            let cred_type = obj.get(js_string!("type"), ctx).unwrap_or(JsValue::undefined());
            let authenticator_attachment = obj.get(js_string!("authenticatorAttachment"), ctx).unwrap_or(JsValue::undefined());

            // Simplified response JSON
            let response_json = ObjectInitializer::new(ctx)
                .property(js_string!("clientDataJSON"), js_string!("base64url-encoded"), Attribute::all())
                .property(js_string!("attestationObject"), js_string!("base64url-encoded"), Attribute::all())
                .property(js_string!("transports"), JsValue::undefined(), Attribute::all())
                .build();

            // Create extension results before main object
            let extension_results = ObjectInitializer::new(ctx).build();

            let json = ObjectInitializer::new(ctx)
                .property(js_string!("id"), id, Attribute::all())
                .property(js_string!("rawId"), js_string!("base64url-encoded"), Attribute::all())
                .property(js_string!("type"), cred_type, Attribute::all())
                .property(js_string!("authenticatorAttachment"), authenticator_attachment, Attribute::all())
                .property(js_string!("response"), JsValue::from(response_json), Attribute::all())
                .property(js_string!("clientExtensionResults"), JsValue::from(extension_results), Attribute::all())
                .build();
            Ok(JsValue::from(json))
        } else {
            Ok(JsValue::undefined())
        }
    });

    ObjectInitializer::new(ctx)
        // Credential base properties
        .property(js_string!("id"), js_string!(cred_id.as_str()), Attribute::READONLY)
        .property(js_string!("type"), js_string!("public-key"), Attribute::READONLY)
        // PublicKeyCredential-specific properties
        .property(js_string!("rawId"), JsValue::from(raw_id), Attribute::READONLY)
        .property(js_string!("response"), JsValue::from(response), Attribute::READONLY)
        .property(js_string!("authenticatorAttachment"), js_string!(authenticator_attachment), Attribute::READONLY)
        // Methods
        .function(get_client_extension_results, js_string!("getClientExtensionResults"), 0)
        .function(to_json, js_string!("toJSON"), 0)
        .build()
}

/// Create a PublicKeyCredential object for authentication (assertion)
fn create_public_key_credential_assertion(
    ctx: &mut Context,
    cred_id: &str,
    rp_id: &str,
    authenticator_attachment: &str,
) -> JsObject {
    let cred_id_bytes = cred_id.as_bytes();
    let raw_id = create_array_buffer_like(ctx, cred_id_bytes);
    let response = create_assertion_response(ctx, rp_id);

    // getClientExtensionResults method
    let get_client_extension_results = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        let results = ObjectInitializer::new(ctx).build();
        Ok(JsValue::from(results))
    });

    // toJSON method
    let to_json = NativeFunction::from_copy_closure(|this, _args, ctx| {
        if let Some(obj) = this.as_object() {
            let id = obj.get(js_string!("id"), ctx).unwrap_or(JsValue::undefined());
            let cred_type = obj.get(js_string!("type"), ctx).unwrap_or(JsValue::undefined());
            let authenticator_attachment = obj.get(js_string!("authenticatorAttachment"), ctx).unwrap_or(JsValue::undefined());

            let response_json = ObjectInitializer::new(ctx)
                .property(js_string!("clientDataJSON"), js_string!("base64url-encoded"), Attribute::all())
                .property(js_string!("authenticatorData"), js_string!("base64url-encoded"), Attribute::all())
                .property(js_string!("signature"), js_string!("base64url-encoded"), Attribute::all())
                .property(js_string!("userHandle"), js_string!("base64url-encoded"), Attribute::all())
                .build();

            // Create extension results before main object
            let extension_results = ObjectInitializer::new(ctx).build();

            let json = ObjectInitializer::new(ctx)
                .property(js_string!("id"), id, Attribute::all())
                .property(js_string!("rawId"), js_string!("base64url-encoded"), Attribute::all())
                .property(js_string!("type"), cred_type, Attribute::all())
                .property(js_string!("authenticatorAttachment"), authenticator_attachment, Attribute::all())
                .property(js_string!("response"), JsValue::from(response_json), Attribute::all())
                .property(js_string!("clientExtensionResults"), JsValue::from(extension_results), Attribute::all())
                .build();
            Ok(JsValue::from(json))
        } else {
            Ok(JsValue::undefined())
        }
    });

    ObjectInitializer::new(ctx)
        .property(js_string!("id"), js_string!(cred_id), Attribute::READONLY)
        .property(js_string!("type"), js_string!("public-key"), Attribute::READONLY)
        .property(js_string!("rawId"), JsValue::from(raw_id), Attribute::READONLY)
        .property(js_string!("response"), JsValue::from(response), Attribute::READONLY)
        .property(js_string!("authenticatorAttachment"), js_string!(authenticator_attachment), Attribute::READONLY)
        .function(get_client_extension_results, js_string!("getClientExtensionResults"), 0)
        .function(to_json, js_string!("toJSON"), 0)
        .build()
}

/// Parse PublicKeyCredentialCreationOptions
fn parse_creation_options(opts: &JsObject, ctx: &mut Context) -> (String, Vec<u8>, String) {
    let mut rp_id = "localhost".to_string();
    let mut user_id = vec![0u8; 16];
    let mut authenticator_attachment = "platform".to_string();

    // Parse rp.id
    if let Ok(rp) = opts.get(js_string!("rp"), ctx) {
        if let Some(rp_obj) = rp.as_object() {
            if let Ok(id) = rp_obj.get(js_string!("id"), ctx) {
                if !id.is_undefined() {
                    rp_id = id.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or(rp_id);
                }
            }
        }
    }

    // Parse user.id
    if let Ok(user) = opts.get(js_string!("user"), ctx) {
        if let Some(user_obj) = user.as_object() {
            if let Ok(id) = user_obj.get(js_string!("id"), ctx) {
                if let Some(id_obj) = id.as_object() {
                    // Try to read as array
                    if let Ok(len) = id_obj.get(js_string!("length"), ctx) {
                        if let Ok(length) = len.to_index(ctx) {
                            user_id.clear();
                            for i in 0..length.min(64) {
                                if let Ok(byte) = id_obj.get(i, ctx) {
                                    if let Ok(b) = byte.to_i32(ctx) {
                                        user_id.push(b as u8);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Parse authenticatorSelection.authenticatorAttachment
    if let Ok(auth_sel) = opts.get(js_string!("authenticatorSelection"), ctx) {
        if let Some(auth_sel_obj) = auth_sel.as_object() {
            if let Ok(attach) = auth_sel_obj.get(js_string!("authenticatorAttachment"), ctx) {
                if !attach.is_undefined() {
                    authenticator_attachment = attach.to_string(ctx)
                        .map(|s| s.to_std_string_escaped())
                        .unwrap_or(authenticator_attachment);
                }
            }
        }
    }

    (rp_id, user_id, authenticator_attachment)
}

/// Parse PublicKeyCredentialRequestOptions
fn parse_request_options(opts: &JsObject, ctx: &mut Context) -> (String, Option<String>, String) {
    let mut rp_id = "localhost".to_string();
    let mut allow_credential_id: Option<String> = None;
    let authenticator_attachment = "platform".to_string();

    // Parse rpId
    if let Ok(id) = opts.get(js_string!("rpId"), ctx) {
        if !id.is_undefined() {
            rp_id = id.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or(rp_id);
        }
    }

    // Parse allowCredentials[0].id
    if let Ok(allow) = opts.get(js_string!("allowCredentials"), ctx) {
        if let Some(allow_obj) = allow.as_object() {
            if let Ok(first) = allow_obj.get(0_usize, ctx) {
                if let Some(first_obj) = first.as_object() {
                    if let Ok(id) = first_obj.get(js_string!("id"), ctx) {
                        if !id.is_undefined() {
                            allow_credential_id = Some(
                                id.to_string(ctx)
                                    .map(|s| s.to_std_string_escaped())
                                    .unwrap_or_default()
                            );
                        }
                    }
                }
            }
        }
    }

    (rp_id, allow_credential_id, authenticator_attachment)
}

// =============================================================================
// CredentialsContainer (navigator.credentials)
// =============================================================================

/// Create CredentialsContainer object (navigator.credentials)
pub fn create_credentials_container(ctx: &mut Context) -> JsResult<JsObject> {
    // Internal credential store (simulated)
    let credential_store: Rc<RefCell<Vec<JsObject>>> = Rc::new(RefCell::new(Vec::new()));

    // get(options?) - retrieves a credential
    let store_get = credential_store.clone();
    let get = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let options = args.get_or_undefined(0);

            // Check what type of credential is requested
            let mut want_password = false;
            let mut want_federated = false;
            let mut want_public_key = false;
            let mut pk_options: Option<JsObject> = None;
            let mut _mediation = "optional".to_string();

            if let Some(opts) = options.as_object() {
                // Check for password credentials
                if let Ok(pwd) = opts.get(js_string!("password"), ctx) {
                    want_password = pwd.to_boolean();
                }
                // Check for federated credentials
                if let Ok(fed) = opts.get(js_string!("federated"), ctx) {
                    if !fed.is_undefined() && !fed.is_null() {
                        want_federated = true;
                    }
                }
                // Check for publicKey credentials (WebAuthn)
                if let Ok(pk) = opts.get(js_string!("publicKey"), ctx) {
                    if !pk.is_undefined() && !pk.is_null() {
                        want_public_key = true;
                        pk_options = pk.as_object().clone();
                    }
                }
                // Check mediation
                if let Ok(med) = opts.get(js_string!("mediation"), ctx) {
                    if !med.is_undefined() {
                        _mediation = med.to_string(ctx)?.to_std_string_escaped();
                    }
                }
            }

            // Handle WebAuthn assertion (get)
            if want_public_key {
                if let Some(pk_obj) = pk_options {
                    let (rp_id, allow_cred_id, authenticator_attachment) = parse_request_options(&pk_obj, ctx);

                    // Check if we have a stored credential
                    let stored = store_get.borrow();
                    let stored_cred = stored.iter().find(|c| {
                        c.get(js_string!("type"), ctx)
                            .map(|t| t.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default() == "public-key")
                            .unwrap_or(false)
                    });

                    if let Some(stored) = stored_cred {
                        // Get the credential ID from stored credential
                        let cred_id = stored.get(js_string!("id"), ctx)
                            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()).unwrap_or_default())
                            .unwrap_or_else(|_| allow_cred_id.unwrap_or_else(|| "mock-cred-id".to_string()));

                        // Create assertion response
                        let credential = create_public_key_credential_assertion(
                            ctx,
                            &cred_id,
                            &rp_id,
                            &authenticator_attachment,
                        );
                        return create_credential_promise(ctx, JsValue::from(credential));
                    } else if let Some(cred_id) = allow_cred_id {
                        // No stored credential, but allowCredentials specified - create mock assertion
                        let credential = create_public_key_credential_assertion(
                            ctx,
                            &cred_id,
                            &rp_id,
                            &authenticator_attachment,
                        );
                        return create_credential_promise(ctx, JsValue::from(credential));
                    }
                }
                // No credential available
                return create_credential_promise(ctx, JsValue::null());
            }

            // Handle password/federated credentials
            let stored = store_get.borrow();
            let result = if stored.is_empty() {
                JsValue::null()
            } else {
                // Return the first matching credential
                let mut found: Option<JsValue> = None;
                for cred in stored.iter() {
                    if let Ok(cred_type) = cred.get(js_string!("type"), ctx) {
                        let type_str = cred_type.to_string(ctx)?.to_std_string_escaped();
                        if (want_password && type_str == "password") ||
                           (want_federated && type_str == "federated") {
                            found = Some(JsValue::from(cred.clone()));
                            break;
                        }
                    }
                }
                found.unwrap_or(JsValue::null())
            };

            create_credential_promise(ctx, result)
        })
    };

    // store(credential) - stores a credential
    let store_store = credential_store.clone();
    let store = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let credential = args.get_or_undefined(0);

            if let Some(cred_obj) = credential.as_object() {
                store_store.borrow_mut().push(cred_obj.clone());
                // Return the credential wrapped in a promise
                create_credential_promise(ctx, JsValue::from(cred_obj.clone()))
            } else {
                // Return null if not a valid credential
                create_credential_promise(ctx, JsValue::null())
            }
        })
    };

    // create(options) - creates a new credential
    let store_create = credential_store.clone();
    let create = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let options = args.get_or_undefined(0);

            if let Some(opts) = options.as_object() {
                // Check for publicKey (WebAuthn) credential creation
                if let Ok(pk_opts) = opts.get(js_string!("publicKey"), ctx) {
                    if let Some(pk_obj) = pk_opts.as_object() {
                        // Parse creation options
                        let (rp_id, user_id, authenticator_attachment) = parse_creation_options(&pk_obj, ctx);

                        // Create PublicKeyCredential with attestation response
                        let credential = create_public_key_credential_attestation(
                            ctx,
                            &rp_id,
                            &user_id,
                            &authenticator_attachment,
                        );

                        // Store the credential for later retrieval
                        store_create.borrow_mut().push(credential.clone());

                        return create_credential_promise(ctx, JsValue::from(credential));
                    }
                }

                // Check for password credential creation (not typically done via create)
                if let Ok(pwd_opts) = opts.get(js_string!("password"), ctx) {
                    if pwd_opts.as_object().is_some() {
                        // Password credentials are created via the constructor, not create()
                        return create_credential_promise(ctx, JsValue::null());
                    }
                }
            }

            // Return null if no valid options
            create_credential_promise(ctx, JsValue::null())
        })
    };

    // preventSilentAccess() - prevents automatic sign-in
    let prevent_silent_access = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        // In a real browser, this would set a flag to prevent silent access
        // For our stub, just return a resolved promise
        create_credential_promise(ctx, JsValue::undefined())
    });

    let container = ObjectInitializer::new(ctx)
        .function(get, js_string!("get"), 1)
        .function(store, js_string!("store"), 1)
        .function(create, js_string!("create"), 1)
        .function(prevent_silent_access, js_string!("preventSilentAccess"), 0)
        .build();

    Ok(container)
}

// =============================================================================
// CredentialsContainer Constructor
// =============================================================================

/// Create CredentialsContainer constructor (throws - singleton via navigator.credentials)
fn create_credentials_container_constructor(context: &mut Context) -> NativeFunction {
    NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Err(boa_engine::JsError::from_opaque(JsValue::from(js_string!(
            "CredentialsContainer cannot be instantiated directly. Use navigator.credentials instead."
        ))))
    })
}

// =============================================================================
// Registration
// =============================================================================

/// Register all Credential Management API constructors and objects
pub fn register_credential_apis(context: &mut Context) -> JsResult<()> {
    // Register Credential constructor (abstract base)
    let credential_native = create_credential_constructor(context);
    let credential_ctor = FunctionObjectBuilder::new(context.realm(), credential_native)
        .name(js_string!("Credential"))
        .length(0)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("Credential"), credential_ctor, Attribute::all())?;

    // Register PasswordCredential constructor
    let password_credential_native = create_password_credential_constructor(context);
    let password_credential_ctor = FunctionObjectBuilder::new(context.realm(), password_credential_native)
        .name(js_string!("PasswordCredential"))
        .length(1)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("PasswordCredential"), password_credential_ctor, Attribute::all())?;

    // Register FederatedCredential constructor
    let federated_credential_native = create_federated_credential_constructor(context);
    let federated_credential_ctor = FunctionObjectBuilder::new(context.realm(), federated_credential_native)
        .name(js_string!("FederatedCredential"))
        .length(1)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("FederatedCredential"), federated_credential_ctor, Attribute::all())?;

    // Register PublicKeyCredential constructor
    let public_key_credential_native = create_public_key_credential_constructor(context);
    let public_key_credential_ctor = FunctionObjectBuilder::new(context.realm(), public_key_credential_native)
        .name(js_string!("PublicKeyCredential"))
        .length(0)
        .constructor(true)
        .build();

    // Add static methods to PublicKeyCredential
    // isConditionalMediationAvailable() - returns Promise<boolean>
    let is_conditional_mediation_available = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        create_credential_promise(ctx, JsValue::from(true))
    }).to_js_function(context.realm());
    public_key_credential_ctor.set(js_string!("isConditionalMediationAvailable"), is_conditional_mediation_available, false, context)?;

    // isUserVerifyingPlatformAuthenticatorAvailable() - returns Promise<boolean>
    let is_uvpa_available = NativeFunction::from_copy_closure(|_this, _args, ctx| {
        create_credential_promise(ctx, JsValue::from(true))
    }).to_js_function(context.realm());
    public_key_credential_ctor.set(js_string!("isUserVerifyingPlatformAuthenticatorAvailable"), is_uvpa_available, false, context)?;

    context.register_global_property(js_string!("PublicKeyCredential"), public_key_credential_ctor, Attribute::all())?;

    // Register CredentialsContainer constructor (throws)
    let credentials_container_native = create_credentials_container_constructor(context);
    let credentials_container_ctor = FunctionObjectBuilder::new(context.realm(), credentials_container_native)
        .name(js_string!("CredentialsContainer"))
        .length(0)
        .constructor(true)
        .build();
    context.register_global_property(js_string!("CredentialsContainer"), credentials_container_ctor, Attribute::all())?;

    // Add navigator.credentials
    let credentials = create_credentials_container(context)?;
    let navigator = context.global_object().get(js_string!("navigator"), context)?;
    if let Some(nav_obj) = navigator.as_object() {
        nav_obj.set(js_string!("credentials"), JsValue::from(credentials), false, context)?;
    }

    Ok(())
}
