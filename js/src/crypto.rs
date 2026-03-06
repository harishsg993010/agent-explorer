//! Web Crypto API - crypto.subtle and crypto.getRandomValues
//!
//! Provides cryptographic operations including hashing, encryption, signing, and key management.

use boa_engine::{
    js_string, native_function::NativeFunction, object::ObjectInitializer, property::Attribute,
    object::builtins::JsArray, Context, JsArgs, JsNativeError, JsObject, JsResult, JsValue,
};
use std::collections::HashMap;
use std::sync::Mutex;

use sha1::Sha1;
use sha2::{Sha256, Sha384, Sha512, Digest};
use hmac::{Hmac, Mac};
use aes_gcm::{Aes128Gcm, Aes256Gcm, KeyInit, aead::Aead};
use aes_gcm::aead::generic_array::GenericArray;
use rand::RngCore;
use uuid::Uuid;

type HmacSha1 = Hmac<Sha1>;
type HmacSha256 = Hmac<Sha256>;
type HmacSha384 = Hmac<Sha384>;
type HmacSha512 = Hmac<Sha512>;

lazy_static::lazy_static! {
    /// Storage for CryptoKey objects by ID
    static ref CRYPTO_KEY_STORAGE: Mutex<HashMap<u32, CryptoKeyData>> = Mutex::new(HashMap::new());
    static ref NEXT_KEY_ID: Mutex<u32> = Mutex::new(1);
}

#[derive(Clone)]
struct CryptoKeyData {
    key_type: String,        // "secret", "public", "private"
    extractable: bool,
    algorithm: String,       // "AES-GCM", "AES-CBC", "HMAC", "PBKDF2", etc.
    usages: Vec<String>,     // ["encrypt", "decrypt", "sign", "verify", etc.]
    raw_key: Vec<u8>,        // The actual key material
    length: usize,           // Key length in bits
}

/// Register all Web Crypto APIs
pub fn register_all_crypto_apis(ctx: &mut Context) -> JsResult<()> {
    let crypto = create_crypto_object(ctx)?;
    ctx.register_global_property(js_string!("crypto"), crypto, Attribute::READONLY)?;
    Ok(())
}

/// Create the main crypto object
fn create_crypto_object(ctx: &mut Context) -> JsResult<JsObject> {
    let subtle = create_subtle_crypto(ctx)?;

    // crypto.getRandomValues(typedArray)
    let get_random_values = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let typed_array = args.get_or_undefined(0);

        if typed_array.is_undefined() || typed_array.is_null() {
            return Err(JsNativeError::typ()
                .with_message("getRandomValues requires a typed array")
                .into());
        }

        // Get the array object
        if let Some(obj) = typed_array.as_object() {
            // Try to get length property
            let length = obj.get(js_string!("length"), ctx)?;
            if let Some(len) = length.as_number() {
                let len = len as usize;

                // Generate random bytes
                let mut rng = rand::thread_rng();
                let mut bytes = vec![0u8; len];
                rng.fill_bytes(&mut bytes);

                // Set values on the array
                for (i, byte) in bytes.iter().enumerate() {
                    obj.set(i, JsValue::from(*byte as i32), false, ctx)?;
                }
            }

            return Ok(typed_array.clone());
        }

        Ok(typed_array.clone())
    });

    // crypto.randomUUID()
    let random_uuid = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        let uuid = Uuid::new_v4().to_string();
        Ok(JsValue::from(js_string!(uuid)))
    });

    let crypto = ObjectInitializer::new(ctx)
        .property(js_string!("subtle"), subtle, Attribute::READONLY)
        .function(get_random_values, js_string!("getRandomValues"), 1)
        .function(random_uuid, js_string!("randomUUID"), 0)
        .build();

    Ok(crypto)
}

/// Create the SubtleCrypto object with all methods
fn create_subtle_crypto(ctx: &mut Context) -> JsResult<JsObject> {
    // digest(algorithm, data) -> Promise<ArrayBuffer>
    let digest = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let algorithm = get_algorithm_name(args.get_or_undefined(0), ctx)?;
        let data = get_buffer_data(args.get_or_undefined(1), ctx)?;

        let hash_result = match algorithm.to_uppercase().as_str() {
            "SHA-1" => {
                let mut hasher = Sha1::new();
                hasher.update(&data);
                hasher.finalize().to_vec()
            }
            "SHA-256" => {
                let mut hasher = Sha256::new();
                hasher.update(&data);
                hasher.finalize().to_vec()
            }
            "SHA-384" => {
                let mut hasher = Sha384::new();
                hasher.update(&data);
                hasher.finalize().to_vec()
            }
            "SHA-512" => {
                let mut hasher = Sha512::new();
                hasher.update(&data);
                hasher.finalize().to_vec()
            }
            _ => {
                return Err(JsNativeError::typ()
                    .with_message(format!("Unsupported algorithm: {}", algorithm))
                    .into());
            }
        };

        create_resolved_promise_with_array_buffer(ctx, hash_result)
    });

    // generateKey(algorithm, extractable, keyUsages) -> Promise<CryptoKey | CryptoKeyPair>
    let generate_key = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let algorithm = args.get_or_undefined(0);
        let extractable = args.get_or_undefined(1).to_boolean();
        let key_usages = get_key_usages(args.get_or_undefined(2), ctx)?;

        let (algo_name, key_length) = parse_algorithm_params(algorithm, ctx)?;

        match algo_name.to_uppercase().as_str() {
            "AES-GCM" | "AES-CBC" | "AES-CTR" => {
                let length = key_length.unwrap_or(256);
                let mut key_bytes = vec![0u8; length / 8];
                rand::thread_rng().fill_bytes(&mut key_bytes);

                let key_id = store_crypto_key(CryptoKeyData {
                    key_type: "secret".to_string(),
                    extractable,
                    algorithm: algo_name.clone(),
                    usages: key_usages,
                    raw_key: key_bytes,
                    length,
                });

                let crypto_key = create_crypto_key_object(ctx, key_id, "secret", &algo_name, extractable)?;
                create_resolved_promise(ctx, crypto_key.into())
            }
            "HMAC" => {
                let length = key_length.unwrap_or(256);
                let mut key_bytes = vec![0u8; length / 8];
                rand::thread_rng().fill_bytes(&mut key_bytes);

                let key_id = store_crypto_key(CryptoKeyData {
                    key_type: "secret".to_string(),
                    extractable,
                    algorithm: algo_name.clone(),
                    usages: key_usages,
                    raw_key: key_bytes,
                    length,
                });

                let crypto_key = create_crypto_key_object(ctx, key_id, "secret", &algo_name, extractable)?;
                create_resolved_promise(ctx, crypto_key.into())
            }
            _ => {
                Err(JsNativeError::typ()
                    .with_message(format!("Unsupported algorithm for generateKey: {}", algo_name))
                    .into())
            }
        }
    });

    // importKey(format, keyData, algorithm, extractable, keyUsages) -> Promise<CryptoKey>
    let import_key = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let format = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let key_data = args.get_or_undefined(1);
        let algorithm = args.get_or_undefined(2);
        let extractable = args.get_or_undefined(3).to_boolean();
        let key_usages = get_key_usages(args.get_or_undefined(4), ctx)?;

        let (algo_name, _) = parse_algorithm_params(algorithm, ctx)?;

        let raw_key = match format.as_str() {
            "raw" => get_buffer_data(key_data, ctx)?,
            "jwk" => {
                // Parse JWK format
                if let Some(obj) = key_data.as_object() {
                    let k = obj.get(js_string!("k"), ctx)?;
                    if !k.is_undefined() {
                        // Base64url decode
                        let k_str = k.to_string(ctx)?.to_std_string_escaped();
                        base64url_decode(&k_str)?
                    } else {
                        return Err(JsNativeError::typ()
                            .with_message("JWK missing 'k' property")
                            .into());
                    }
                } else {
                    return Err(JsNativeError::typ()
                        .with_message("JWK must be an object")
                        .into());
                }
            }
            _ => {
                return Err(JsNativeError::typ()
                    .with_message(format!("Unsupported key format: {}", format))
                    .into());
            }
        };

        let key_id = store_crypto_key(CryptoKeyData {
            key_type: "secret".to_string(),
            extractable,
            algorithm: algo_name.clone(),
            usages: key_usages,
            raw_key: raw_key.clone(),
            length: raw_key.len() * 8,
        });

        let crypto_key = create_crypto_key_object(ctx, key_id, "secret", &algo_name, extractable)?;
        create_resolved_promise(ctx, crypto_key.into())
    });

    // exportKey(format, key) -> Promise<ArrayBuffer | JsonWebKey>
    let export_key = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let format = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let key = args.get_or_undefined(1);

        let key_id = get_key_id_from_object(key, ctx)?;
        let key_data = get_crypto_key(key_id)?;

        if !key_data.extractable {
            return Err(JsNativeError::typ()
                .with_message("Key is not extractable")
                .into());
        }

        match format.as_str() {
            "raw" => {
                create_resolved_promise_with_array_buffer(ctx, key_data.raw_key.clone())
            }
            "jwk" => {
                let jwk = ObjectInitializer::new(ctx)
                    .property(js_string!("kty"), js_string!("oct"), Attribute::all())
                    .property(js_string!("k"), js_string!(base64url_encode(&key_data.raw_key)), Attribute::all())
                    .property(js_string!("alg"), js_string!(key_data.algorithm.clone()), Attribute::all())
                    .property(js_string!("ext"), JsValue::from(key_data.extractable), Attribute::all())
                    .build();
                create_resolved_promise(ctx, jwk.into())
            }
            _ => {
                Err(JsNativeError::typ()
                    .with_message(format!("Unsupported export format: {}", format))
                    .into())
            }
        }
    });

    // encrypt(algorithm, key, data) -> Promise<ArrayBuffer>
    let encrypt = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let algorithm = args.get_or_undefined(0);
        let key = args.get_or_undefined(1);
        let data = get_buffer_data(args.get_or_undefined(2), ctx)?;

        let key_id = get_key_id_from_object(key, ctx)?;
        let key_data = get_crypto_key(key_id)?;

        if !key_data.usages.contains(&"encrypt".to_string()) {
            return Err(JsNativeError::typ()
                .with_message("Key does not support encryption")
                .into());
        }

        let (algo_name, iv) = parse_encrypt_params(algorithm, ctx)?;

        match algo_name.to_uppercase().as_str() {
            "AES-GCM" => {
                let iv = iv.ok_or_else(|| {
                    JsNativeError::typ().with_message("AES-GCM requires an iv parameter")
                })?;

                let ciphertext = match key_data.raw_key.len() {
                    16 => {
                        let cipher = Aes128Gcm::new(GenericArray::from_slice(&key_data.raw_key));
                        let nonce = GenericArray::from_slice(&iv);
                        cipher.encrypt(nonce, data.as_ref())
                            .map_err(|_| JsNativeError::typ().with_message("Encryption failed"))?
                    }
                    32 => {
                        let cipher = Aes256Gcm::new(GenericArray::from_slice(&key_data.raw_key));
                        let nonce = GenericArray::from_slice(&iv);
                        cipher.encrypt(nonce, data.as_ref())
                            .map_err(|_| JsNativeError::typ().with_message("Encryption failed"))?
                    }
                    _ => {
                        return Err(JsNativeError::typ()
                            .with_message("Invalid AES key length")
                            .into());
                    }
                };

                create_resolved_promise_with_array_buffer(ctx, ciphertext)
            }
            _ => {
                Err(JsNativeError::typ()
                    .with_message(format!("Unsupported encryption algorithm: {}", algo_name))
                    .into())
            }
        }
    });

    // decrypt(algorithm, key, data) -> Promise<ArrayBuffer>
    let decrypt = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let algorithm = args.get_or_undefined(0);
        let key = args.get_or_undefined(1);
        let data = get_buffer_data(args.get_or_undefined(2), ctx)?;

        let key_id = get_key_id_from_object(key, ctx)?;
        let key_data = get_crypto_key(key_id)?;

        if !key_data.usages.contains(&"decrypt".to_string()) {
            return Err(JsNativeError::typ()
                .with_message("Key does not support decryption")
                .into());
        }

        let (algo_name, iv) = parse_encrypt_params(algorithm, ctx)?;

        match algo_name.to_uppercase().as_str() {
            "AES-GCM" => {
                let iv = iv.ok_or_else(|| {
                    JsNativeError::typ().with_message("AES-GCM requires an iv parameter")
                })?;

                let plaintext = match key_data.raw_key.len() {
                    16 => {
                        let cipher = Aes128Gcm::new(GenericArray::from_slice(&key_data.raw_key));
                        let nonce = GenericArray::from_slice(&iv);
                        cipher.decrypt(nonce, data.as_ref())
                            .map_err(|_| JsNativeError::typ().with_message("Decryption failed"))?
                    }
                    32 => {
                        let cipher = Aes256Gcm::new(GenericArray::from_slice(&key_data.raw_key));
                        let nonce = GenericArray::from_slice(&iv);
                        cipher.decrypt(nonce, data.as_ref())
                            .map_err(|_| JsNativeError::typ().with_message("Decryption failed"))?
                    }
                    _ => {
                        return Err(JsNativeError::typ()
                            .with_message("Invalid AES key length")
                            .into());
                    }
                };

                create_resolved_promise_with_array_buffer(ctx, plaintext)
            }
            _ => {
                Err(JsNativeError::typ()
                    .with_message(format!("Unsupported decryption algorithm: {}", algo_name))
                    .into())
            }
        }
    });

    // sign(algorithm, key, data) -> Promise<ArrayBuffer>
    let sign = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let algorithm = args.get_or_undefined(0);
        let key = args.get_or_undefined(1);
        let data = get_buffer_data(args.get_or_undefined(2), ctx)?;

        let key_id = get_key_id_from_object(key, ctx)?;
        let key_data = get_crypto_key(key_id)?;

        if !key_data.usages.contains(&"sign".to_string()) {
            return Err(JsNativeError::typ()
                .with_message("Key does not support signing")
                .into());
        }

        let (algo_name, hash) = parse_sign_params(algorithm, ctx)?;

        match algo_name.to_uppercase().as_str() {
            "HMAC" => {
                let signature = match hash.to_uppercase().as_str() {
                    "SHA-256" => {
                        let mut mac = <HmacSha256 as Mac>::new_from_slice(&key_data.raw_key)
                            .map_err(|_| JsNativeError::typ().with_message("Invalid HMAC key"))?;
                        mac.update(&data);
                        mac.finalize().into_bytes().to_vec()
                    }
                    "SHA-384" => {
                        let mut mac = <HmacSha384 as Mac>::new_from_slice(&key_data.raw_key)
                            .map_err(|_| JsNativeError::typ().with_message("Invalid HMAC key"))?;
                        mac.update(&data);
                        mac.finalize().into_bytes().to_vec()
                    }
                    "SHA-512" => {
                        let mut mac = <HmacSha512 as Mac>::new_from_slice(&key_data.raw_key)
                            .map_err(|_| JsNativeError::typ().with_message("Invalid HMAC key"))?;
                        mac.update(&data);
                        mac.finalize().into_bytes().to_vec()
                    }
                    "SHA-1" => {
                        let mut mac = <HmacSha1 as Mac>::new_from_slice(&key_data.raw_key)
                            .map_err(|_| JsNativeError::typ().with_message("Invalid HMAC key"))?;
                        mac.update(&data);
                        mac.finalize().into_bytes().to_vec()
                    }
                    _ => {
                        return Err(JsNativeError::typ()
                            .with_message(format!("Unsupported hash for HMAC: {}", hash))
                            .into());
                    }
                };

                create_resolved_promise_with_array_buffer(ctx, signature)
            }
            _ => {
                Err(JsNativeError::typ()
                    .with_message(format!("Unsupported signing algorithm: {}", algo_name))
                    .into())
            }
        }
    });

    // verify(algorithm, key, signature, data) -> Promise<boolean>
    let verify = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let algorithm = args.get_or_undefined(0);
        let key = args.get_or_undefined(1);
        let signature = get_buffer_data(args.get_or_undefined(2), ctx)?;
        let data = get_buffer_data(args.get_or_undefined(3), ctx)?;

        let key_id = get_key_id_from_object(key, ctx)?;
        let key_data = get_crypto_key(key_id)?;

        if !key_data.usages.contains(&"verify".to_string()) {
            return Err(JsNativeError::typ()
                .with_message("Key does not support verification")
                .into());
        }

        let (algo_name, hash) = parse_sign_params(algorithm, ctx)?;

        match algo_name.to_uppercase().as_str() {
            "HMAC" => {
                let is_valid = match hash.to_uppercase().as_str() {
                    "SHA-256" => {
                        let mut mac = <HmacSha256 as Mac>::new_from_slice(&key_data.raw_key)
                            .map_err(|_| JsNativeError::typ().with_message("Invalid HMAC key"))?;
                        mac.update(&data);
                        mac.verify_slice(&signature).is_ok()
                    }
                    "SHA-384" => {
                        let mut mac = <HmacSha384 as Mac>::new_from_slice(&key_data.raw_key)
                            .map_err(|_| JsNativeError::typ().with_message("Invalid HMAC key"))?;
                        mac.update(&data);
                        mac.verify_slice(&signature).is_ok()
                    }
                    "SHA-512" => {
                        let mut mac = <HmacSha512 as Mac>::new_from_slice(&key_data.raw_key)
                            .map_err(|_| JsNativeError::typ().with_message("Invalid HMAC key"))?;
                        mac.update(&data);
                        mac.verify_slice(&signature).is_ok()
                    }
                    "SHA-1" => {
                        let mut mac = <HmacSha1 as Mac>::new_from_slice(&key_data.raw_key)
                            .map_err(|_| JsNativeError::typ().with_message("Invalid HMAC key"))?;
                        mac.update(&data);
                        mac.verify_slice(&signature).is_ok()
                    }
                    _ => {
                        return Err(JsNativeError::typ()
                            .with_message(format!("Unsupported hash for HMAC: {}", hash))
                            .into());
                    }
                };

                create_resolved_promise(ctx, JsValue::from(is_valid))
            }
            _ => {
                Err(JsNativeError::typ()
                    .with_message(format!("Unsupported verification algorithm: {}", algo_name))
                    .into())
            }
        }
    });

    // deriveBits(algorithm, baseKey, length) -> Promise<ArrayBuffer>
    let derive_bits = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let algorithm = args.get_or_undefined(0);
        let base_key = args.get_or_undefined(1);
        let length = args.get_or_undefined(2).to_number(ctx)? as usize;

        let key_id = get_key_id_from_object(base_key, ctx)?;
        let key_data = get_crypto_key(key_id)?;

        if !key_data.usages.contains(&"deriveBits".to_string()) && !key_data.usages.contains(&"deriveKey".to_string()) {
            return Err(JsNativeError::typ()
                .with_message("Key does not support key derivation")
                .into());
        }

        let algo_params = parse_derive_params(algorithm, ctx)?;

        match algo_params.name.to_uppercase().as_str() {
            "PBKDF2" => {
                let salt = algo_params.salt.ok_or_else(|| {
                    JsNativeError::typ().with_message("PBKDF2 requires salt")
                })?;
                let iterations = algo_params.iterations.ok_or_else(|| {
                    JsNativeError::typ().with_message("PBKDF2 requires iterations")
                })?;
                let hash = algo_params.hash.unwrap_or_else(|| "SHA-256".to_string());

                let mut derived = vec![0u8; length / 8];

                match hash.to_uppercase().as_str() {
                    "SHA-256" => {
                        pbkdf2::pbkdf2_hmac::<Sha256>(&key_data.raw_key, &salt, iterations, &mut derived);
                    }
                    "SHA-384" => {
                        pbkdf2::pbkdf2_hmac::<Sha384>(&key_data.raw_key, &salt, iterations, &mut derived);
                    }
                    "SHA-512" => {
                        pbkdf2::pbkdf2_hmac::<Sha512>(&key_data.raw_key, &salt, iterations, &mut derived);
                    }
                    "SHA-1" => {
                        pbkdf2::pbkdf2_hmac::<Sha1>(&key_data.raw_key, &salt, iterations, &mut derived);
                    }
                    _ => {
                        return Err(JsNativeError::typ()
                            .with_message(format!("Unsupported hash for PBKDF2: {}", hash))
                            .into());
                    }
                }

                create_resolved_promise_with_array_buffer(ctx, derived)
            }
            "HKDF" => {
                let salt = algo_params.salt.unwrap_or_default();
                let info = algo_params.info.unwrap_or_default();
                let hash = algo_params.hash.unwrap_or_else(|| "SHA-256".to_string());

                let mut derived = vec![0u8; length / 8];

                match hash.to_uppercase().as_str() {
                    "SHA-256" => {
                        let hk = hkdf::Hkdf::<Sha256>::new(Some(&salt), &key_data.raw_key);
                        hk.expand(&info, &mut derived)
                            .map_err(|_| JsNativeError::typ().with_message("HKDF expansion failed"))?;
                    }
                    "SHA-384" => {
                        let hk = hkdf::Hkdf::<Sha384>::new(Some(&salt), &key_data.raw_key);
                        hk.expand(&info, &mut derived)
                            .map_err(|_| JsNativeError::typ().with_message("HKDF expansion failed"))?;
                    }
                    "SHA-512" => {
                        let hk = hkdf::Hkdf::<Sha512>::new(Some(&salt), &key_data.raw_key);
                        hk.expand(&info, &mut derived)
                            .map_err(|_| JsNativeError::typ().with_message("HKDF expansion failed"))?;
                    }
                    _ => {
                        return Err(JsNativeError::typ()
                            .with_message(format!("Unsupported hash for HKDF: {}", hash))
                            .into());
                    }
                }

                create_resolved_promise_with_array_buffer(ctx, derived)
            }
            _ => {
                Err(JsNativeError::typ()
                    .with_message(format!("Unsupported derivation algorithm: {}", algo_params.name))
                    .into())
            }
        }
    });

    // deriveKey(algorithm, baseKey, derivedKeyType, extractable, keyUsages) -> Promise<CryptoKey>
    let derive_key = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let algorithm = args.get_or_undefined(0);
        let base_key = args.get_or_undefined(1);
        let derived_key_type = args.get_or_undefined(2);
        let extractable = args.get_or_undefined(3).to_boolean();
        let key_usages = get_key_usages(args.get_or_undefined(4), ctx)?;

        let key_id = get_key_id_from_object(base_key, ctx)?;
        let key_data = get_crypto_key(key_id)?;

        if !key_data.usages.contains(&"deriveKey".to_string()) {
            return Err(JsNativeError::typ()
                .with_message("Key does not support key derivation")
                .into());
        }

        let algo_params = parse_derive_params(algorithm, ctx)?;
        let (derived_algo, derived_length) = parse_algorithm_params(derived_key_type, ctx)?;
        let length = derived_length.unwrap_or(256);

        let derived_bytes = match algo_params.name.to_uppercase().as_str() {
            "PBKDF2" => {
                let salt = algo_params.salt.ok_or_else(|| {
                    JsNativeError::typ().with_message("PBKDF2 requires salt")
                })?;
                let iterations = algo_params.iterations.ok_or_else(|| {
                    JsNativeError::typ().with_message("PBKDF2 requires iterations")
                })?;
                let hash = algo_params.hash.unwrap_or_else(|| "SHA-256".to_string());

                let mut derived = vec![0u8; length / 8];

                match hash.to_uppercase().as_str() {
                    "SHA-256" => pbkdf2::pbkdf2_hmac::<Sha256>(&key_data.raw_key, &salt, iterations, &mut derived),
                    "SHA-384" => pbkdf2::pbkdf2_hmac::<Sha384>(&key_data.raw_key, &salt, iterations, &mut derived),
                    "SHA-512" => pbkdf2::pbkdf2_hmac::<Sha512>(&key_data.raw_key, &salt, iterations, &mut derived),
                    "SHA-1" => pbkdf2::pbkdf2_hmac::<Sha1>(&key_data.raw_key, &salt, iterations, &mut derived),
                    _ => {
                        return Err(JsNativeError::typ()
                            .with_message(format!("Unsupported hash for PBKDF2: {}", hash))
                            .into());
                    }
                }
                derived
            }
            "HKDF" => {
                let salt = algo_params.salt.unwrap_or_default();
                let info = algo_params.info.unwrap_or_default();
                let hash = algo_params.hash.unwrap_or_else(|| "SHA-256".to_string());

                let mut derived = vec![0u8; length / 8];

                match hash.to_uppercase().as_str() {
                    "SHA-256" => {
                        let hk = hkdf::Hkdf::<Sha256>::new(Some(&salt), &key_data.raw_key);
                        hk.expand(&info, &mut derived)
                            .map_err(|_| JsNativeError::typ().with_message("HKDF expansion failed"))?;
                    }
                    "SHA-384" => {
                        let hk = hkdf::Hkdf::<Sha384>::new(Some(&salt), &key_data.raw_key);
                        hk.expand(&info, &mut derived)
                            .map_err(|_| JsNativeError::typ().with_message("HKDF expansion failed"))?;
                    }
                    "SHA-512" => {
                        let hk = hkdf::Hkdf::<Sha512>::new(Some(&salt), &key_data.raw_key);
                        hk.expand(&info, &mut derived)
                            .map_err(|_| JsNativeError::typ().with_message("HKDF expansion failed"))?;
                    }
                    _ => {
                        return Err(JsNativeError::typ()
                            .with_message(format!("Unsupported hash for HKDF: {}", hash))
                            .into());
                    }
                }
                derived
            }
            _ => {
                return Err(JsNativeError::typ()
                    .with_message(format!("Unsupported derivation algorithm: {}", algo_params.name))
                    .into());
            }
        };

        let new_key_id = store_crypto_key(CryptoKeyData {
            key_type: "secret".to_string(),
            extractable,
            algorithm: derived_algo.clone(),
            usages: key_usages,
            raw_key: derived_bytes,
            length,
        });

        let crypto_key = create_crypto_key_object(ctx, new_key_id, "secret", &derived_algo, extractable)?;
        create_resolved_promise(ctx, crypto_key.into())
    });

    // wrapKey(format, key, wrappingKey, wrapAlgo) -> Promise<ArrayBuffer>
    let wrap_key = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let format = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let key = args.get_or_undefined(1);
        let wrapping_key = args.get_or_undefined(2);
        let wrap_algo = args.get_or_undefined(3);

        let key_id = get_key_id_from_object(key, ctx)?;
        let key_data = get_crypto_key(key_id)?;

        if !key_data.extractable {
            return Err(JsNativeError::typ()
                .with_message("Key is not extractable")
                .into());
        }

        let wrapping_key_id = get_key_id_from_object(wrapping_key, ctx)?;
        let wrapping_key_data = get_crypto_key(wrapping_key_id)?;

        if !wrapping_key_data.usages.contains(&"wrapKey".to_string()) {
            return Err(JsNativeError::typ()
                .with_message("Wrapping key does not support wrapKey")
                .into());
        }

        let key_bytes = match format.as_str() {
            "raw" => key_data.raw_key.clone(),
            _ => {
                return Err(JsNativeError::typ()
                    .with_message(format!("Unsupported wrap format: {}", format))
                    .into());
            }
        };

        let (algo_name, iv) = parse_encrypt_params(wrap_algo, ctx)?;

        match algo_name.to_uppercase().as_str() {
            "AES-GCM" => {
                let iv = iv.ok_or_else(|| {
                    JsNativeError::typ().with_message("AES-GCM requires an iv parameter")
                })?;

                let wrapped = match wrapping_key_data.raw_key.len() {
                    16 => {
                        let cipher = Aes128Gcm::new(GenericArray::from_slice(&wrapping_key_data.raw_key));
                        let nonce = GenericArray::from_slice(&iv);
                        cipher.encrypt(nonce, key_bytes.as_ref())
                            .map_err(|_| JsNativeError::typ().with_message("Key wrapping failed"))?
                    }
                    32 => {
                        let cipher = Aes256Gcm::new(GenericArray::from_slice(&wrapping_key_data.raw_key));
                        let nonce = GenericArray::from_slice(&iv);
                        cipher.encrypt(nonce, key_bytes.as_ref())
                            .map_err(|_| JsNativeError::typ().with_message("Key wrapping failed"))?
                    }
                    _ => {
                        return Err(JsNativeError::typ()
                            .with_message("Invalid AES key length")
                            .into());
                    }
                };

                create_resolved_promise_with_array_buffer(ctx, wrapped)
            }
            _ => {
                Err(JsNativeError::typ()
                    .with_message(format!("Unsupported wrap algorithm: {}", algo_name))
                    .into())
            }
        }
    });

    // unwrapKey(format, wrappedKey, unwrappingKey, unwrapAlgo, unwrappedKeyAlgo, extractable, keyUsages) -> Promise<CryptoKey>
    let unwrap_key = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let format = args.get_or_undefined(0).to_string(ctx)?.to_std_string_escaped();
        let wrapped_key = get_buffer_data(args.get_or_undefined(1), ctx)?;
        let unwrapping_key = args.get_or_undefined(2);
        let unwrap_algo = args.get_or_undefined(3);
        let unwrapped_key_algo = args.get_or_undefined(4);
        let extractable = args.get_or_undefined(5).to_boolean();
        let key_usages = get_key_usages(args.get_or_undefined(6), ctx)?;

        let unwrapping_key_id = get_key_id_from_object(unwrapping_key, ctx)?;
        let unwrapping_key_data = get_crypto_key(unwrapping_key_id)?;

        if !unwrapping_key_data.usages.contains(&"unwrapKey".to_string()) {
            return Err(JsNativeError::typ()
                .with_message("Unwrapping key does not support unwrapKey")
                .into());
        }

        let (unwrap_algo_name, iv) = parse_encrypt_params(unwrap_algo, ctx)?;
        let (key_algo_name, _) = parse_algorithm_params(unwrapped_key_algo, ctx)?;

        let unwrapped_bytes = match unwrap_algo_name.to_uppercase().as_str() {
            "AES-GCM" => {
                let iv = iv.ok_or_else(|| {
                    JsNativeError::typ().with_message("AES-GCM requires an iv parameter")
                })?;

                match unwrapping_key_data.raw_key.len() {
                    16 => {
                        let cipher = Aes128Gcm::new(GenericArray::from_slice(&unwrapping_key_data.raw_key));
                        let nonce = GenericArray::from_slice(&iv);
                        cipher.decrypt(nonce, wrapped_key.as_ref())
                            .map_err(|_| JsNativeError::typ().with_message("Key unwrapping failed"))?
                    }
                    32 => {
                        let cipher = Aes256Gcm::new(GenericArray::from_slice(&unwrapping_key_data.raw_key));
                        let nonce = GenericArray::from_slice(&iv);
                        cipher.decrypt(nonce, wrapped_key.as_ref())
                            .map_err(|_| JsNativeError::typ().with_message("Key unwrapping failed"))?
                    }
                    _ => {
                        return Err(JsNativeError::typ()
                            .with_message("Invalid AES key length")
                            .into());
                    }
                }
            }
            _ => {
                return Err(JsNativeError::typ()
                    .with_message(format!("Unsupported unwrap algorithm: {}", unwrap_algo_name))
                    .into());
            }
        };

        if format != "raw" {
            return Err(JsNativeError::typ()
                .with_message(format!("Unsupported unwrap format: {}", format))
                .into());
        }

        let new_key_id = store_crypto_key(CryptoKeyData {
            key_type: "secret".to_string(),
            extractable,
            algorithm: key_algo_name.clone(),
            usages: key_usages,
            raw_key: unwrapped_bytes.clone(),
            length: unwrapped_bytes.len() * 8,
        });

        let crypto_key = create_crypto_key_object(ctx, new_key_id, "secret", &key_algo_name, extractable)?;
        create_resolved_promise(ctx, crypto_key.into())
    });

    let subtle = ObjectInitializer::new(ctx)
        .function(digest, js_string!("digest"), 2)
        .function(generate_key, js_string!("generateKey"), 3)
        .function(import_key, js_string!("importKey"), 5)
        .function(export_key, js_string!("exportKey"), 2)
        .function(encrypt, js_string!("encrypt"), 3)
        .function(decrypt, js_string!("decrypt"), 3)
        .function(sign, js_string!("sign"), 3)
        .function(verify, js_string!("verify"), 4)
        .function(derive_bits, js_string!("deriveBits"), 3)
        .function(derive_key, js_string!("deriveKey"), 5)
        .function(wrap_key, js_string!("wrapKey"), 4)
        .function(unwrap_key, js_string!("unwrapKey"), 7)
        .build();

    Ok(subtle)
}

// Helper functions

fn get_algorithm_name(value: &JsValue, ctx: &mut Context) -> JsResult<String> {
    if value.is_string() {
        return Ok(value.to_string(ctx)?.to_std_string_escaped());
    }

    if let Some(obj) = value.as_object() {
        let name = obj.get(js_string!("name"), ctx)?;
        if !name.is_undefined() {
            return Ok(name.to_string(ctx)?.to_std_string_escaped());
        }
    }

    Err(JsNativeError::typ()
        .with_message("Algorithm must be a string or object with 'name' property")
        .into())
}

fn parse_algorithm_params(value: &JsValue, ctx: &mut Context) -> JsResult<(String, Option<usize>)> {
    if value.is_string() {
        return Ok((value.to_string(ctx)?.to_std_string_escaped(), None));
    }

    if let Some(obj) = value.as_object() {
        let name = obj.get(js_string!("name"), ctx)?;
        let name_str = if name.is_undefined() {
            return Err(JsNativeError::typ()
                .with_message("Algorithm object requires 'name' property")
                .into());
        } else {
            name.to_string(ctx)?.to_std_string_escaped()
        };

        let length = obj.get(js_string!("length"), ctx)?;
        let length_val = if length.is_undefined() {
            None
        } else {
            Some(length.to_number(ctx)? as usize)
        };

        return Ok((name_str, length_val));
    }

    Err(JsNativeError::typ()
        .with_message("Algorithm must be a string or object")
        .into())
}

fn parse_encrypt_params(value: &JsValue, ctx: &mut Context) -> JsResult<(String, Option<Vec<u8>>)> {
    if let Some(obj) = value.as_object() {
        let name = obj.get(js_string!("name"), ctx)?;
        let name_str = name.to_string(ctx)?.to_std_string_escaped();

        let iv = obj.get(js_string!("iv"), ctx)?;
        let iv_bytes = if iv.is_undefined() {
            None
        } else {
            Some(get_buffer_data(&iv, ctx)?)
        };

        return Ok((name_str, iv_bytes));
    }

    Err(JsNativeError::typ()
        .with_message("Encryption algorithm must be an object with 'name' and 'iv'")
        .into())
}

fn parse_sign_params(value: &JsValue, ctx: &mut Context) -> JsResult<(String, String)> {
    if value.is_string() {
        return Ok((value.to_string(ctx)?.to_std_string_escaped(), "SHA-256".to_string()));
    }

    if let Some(obj) = value.as_object() {
        let name = obj.get(js_string!("name"), ctx)?;
        let name_str = name.to_string(ctx)?.to_std_string_escaped();

        let hash = obj.get(js_string!("hash"), ctx)?;
        let hash_str = if hash.is_undefined() {
            "SHA-256".to_string()
        } else if hash.is_string() {
            hash.to_string(ctx)?.to_std_string_escaped()
        } else if let Some(hash_obj) = hash.as_object() {
            let hash_name = hash_obj.get(js_string!("name"), ctx)?;
            hash_name.to_string(ctx)?.to_std_string_escaped()
        } else {
            "SHA-256".to_string()
        };

        return Ok((name_str, hash_str));
    }

    Err(JsNativeError::typ()
        .with_message("Sign algorithm must be a string or object")
        .into())
}

struct DeriveParams {
    name: String,
    salt: Option<Vec<u8>>,
    iterations: Option<u32>,
    hash: Option<String>,
    info: Option<Vec<u8>>,
}

fn parse_derive_params(value: &JsValue, ctx: &mut Context) -> JsResult<DeriveParams> {
    if let Some(obj) = value.as_object() {
        let name = obj.get(js_string!("name"), ctx)?;
        let name_str = name.to_string(ctx)?.to_std_string_escaped();

        let salt = obj.get(js_string!("salt"), ctx)?;
        let salt_bytes = if salt.is_undefined() {
            None
        } else {
            Some(get_buffer_data(&salt, ctx)?)
        };

        let iterations = obj.get(js_string!("iterations"), ctx)?;
        let iterations_val = if iterations.is_undefined() {
            None
        } else {
            Some(iterations.to_number(ctx)? as u32)
        };

        let hash = obj.get(js_string!("hash"), ctx)?;
        let hash_str = if hash.is_undefined() {
            None
        } else if hash.is_string() {
            Some(hash.to_string(ctx)?.to_std_string_escaped())
        } else if let Some(hash_obj) = hash.as_object() {
            let hash_name = hash_obj.get(js_string!("name"), ctx)?;
            Some(hash_name.to_string(ctx)?.to_std_string_escaped())
        } else {
            None
        };

        let info = obj.get(js_string!("info"), ctx)?;
        let info_bytes = if info.is_undefined() {
            None
        } else {
            Some(get_buffer_data(&info, ctx)?)
        };

        return Ok(DeriveParams {
            name: name_str,
            salt: salt_bytes,
            iterations: iterations_val,
            hash: hash_str,
            info: info_bytes,
        });
    }

    Err(JsNativeError::typ()
        .with_message("Derive algorithm must be an object")
        .into())
}

fn get_buffer_data(value: &JsValue, ctx: &mut Context) -> JsResult<Vec<u8>> {
    if value.is_string() {
        return Ok(value.to_string(ctx)?.to_std_string_escaped().into_bytes());
    }

    if let Some(obj) = value.as_object() {
        // Check for ArrayBuffer-like object
        let length = obj.get(js_string!("length"), ctx)?;
        if let Some(len) = length.as_number() {
            let len = len as usize;
            let mut bytes = Vec::with_capacity(len);

            // Check if it's a Uint8Array or similar
            let byte_length = obj.get(js_string!("byteLength"), ctx)?;
            if !byte_length.is_undefined() {
                // It's a TypedArray
                for i in 0..len {
                    let val = obj.get(i, ctx)?;
                    if let Some(n) = val.as_number() {
                        bytes.push(n as u8);
                    }
                }
            } else {
                // It's a regular array
                for i in 0..len {
                    let val = obj.get(i, ctx)?;
                    if let Some(n) = val.as_number() {
                        bytes.push(n as u8);
                    }
                }
            }

            return Ok(bytes);
        }

        // Check for buffer property (ArrayBufferView)
        let buffer = obj.get(js_string!("buffer"), ctx)?;
        if !buffer.is_undefined() {
            return get_buffer_data(&buffer, ctx);
        }
    }

    Ok(Vec::new())
}

fn get_key_usages(value: &JsValue, ctx: &mut Context) -> JsResult<Vec<String>> {
    let mut usages = Vec::new();

    if let Some(obj) = value.as_object() {
        let length = obj.get(js_string!("length"), ctx)?;
        if let Some(len) = length.as_number() {
            for i in 0..(len as usize) {
                let usage = obj.get(i, ctx)?;
                if !usage.is_undefined() {
                    usages.push(usage.to_string(ctx)?.to_std_string_escaped());
                }
            }
        }
    }

    Ok(usages)
}

fn store_crypto_key(key_data: CryptoKeyData) -> u32 {
    let mut id = NEXT_KEY_ID.lock().unwrap();
    let key_id = *id;
    *id += 1;

    let mut storage = CRYPTO_KEY_STORAGE.lock().unwrap();
    storage.insert(key_id, key_data);

    key_id
}

fn get_crypto_key(key_id: u32) -> JsResult<CryptoKeyData> {
    let storage = CRYPTO_KEY_STORAGE.lock().unwrap();
    storage.get(&key_id).cloned().ok_or_else(|| {
        JsNativeError::typ()
            .with_message("Invalid CryptoKey")
            .into()
    })
}

fn get_key_id_from_object(value: &JsValue, ctx: &mut Context) -> JsResult<u32> {
    if let Some(obj) = value.as_object() {
        let id = obj.get(js_string!("_keyId"), ctx)?;
        if let Some(n) = id.as_number() {
            return Ok(n as u32);
        }
    }

    Err(JsNativeError::typ()
        .with_message("Invalid CryptoKey object")
        .into())
}

fn create_crypto_key_object(
    ctx: &mut Context,
    key_id: u32,
    key_type: &str,
    algorithm: &str,
    extractable: bool,
) -> JsResult<JsObject> {
    let algo_obj = ObjectInitializer::new(ctx)
        .property(js_string!("name"), js_string!(algorithm.to_string()), Attribute::READONLY)
        .build();

    let key = ObjectInitializer::new(ctx)
        .property(js_string!("type"), js_string!(key_type.to_string()), Attribute::READONLY)
        .property(js_string!("extractable"), JsValue::from(extractable), Attribute::READONLY)
        .property(js_string!("algorithm"), algo_obj, Attribute::READONLY)
        .property(js_string!("_keyId"), JsValue::from(key_id), Attribute::empty())
        .build();

    Ok(key)
}

fn create_resolved_promise(ctx: &mut Context, value: JsValue) -> JsResult<JsValue> {
    let then = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let callback = args.get_or_undefined(0);
            if let Some(func) = callback.as_callable() {
                func.call(&JsValue::undefined(), &[value.clone()], ctx)
            } else {
                Ok(value.clone())
            }
        })
    };

    let catch_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let finally_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if let Some(func) = callback.as_callable() {
            let _ = func.call(&JsValue::undefined(), &[], ctx);
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

fn create_resolved_promise_with_array_buffer(ctx: &mut Context, data: Vec<u8>) -> JsResult<JsValue> {
    // Create a Uint8Array-like object
    let array = JsArray::new(ctx);
    for (i, byte) in data.iter().enumerate() {
        array.set(i, JsValue::from(*byte as i32), false, ctx)?;
    }

    let array_obj: JsObject = array.into();
    array_obj.set(js_string!("byteLength"), JsValue::from(data.len() as i32), false, ctx)?;

    let data_clone = data.clone();
    let then = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let callback = args.get_or_undefined(0);
            if let Some(func) = callback.as_callable() {
                // Recreate the array for the callback
                let arr = JsArray::new(ctx);
                for (i, byte) in data_clone.iter().enumerate() {
                    arr.set(i, JsValue::from(*byte as i32), false, ctx)?;
                }
                let arr_obj: JsObject = arr.into();
                arr_obj.set(js_string!("byteLength"), JsValue::from(data_clone.len() as i32), false, ctx)?;
                func.call(&JsValue::undefined(), &[arr_obj.into()], ctx)
            } else {
                let arr = JsArray::new(ctx);
                for (i, byte) in data_clone.iter().enumerate() {
                    arr.set(i, JsValue::from(*byte as i32), false, ctx)?;
                }
                Ok(arr.into())
            }
        })
    };

    let catch_fn = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        Ok(JsValue::undefined())
    });

    let finally_fn = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let callback = args.get_or_undefined(0);
        if let Some(func) = callback.as_callable() {
            let _ = func.call(&JsValue::undefined(), &[], ctx);
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

fn base64url_encode(data: &[u8]) -> String {
    let mut result = String::new();
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    for chunk in data.chunks(3) {
        let mut buf = [0u8; 4];
        let len = chunk.len();

        buf[0] = chunk[0] >> 2;
        buf[1] = ((chunk[0] & 0x03) << 4) | if len > 1 { chunk[1] >> 4 } else { 0 };
        buf[2] = if len > 1 { ((chunk[1] & 0x0f) << 2) | if len > 2 { chunk[2] >> 6 } else { 0 } } else { 64 };
        buf[3] = if len > 2 { chunk[2] & 0x3f } else { 64 };

        for (i, &b) in buf.iter().enumerate() {
            if b < 64 {
                result.push(ALPHABET[b as usize] as char);
            } else if i < len + 1 {
                result.push(ALPHABET[b as usize] as char);
            }
        }
    }

    result
}

fn base64url_decode(data: &str) -> JsResult<Vec<u8>> {
    let mut result = Vec::new();
    let chars: Vec<u8> = data.bytes().collect();

    let decode_char = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    };

    for chunk in chars.chunks(4) {
        let mut buf = [0u8; 4];
        let mut valid = 0;

        for (i, &c) in chunk.iter().enumerate() {
            if let Some(v) = decode_char(c) {
                buf[i] = v;
                valid = i + 1;
            }
        }

        if valid >= 2 {
            result.push((buf[0] << 2) | (buf[1] >> 4));
        }
        if valid >= 3 {
            result.push((buf[1] << 4) | (buf[2] >> 2));
        }
        if valid >= 4 {
            result.push((buf[2] << 6) | buf[3]);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64url_encode() {
        assert_eq!(base64url_encode(b"hello"), "aGVsbG8");
        assert_eq!(base64url_encode(b"world"), "d29ybGQ");
    }

    #[test]
    fn test_base64url_decode() {
        assert_eq!(base64url_decode("aGVsbG8").unwrap(), b"hello");
        assert_eq!(base64url_decode("d29ybGQ").unwrap(), b"world");
    }

    #[test]
    fn test_sha256_digest() {
        let mut hasher = Sha256::new();
        hasher.update(b"hello world");
        let result = hasher.finalize();
        assert_eq!(result.len(), 32);
    }
}
