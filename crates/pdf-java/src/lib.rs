//! Java JNI bindings for the XFA PDF engine.
//!
//! Exposes `PdfDocument` and `PdfUtils` to Java via JNI.
//!
//! Operations supported:
//! - Round 1: open/close, page count, text extraction, save, merge, PDF/A validation
//! - Round 2: form fields (get/set), annotations (get/add), redaction, encryption

use std::sync::{Arc, Mutex, MutexGuard};

use jni::objects::{JByteArray, JClass, JObject, JObjectArray, JString};
use jni::sys::{jboolean, jdouble, jint, jlong, jobject, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;

use lopdf::{
    Document as LopdfDocument, EncryptionVersion, Object as LopdfObject,
    Permissions as LopdfPermissions, StringFormat,
};

use pdf_annot::builder::{add_annotation_to_page, AnnotRect, AnnotationBuilder};
use pdf_annot::Annotation;
use pdf_compliance::{validate_pdfa as compliance_validate_pdfa, PdfALevel, Severity};
use pdf_engine::{PdfDocument, RenderOptions, ThumbnailOptions};
use pdf_forms::{
    apply_field_value, parse_acroform, FieldType, FieldValue, WriteOutcome, WriteValue,
    WritebackError,
};
use pdf_manip::encrypt::remove_encryption;
use pdf_manip::pages;
use pdf_redact::{search_and_redact, RedactSearchOptions};

// ---------------------------------------------------------------------------
// Document handle — combines read-only engine with mutable lopdf state.
// ---------------------------------------------------------------------------

struct JniDocument {
    /// Read-only engine for text, render, metadata operations.
    engine: PdfDocument,
    /// Original raw bytes, used to lazily initialize lopdf and for decrypt.
    raw_bytes: Vec<u8>,
    /// Mutable lopdf document, initialized on first write operation.
    lopdf: Mutex<Option<LopdfDocument>>,
}

fn to_handle(engine: PdfDocument, raw_bytes: Vec<u8>) -> jlong {
    let boxed = Box::new(Arc::new(JniDocument {
        engine,
        raw_bytes,
        lopdf: Mutex::new(None),
    }));
    Box::into_raw(boxed) as jlong
}

/// Recover an Arc<JniDocument> reference from a Java handle.
///
/// # Safety
/// The handle must have been created by `to_handle` and not yet freed.
unsafe fn from_handle(handle: jlong) -> &'static Arc<JniDocument> {
    &*(handle as *const Arc<JniDocument>)
}

/// Free a JniDocument handle.
///
/// # Safety
/// The handle must have been created by `to_handle` and must only be freed once.
unsafe fn free_handle(handle: jlong) {
    let _ = Box::from_raw(handle as *mut Arc<JniDocument>);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn throw_pdf_exception(env: &mut JNIEnv, msg: &str) {
    let _ = env.throw_new("com/pdfluent/PdfluentException", msg);
}

fn throw_pdf_parse_exception(env: &mut JNIEnv, msg: &str) {
    let _ = env.throw_new("com/pdfluent/PdfluentParseException", msg);
}

/// Construct and throw a PDFluent exception subclass that carries a canonical
/// C8 error code via the `(String message, String code)` constructor.
///
/// Falls back to the no-code constructor via `env.throw_new` if anything
/// goes wrong while building the typed throwable, so a missing code never
/// degrades into a silently-swallowed JNI error.
fn throw_pdf_exception_with_code(env: &mut JNIEnv, class_name: &str, msg: &str, code: &str) {
    // Try the (String, String) constructor introduced in 0.2 first.
    let class = match env.find_class(class_name) {
        Ok(c) => c,
        Err(_) => {
            let _ = env.throw_new(class_name, msg);
            return;
        }
    };
    let msg_obj = match env.new_string(msg) {
        Ok(s) => s,
        Err(_) => {
            let _ = env.throw_new(class_name, msg);
            return;
        }
    };
    let code_obj = match env.new_string(code) {
        Ok(s) => s,
        Err(_) => {
            let _ = env.throw_new(class_name, msg);
            return;
        }
    };
    let throwable = env.new_object(
        &class,
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[
            jni::objects::JValue::Object(&msg_obj),
            jni::objects::JValue::Object(&code_obj),
        ],
    );
    match throwable {
        Ok(obj) => {
            // SAFETY: obj is a freshly-constructed Throwable subclass; the
            // JVM accepts the raw JObject and takes ownership.
            let _ = env.throw(jni::objects::JThrowable::from(obj));
        }
        Err(_) => {
            let _ = env.throw_new(class_name, msg);
        }
    }
}

/// Lazily initialize lopdf from the document's raw bytes.
fn ensure_lopdf(
    handle: &'static Arc<JniDocument>,
) -> Result<MutexGuard<'static, Option<LopdfDocument>>, String> {
    let mut guard = handle.lopdf.lock().unwrap();
    if guard.is_none() {
        match LopdfDocument::load_mem(&handle.raw_bytes) {
            Ok(doc) => *guard = Some(doc),
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(guard)
}

/// Convert a Java String[] (JObjectArray) to a Vec<String>.
fn read_string_array(
    env: &mut JNIEnv,
    arr: &JObjectArray,
) -> Result<Vec<String>, jni::errors::Error> {
    let len = env.get_array_length(arr)?;
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let elem = env.get_object_array_element(arr, i)?;
        let jstr = JString::from(elem);
        let s: String = env.get_string(&jstr)?.into();
        out.push(s);
    }
    Ok(out)
}

/// Build a Java String[] from a Rust slice of strings.
fn new_string_array<'a>(
    env: &mut JNIEnv<'a>,
    items: &[String],
) -> Result<JObject<'a>, jni::errors::Error> {
    let null = JObject::null();
    let arr = env.new_object_array(items.len() as i32, "java/lang/String", &null)?;
    for (i, item) in items.iter().enumerate() {
        let s = env.new_string(item.as_str())?;
        env.set_object_array_element(&arr, i as i32, &s)?;
    }
    Ok(JObject::from(arr))
}

/// Parse a PDF/A level string ("1b", "2b", "3b", …).  Defaults to 2b.
fn parse_pdfa_level(level: &str) -> PdfALevel {
    match level.to_lowercase().as_str() {
        "1a" | "pdfa-1a" => PdfALevel::A1a,
        "1b" | "pdfa-1b" => PdfALevel::A1b,
        "2a" | "pdfa-2a" => PdfALevel::A2a,
        "2u" | "pdfa-2u" => PdfALevel::A2u,
        "3a" | "pdfa-3a" => PdfALevel::A3a,
        "3b" | "pdfa-3b" => PdfALevel::A3b,
        "3u" | "pdfa-3u" => PdfALevel::A3u,
        "4" | "pdfa-4" => PdfALevel::A4,
        _ => PdfALevel::A2b,
    }
}

// ---------------------------------------------------------------------------
// JNI exports — com.xfa.pdf.PdfDocument
// ---------------------------------------------------------------------------

/// `native long nativeOpen(byte[] data)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeOpen(
    mut env: JNIEnv,
    _class: JClass,
    data: JByteArray,
) -> jlong {
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("failed to read byte array: {e}"));
            return 0;
        }
    };

    match PdfDocument::open(bytes.clone()) {
        Ok(doc) => to_handle(doc, bytes),
        Err(e) => {
            throw_pdf_parse_exception(&mut env, &e.to_string());
            0
        }
    }
}

/// `native long nativeOpenWithPassword(byte[] data, String password)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeOpenWithPassword(
    mut env: JNIEnv,
    _class: JClass,
    data: JByteArray,
    password: JString,
) -> jlong {
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("failed to read byte array: {e}"));
            return 0;
        }
    };

    let pw: String = match env.get_string(&password) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("failed to read password: {e}"));
            return 0;
        }
    };

    match PdfDocument::open_with_password(bytes.clone(), &pw) {
        Ok(doc) => to_handle(doc, bytes),
        Err(e) => {
            throw_pdf_parse_exception(&mut env, &e.to_string());
            0
        }
    }
}

/// `native void nativeClose(long handle)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeClose(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) {
    if handle != 0 {
        unsafe { free_handle(handle) };
    }
}

/// `native int nativePageCount(long handle)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativePageCount(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    if handle == 0 {
        return 0;
    }
    let doc = unsafe { from_handle(handle) };
    doc.engine.page_count() as jint
}

/// `native double nativePageWidth(long handle, int pageIndex)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativePageWidth(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    page_index: jint,
) -> jdouble {
    if handle == 0 {
        return 0.0;
    }
    let doc = unsafe { from_handle(handle) };
    match doc.engine.page_geometry(page_index as usize) {
        Ok(geom) => {
            let (w, _) = geom.effective_dimensions();
            w
        }
        Err(e) => {
            throw_pdf_exception(&mut env, &e.to_string());
            0.0
        }
    }
}

/// `native double nativePageHeight(long handle, int pageIndex)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativePageHeight(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    page_index: jint,
) -> jdouble {
    if handle == 0 {
        return 0.0;
    }
    let doc = unsafe { from_handle(handle) };
    match doc.engine.page_geometry(page_index as usize) {
        Ok(geom) => {
            let (_, h) = geom.effective_dimensions();
            h
        }
        Err(e) => {
            throw_pdf_exception(&mut env, &e.to_string());
            0.0
        }
    }
}

/// `native int nativePageRotation(long handle, int pageIndex)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativePageRotation(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    page_index: jint,
) -> jint {
    if handle == 0 {
        return 0;
    }
    let doc = unsafe { from_handle(handle) };
    match doc.engine.page_geometry(page_index as usize) {
        Ok(geom) => geom.rotation.degrees() as jint,
        Err(e) => {
            throw_pdf_exception(&mut env, &e.to_string());
            0
        }
    }
}

/// `native String nativeExtractText(long handle, int pageIndex)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeExtractText<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    page_index: jint,
) -> jobject {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return JObject::null().into_raw();
    }
    let doc = unsafe { from_handle(handle) };
    match doc.engine.extract_text(page_index as usize) {
        Ok(text) => match env.new_string(&text) {
            Ok(s) => s.into_raw(),
            Err(e) => {
                throw_pdf_exception(&mut env, &format!("string conversion error: {e}"));
                JObject::null().into_raw()
            }
        },
        Err(e) => {
            throw_pdf_exception(&mut env, &e.to_string());
            JObject::null().into_raw()
        }
    }
}

/// `native byte[] nativeRenderPage(long handle, int pageIndex, double dpi)`
///
/// Returns `[width:4 bytes BE][height:4 bytes BE][RGBA pixels…]`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeRenderPage<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    page_index: jint,
    dpi: jdouble,
) -> jobject {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return JObject::null().into_raw();
    }
    let doc = unsafe { from_handle(handle) };
    let options = RenderOptions {
        dpi,
        ..Default::default()
    };
    match doc.engine.render_page(page_index as usize, &options) {
        Ok(rendered) => {
            let w_bytes = (rendered.width as i32).to_be_bytes();
            let h_bytes = (rendered.height as i32).to_be_bytes();
            let mut buf = Vec::with_capacity(8 + rendered.pixels.len());
            buf.extend_from_slice(&w_bytes);
            buf.extend_from_slice(&h_bytes);
            buf.extend_from_slice(&rendered.pixels);
            match env.byte_array_from_slice(&buf) {
                Ok(arr) => arr.into_raw(),
                Err(e) => {
                    throw_pdf_exception(&mut env, &format!("array creation error: {e}"));
                    JObject::null().into_raw()
                }
            }
        }
        Err(e) => {
            throw_pdf_exception(&mut env, &e.to_string());
            JObject::null().into_raw()
        }
    }
}

/// `native byte[] nativeRenderThumbnail(long handle, int pageIndex, int maxDimension)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeRenderThumbnail<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    page_index: jint,
    max_dimension: jint,
) -> jobject {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return JObject::null().into_raw();
    }
    let doc = unsafe { from_handle(handle) };
    let options = ThumbnailOptions {
        max_dimension: max_dimension as u32,
    };
    match doc.engine.thumbnail(page_index as usize, &options) {
        Ok(rendered) => {
            let w_bytes = (rendered.width as i32).to_be_bytes();
            let h_bytes = (rendered.height as i32).to_be_bytes();
            let mut buf = Vec::with_capacity(8 + rendered.pixels.len());
            buf.extend_from_slice(&w_bytes);
            buf.extend_from_slice(&h_bytes);
            buf.extend_from_slice(&rendered.pixels);
            match env.byte_array_from_slice(&buf) {
                Ok(arr) => arr.into_raw(),
                Err(e) => {
                    throw_pdf_exception(&mut env, &format!("array creation error: {e}"));
                    JObject::null().into_raw()
                }
            }
        }
        Err(e) => {
            throw_pdf_exception(&mut env, &e.to_string());
            JObject::null().into_raw()
        }
    }
}

/// `native String nativeGetMetadata(long handle, String key)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeGetMetadata<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    key: JString<'a>,
) -> jobject {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return JObject::null().into_raw();
    }
    let doc = unsafe { from_handle(handle) };
    let key_str: String = match env.get_string(&key) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("key string error: {e}"));
            return JObject::null().into_raw();
        }
    };

    let info = doc.engine.info();
    let value = match key_str.as_str() {
        "Title" | "title" => info.title,
        "Author" | "author" => info.author,
        "Subject" | "subject" => info.subject,
        "Keywords" | "keywords" => info.keywords,
        "Creator" | "creator" => info.creator,
        "Producer" | "producer" => info.producer,
        _ => None,
    };

    match value {
        Some(v) => match env.new_string(&v) {
            Ok(s) => s.into_raw(),
            Err(e) => {
                throw_pdf_exception(&mut env, &format!("string conversion error: {e}"));
                JObject::null().into_raw()
            }
        },
        None => JObject::null().into_raw(),
    }
}

/// `native int nativeBookmarkCount(long handle)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeBookmarkCount(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    if handle == 0 {
        return 0;
    }
    let doc = unsafe { from_handle(handle) };
    doc.engine.bookmarks().len() as jint
}

/// `native int[] nativeSearchText(long handle, String query)`
///
/// Returns 0-based page indices of pages containing the query.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeSearchText<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    query: JString<'a>,
) -> jobject {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return JObject::null().into_raw();
    }
    let doc = unsafe { from_handle(handle) };
    let query_str: String = match env.get_string(&query) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("query string error: {e}"));
            return JObject::null().into_raw();
        }
    };

    let pages = doc.engine.search_text(&query_str);
    let int_pages: Vec<i32> = pages.into_iter().map(|p| p as i32).collect();

    match env.new_int_array(int_pages.len() as i32) {
        Ok(arr) => {
            let _ = env.set_int_array_region(&arr, 0, &int_pages);
            arr.into_raw()
        }
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("array creation error: {e}"));
            JObject::null().into_raw()
        }
    }
}

// ---------------------------------------------------------------------------
// Round 1 additions
// ---------------------------------------------------------------------------

/// `native void nativeSave(long handle, String path)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeSave<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    path: JString<'a>,
) {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return;
    }
    let path_str: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("path string error: {e}"));
            return;
        }
    };
    let doc = unsafe { from_handle(handle) };

    // If lopdf was initialized (mutations exist), save lopdf state; else write original bytes.
    let guard = doc.lopdf.lock().unwrap();
    if let Some(ref lopdf_doc) = *guard {
        let mut buf = Vec::new();
        let mut doc_clone = lopdf_doc.clone();
        if let Err(e) = doc_clone.save_to(&mut buf) {
            drop(guard);
            throw_pdf_exception(&mut env, &format!("save failed: {e}"));
            return;
        }
        drop(guard);
        if let Err(e) = std::fs::write(&path_str, &buf) {
            throw_pdf_exception(&mut env, &format!("write failed: {e}"));
        }
    } else {
        drop(guard);
        if let Err(e) = std::fs::write(&path_str, &doc.raw_bytes) {
            throw_pdf_exception(&mut env, &format!("write failed: {e}"));
        }
    }
}

// ---------------------------------------------------------------------------
// Round 2: Form fields
// ---------------------------------------------------------------------------

/// `native String[] nativeGetFormFields(long handle)`
///
/// Returns a flat String[] with stride 4: [name, type, value, page] per field.
/// `value` is "" if absent.  `page` is "-1" if unknown.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeGetFormFields<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
) -> jobject {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return JObject::null().into_raw();
    }
    let doc = unsafe { from_handle(handle) };
    let Some(tree) = parse_acroform(doc.engine.pdf()) else {
        // No AcroForm — return empty array.
        return match new_string_array(&mut env, &[]) {
            Ok(arr) => arr.into_raw(),
            Err(e) => {
                throw_pdf_exception(&mut env, &format!("array creation error: {e}"));
                JObject::null().into_raw()
            }
        };
    };

    let mut items: Vec<String> = Vec::new();
    for id in tree.terminal_fields() {
        let name = tree.fully_qualified_name(id);
        let field_type = tree
            .effective_field_type(id)
            .map(|ft| match ft {
                FieldType::Text => "text",
                FieldType::Button => "button",
                FieldType::Choice => "choice",
                FieldType::Signature => "signature",
            })
            .unwrap_or("unknown")
            .to_string();
        let value = tree
            .effective_value(id)
            .map(|v| match v {
                FieldValue::Text(s) => s.clone(),
                FieldValue::StringArray(a) => a.join(", "),
            })
            .unwrap_or_default();
        let page = tree
            .get(id)
            .page_index
            .map(|p| p.to_string())
            .unwrap_or_else(|| "-1".to_string());
        items.push(name);
        items.push(field_type);
        items.push(value);
        items.push(page);
    }

    match new_string_array(&mut env, &items) {
        Ok(arr) => arr.into_raw(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("array creation error: {e}"));
            JObject::null().into_raw()
        }
    }
}

/// `native boolean nativeSetFormField(long handle, String name, String value)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeSetFormField<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    name: JString<'a>,
    value: JString<'a>,
) -> jboolean {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return JNI_FALSE;
    }
    let doc = unsafe { from_handle(handle) };

    let name_str: String = match env.get_string(&name) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("name string error: {e}"));
            return JNI_FALSE;
        }
    };
    let value_str: String = match env.get_string(&value) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("value string error: {e}"));
            return JNI_FALSE;
        }
    };

    // No form at all → field cannot be found; preserve the boolean
    // contract (false) instead of throwing.
    if parse_acroform(doc.engine.pdf()).is_none() {
        return JNI_FALSE;
    }

    let mut guard = match ensure_lopdf(doc) {
        Ok(g) => g,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("lopdf init failed: {e}"));
            return JNI_FALSE;
        }
    };
    let lopdf_doc = guard.as_mut().unwrap();

    // Single SDK writeback chain (pdf_forms::apply_field_value): correct /V
    // encoding (ASCII literal else UTF-16BE+BOM), /V-as-Name for buttons,
    // per-widget /AS sync, /AP regeneration, /Kids-recursive FQN lookup, and
    // read-only rejection.
    match apply_string_value(lopdf_doc, &name_str, &value_str) {
        Ok(_) => JNI_TRUE,
        Err(WritebackError::FieldNotFound(_)) => JNI_FALSE,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("setFormField '{name_str}': {e}"));
            JNI_FALSE
        }
    }
}

/// Apply a string value with type-aware dispatch, mirroring the CLI's
/// `fill_one` (crates/xfa-cli/src/cmd_fill.rs): try Text first (the common
/// case), then on a `/FT` type mismatch fall through to Radio, Choice, and
/// finally Checkbox with a bool-ish string.
fn apply_string_value(
    doc: &mut LopdfDocument,
    name: &str,
    value: &str,
) -> Result<WriteOutcome, WritebackError> {
    match apply_field_value(doc, name, WriteValue::Text(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    match apply_field_value(doc, name, WriteValue::Radio(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    match apply_field_value(doc, name, WriteValue::Choice(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    // Checkbox via bool-ish string ("true"/"Yes"/"Off"/"false").
    let on = !matches!(value, "false" | "Off" | "0" | "");
    apply_field_value(doc, name, WriteValue::Checkbox(on))
}

// ---------------------------------------------------------------------------
// Round 2: Annotations
// ---------------------------------------------------------------------------

/// `native String[] nativeGetAnnotations(long handle, int page)`
///
/// Returns a flat String[] with stride 7 per annotation:
/// [type, x0, y0, x1, y1, contents, author].
/// Coordinates are in PDF user-space points.  Missing values are "".
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeGetAnnotations<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    page: jint,
) -> jobject {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return JObject::null().into_raw();
    }
    let doc = unsafe { from_handle(handle) };
    let page_idx = page as usize;
    let pdf_pages = doc.engine.pdf().pages();

    if page_idx >= pdf_pages.len() {
        throw_pdf_exception(
            &mut env,
            &format!("page {page_idx} out of range ({} pages)", pdf_pages.len()),
        );
        return JObject::null().into_raw();
    }

    let raw_annots = Annotation::from_page(&pdf_pages[page_idx]);
    let mut items: Vec<String> = Vec::new();
    for a in raw_annots {
        let annot_type = format!("{:?}", a.annotation_type());
        let (x0, y0, x1, y1) = a
            .rect()
            .map(|r| (r.x0, r.y0, r.x1, r.y1))
            .unwrap_or((0.0, 0.0, 0.0, 0.0));
        items.push(annot_type);
        items.push(x0.to_string());
        items.push(y0.to_string());
        items.push(x1.to_string());
        items.push(y1.to_string());
        items.push(a.contents().unwrap_or_default());
        items.push(a.author().unwrap_or_default());
    }

    match new_string_array(&mut env, &items) {
        Ok(arr) => arr.into_raw(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("array creation error: {e}"));
            JObject::null().into_raw()
        }
    }
}

/// `native void nativeAddAnnotation(long handle, int page, String type,
///                                  double x0, double y0, double x1, double y1,
///                                  String content)`
///
/// `type` must be `"highlight"` or `"freetext"`.
/// `page` is 0-based.  Rect coordinates in PDF user-space points.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeAddAnnotation<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    page: jint,
    annot_type: JString<'a>,
    x0: jdouble,
    y0: jdouble,
    x1: jdouble,
    y1: jdouble,
    content: JString<'a>,
) {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return;
    }
    let doc = unsafe { from_handle(handle) };

    let type_str: String = match env.get_string(&annot_type) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("type string error: {e}"));
            return;
        }
    };
    let content_str: String = match env.get_string(&content) {
        Ok(s) => s.into(),
        Err(_) => String::new(),
    };

    let ar = AnnotRect::new(x0, y0, x1, y1);
    let builder = match type_str.to_lowercase().as_str() {
        "highlight" => {
            let b = AnnotationBuilder::highlight(ar).quad_points_from_rect(&ar);
            if content_str.is_empty() {
                b
            } else {
                b.contents(&content_str)
            }
        }
        "freetext" | "free_text" => AnnotationBuilder::free_text(ar, &content_str, 12.0),
        other => {
            throw_pdf_exception(
                &mut env,
                &format!(
                    "unsupported annotation type {other:?}; supported: \"highlight\", \"freetext\""
                ),
            );
            return;
        }
    };

    let mut guard = match ensure_lopdf(doc) {
        Ok(g) => g,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("lopdf init failed: {e}"));
            return;
        }
    };
    let lopdf_doc = guard.as_mut().unwrap();

    let annot_id = match builder.build(lopdf_doc) {
        Ok(id) => id,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("annotation build failed: {e:?}"));
            return;
        }
    };

    let page_1based = (page + 1) as u32;
    if let Err(e) = add_annotation_to_page(lopdf_doc, page_1based, annot_id) {
        throw_pdf_exception(&mut env, &format!("add annotation to page failed: {e:?}"));
    }
}

// ---------------------------------------------------------------------------
// Round 2: Redaction
// ---------------------------------------------------------------------------

/// `native int[] nativeRedactText(long handle, int page, String term)`
///
/// Returns int[3]: [matchesFound, areasRedacted, pagesAffected].
/// Pass `page = -1` to search all pages.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeRedactText<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    page: jint,
    term: JString<'a>,
) -> jobject {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return JObject::null().into_raw();
    }
    let doc = unsafe { from_handle(handle) };

    let term_str: String = match env.get_string(&term) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("term string error: {e}"));
            return JObject::null().into_raw();
        }
    };

    let mut options = RedactSearchOptions::default();
    if page >= 0 {
        options = options.pages(vec![(page + 1) as u32]);
    }

    let mut guard = match ensure_lopdf(doc) {
        Ok(g) => g,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("lopdf init failed: {e}"));
            return JObject::null().into_raw();
        }
    };
    let lopdf_doc = guard.as_mut().unwrap();

    let report = match search_and_redact(lopdf_doc, &term_str, &options) {
        Ok(r) => r,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("redaction failed: {e}"));
            return JObject::null().into_raw();
        }
    };

    let result = [
        report.matches_found as i32,
        report.areas_redacted as i32,
        report.pages_affected as i32,
    ];
    match env.new_int_array(3) {
        Ok(arr) => {
            let _ = env.set_int_array_region(&arr, 0, &result);
            arr.into_raw()
        }
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("array creation error: {e}"));
            JObject::null().into_raw()
        }
    }
}

// ---------------------------------------------------------------------------
// Round 2: Encryption
// ---------------------------------------------------------------------------

/// `native void nativeEncrypt(long handle, String outputPath, String password)`
///
/// Saves an RC4-128-encrypted copy of the document to `outputPath`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeEncrypt<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    output_path: JString<'a>,
    password: JString<'a>,
) {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return;
    }
    let doc = unsafe { from_handle(handle) };

    let out_str: String = match env.get_string(&output_path) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("output path error: {e}"));
            return;
        }
    };
    let pw_str: String = match env.get_string(&password) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("password error: {e}"));
            return;
        }
    };

    let mut guard = match ensure_lopdf(doc) {
        Ok(g) => g,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("lopdf init failed: {e}"));
            return;
        }
    };
    let lopdf_doc = guard.as_mut().unwrap();

    // PDF encryption requires a /ID in the trailer.
    if lopdf_doc.trailer.get(b"ID").is_err() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(12_345_678);
        let mut id = [0u8; 16];
        let seed_bytes = seed.to_le_bytes();
        for (i, b) in id.iter_mut().enumerate() {
            *b = seed_bytes[i % 4].wrapping_add(i as u8);
        }
        let id_obj = LopdfObject::String(id.to_vec(), StringFormat::Hexadecimal);
        lopdf_doc
            .trailer
            .set("ID", LopdfObject::Array(vec![id_obj.clone(), id_obj]));
    }

    // V2 = RC4-128, revision 3 — well-supported for read-back.
    let state = match lopdf::EncryptionState::try_from(EncryptionVersion::V2 {
        document: lopdf_doc,
        owner_password: &pw_str,
        user_password: &pw_str,
        key_length: 128,
        permissions: LopdfPermissions::all(),
    }) {
        Ok(s) => s,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("encryption setup failed: {e}"));
            return;
        }
    };

    if let Err(e) = lopdf_doc.encrypt(&state) {
        throw_pdf_exception(&mut env, &format!("encryption failed: {e}"));
        return;
    }

    if let Err(e) = lopdf_doc.save(&out_str) {
        throw_pdf_exception(&mut env, &format!("save failed: {e}"));
    }
}

/// `native void nativeDecrypt(long handle, String outputPath, String password)`
///
/// Loads the document with the given password, strips encryption, and saves
/// to `outputPath` as a plain (unencrypted) PDF.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeDecrypt<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    output_path: JString<'a>,
    password: JString<'a>,
) {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return;
    }
    let doc = unsafe { from_handle(handle) };

    let out_str: String = match env.get_string(&output_path) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("output path error: {e}"));
            return;
        }
    };
    let pw_str: String = match env.get_string(&password) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("password error: {e}"));
            return;
        }
    };

    let mut decrypted_doc = match LopdfDocument::load_mem_with_password(&doc.raw_bytes, &pw_str) {
        Ok(d) => d,
        Err(e) => {
            throw_pdf_exception(
                &mut env,
                &format!("failed to open document with password: {e}"),
            );
            return;
        }
    };

    remove_encryption(&mut decrypted_doc);

    if let Err(e) = decrypted_doc.save(&out_str) {
        throw_pdf_exception(&mut env, &format!("save failed: {e}"));
    }
}

// ---------------------------------------------------------------------------
// JNI exports — com.xfa.pdf.PdfUtils (static utility methods)
// ---------------------------------------------------------------------------

/// `static native void nativeMergePdfs(String[] paths, String outputPath)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfUtils_nativeMergePdfs<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    paths: JObjectArray<'a>,
    output_path: JString<'a>,
) {
    let path_strings = match read_string_array(&mut env, &paths) {
        Ok(v) => v,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("failed to read paths array: {e}"));
            return;
        }
    };
    let out_str: String = match env.get_string(&output_path) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("output path error: {e}"));
            return;
        }
    };

    match pages::merge(&path_strings) {
        Ok(mut merged) => {
            if let Err(e) = merged.save(&out_str) {
                throw_pdf_exception(&mut env, &format!("save failed: {e}"));
            }
        }
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("merge failed: {e}"));
        }
    }
}

/// `static native String[] nativeValidatePdfa(String path, String level)`
///
/// Returns a flat String[] encoding the compliance report:
/// [0] compliant ("true"/"false"), [1] errorCount, [2] warningCount,
/// then for each issue: rule, severity ("error"/"warning"/"info"), message.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfUtils_nativeValidatePdfa<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    path: JString<'a>,
    level: JString<'a>,
) -> jobject {
    let path_str: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("path string error: {e}"));
            return JObject::null().into_raw();
        }
    };
    let level_str: String = match env.get_string(&level) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("level string error: {e}"));
            return JObject::null().into_raw();
        }
    };

    let bytes = match std::fs::read(&path_str) {
        Ok(b) => b,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("file read failed: {e}"));
            return JObject::null().into_raw();
        }
    };
    let engine_doc = match PdfDocument::open(bytes) {
        Ok(d) => d,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("PDF open failed: {e}"));
            return JObject::null().into_raw();
        }
    };

    let pdfa_level = parse_pdfa_level(&level_str);
    let report = compliance_validate_pdfa(engine_doc.pdf(), pdfa_level);

    let mut items: Vec<String> = Vec::new();
    items.push(report.compliant.to_string());
    items.push(report.error_count().to_string());
    items.push(report.warning_count().to_string());
    for issue in &report.issues {
        items.push(issue.rule.clone());
        items.push(
            match issue.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Info => "info",
            }
            .to_string(),
        );
        items.push(issue.message.clone());
    }

    match new_string_array(&mut env, &items) {
        Ok(arr) => arr.into_raw(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("array creation error: {e}"));
            JObject::null().into_raw()
        }
    }
}

// ---------------------------------------------------------------------------
// License activation — JNI surface for com.xfa.pdf.PdfluentLicensing
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicU8, Ordering as AtomicOrdering};

static JAVA_LICENSE_SOURCE: AtomicU8 = AtomicU8::new(0);

fn java_record_explicit() {
    JAVA_LICENSE_SOURCE.store(2, AtomicOrdering::Relaxed);
}

fn java_current_source() -> &'static str {
    match JAVA_LICENSE_SOURCE.load(AtomicOrdering::Relaxed) {
        2 => "Explicit",
        _ => {
            if let Ok(key) = std::env::var("PDFLUENT_LICENSE_KEY") {
                if !key.is_empty() && pdfluent::license_info().tier != pdfluent::Tier::Trial {
                    return "EnvVar";
                }
            }
            "Default"
        }
    }
}

fn java_tier_to_int(t: pdfluent::Tier) -> jint {
    match t {
        pdfluent::Tier::Trial => 0,
        pdfluent::Tier::Developer => 1,
        pdfluent::Tier::Team => 2,
        pdfluent::Tier::Business => 3,
        pdfluent::Tier::Enterprise => 4,
        _ => -1,
    }
}

fn java_source_to_int(s: &str) -> jint {
    match s {
        "EnvVar" => 1,
        "Explicit" => 2,
        _ => 0,
    }
}

fn map_license_error_for_java(env: &mut JNIEnv<'_>, e: pdfluent::Error) {
    // Canonical C8 codes — see docs/error_catalogue.md and Rust
    // `pdfluent::Error::code()` for the source of truth.
    const LICENSE_EXCEPTION: &str = "com/pdfluent/PdfluentLicenseException";
    const E_LICENSE_INVALID: &str = "E-LICENSE-INVALID";
    const E_LICENSE_EXPIRED: &str = "E-LICENSE-EXPIRED";
    const E_LICENSE_INVALID_SIGNATURE: &str = "E-LICENSE-INVALID-SIGNATURE";
    const E_LICENSE_FEATURE_NOT_IN_TIER: &str = "E-LICENSE-FEATURE-NOT-IN-TIER";
    const E_LICENSE_CAPABILITY_NOT_COMPILED: &str = "E-LICENSE-CAPABILITY-NOT-COMPILED";

    match e {
        pdfluent::Error::InvalidLicense { reason } => {
            throw_pdf_exception_with_code(
                env,
                LICENSE_EXCEPTION,
                &format!("invalid license: {reason}"),
                E_LICENSE_INVALID,
            );
        }
        pdfluent::Error::LicenseExpired { expires_at } => {
            throw_pdf_exception_with_code(
                env,
                LICENSE_EXCEPTION,
                &format!("license expired at unix timestamp {expires_at}"),
                E_LICENSE_EXPIRED,
            );
        }
        pdfluent::Error::LicenseInvalidSignature => {
            throw_pdf_exception_with_code(
                env,
                LICENSE_EXCEPTION,
                "license signature does not verify against the configured public key",
                E_LICENSE_INVALID_SIGNATURE,
            );
        }
        pdfluent::Error::FeatureNotInTier { .. } => {
            throw_pdf_exception_with_code(
                env,
                LICENSE_EXCEPTION,
                &format!("license error: {e}"),
                E_LICENSE_FEATURE_NOT_IN_TIER,
            );
        }
        pdfluent::Error::CapabilityNotCompiled { .. } => {
            throw_pdf_exception_with_code(
                env,
                LICENSE_EXCEPTION,
                &format!("license error: {e}"),
                E_LICENSE_CAPABILITY_NOT_COMPILED,
            );
        }
        other => throw_pdf_exception(env, &format!("license error: {other}")),
    }
}

/// `static native void nativeActivateKey(String key)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentLicensing_nativeActivateKey<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    key: JString<'a>,
) {
    let key_str: String = match env.get_string(&key) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("failed to read key: {e}"));
            return;
        }
    };
    match pdfluent::set_license_key(&key_str) {
        Ok(()) => java_record_explicit(),
        Err(e) => map_license_error_for_java(&mut env, e),
    }
}

/// `static native void nativeActivateFile(String path)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentLicensing_nativeActivateFile<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    path: JString<'a>,
) {
    let path_str: String = match env.get_string(&path) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("failed to read path: {e}"));
            return;
        }
    };
    let contents = match std::fs::read_to_string(&path_str) {
        Ok(c) => c,
        Err(e) => {
            let _ = env.throw_new(
                "java/io/IOException",
                format!("could not read license file: {e}"),
            );
            return;
        }
    };
    match pdfluent::set_license_key(contents.trim()) {
        Ok(()) => java_record_explicit(),
        Err(e) => map_license_error_for_java(&mut env, e),
    }
}

/// `static native void nativeSetPublicKey(byte[] key)`
///
/// Configure the 32-byte Ed25519 public key for signed-payload verification.
/// Must be called before [`Java_com_pdfluent_PdfluentLicensing_nativeActivatePayload`].
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentLicensing_nativeSetPublicKey<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    key: JByteArray<'a>,
) {
    let key_bytes: Vec<u8> = match env.convert_byte_array(key) {
        Ok(b) => b,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("failed to read key bytes: {e}"));
            return;
        }
    };
    if let Err(e) = pdfluent::set_license_public_key(&key_bytes) {
        map_license_error_for_java(&mut env, e);
    }
}

/// `static native void nativeActivatePayload(String payloadJson)`
///
/// Activate a cryptographically-signed JSON license payload (SDK 1.1+).
/// [`Java_com_pdfluent_PdfluentLicensing_nativeSetPublicKey`] must be called first.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentLicensing_nativeActivatePayload<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    payload_json: JString<'a>,
) {
    let json_str: String = match env.get_string(&payload_json) {
        Ok(s) => s.into(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("failed to read payload JSON: {e}"));
            return;
        }
    };
    match pdfluent::set_license_payload(&json_str) {
        Ok(()) => java_record_explicit(),
        Err(e) => map_license_error_for_java(&mut env, e),
    }
}

/// `static native int nativeEffectiveTier()`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentLicensing_nativeEffectiveTier(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    java_tier_to_int(pdfluent::license_info().tier)
}

/// `static native int[] nativeStatus()` — returns `[tier, source, outputIsMarked]`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentLicensing_nativeStatus<'a>(
    env: JNIEnv<'a>,
    _class: JClass<'a>,
) -> jobject {
    let info = pdfluent::license_info();
    let arr = match env.new_int_array(3) {
        Ok(a) => a,
        Err(_) => return JObject::null().into_raw(),
    };
    let values: [jint; 3] = [
        java_tier_to_int(info.tier),
        java_source_to_int(java_current_source()),
        if info.output_is_marked { 1 } else { 0 },
    ];
    if env.set_int_array_region(&arr, 0, &values).is_err() {
        return JObject::null().into_raw();
    }
    arr.into_raw()
}

/// `native String[] nativeExtractTextBlocks(long handle, int pageIndex)`
///
/// Returns a flat `String[]` with stride 5: `[x, y, width, height, text]` per
/// text block. The bounding box is the union of all spans in the block.
/// Returns an empty array for pages with no text.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_pdfluent_PdfluentDocument_nativeExtractTextBlocks<'a>(
    mut env: JNIEnv<'a>,
    _class: JClass<'a>,
    handle: jlong,
    page_index: jint,
) -> jobject {
    if handle == 0 {
        throw_pdf_exception(&mut env, "document is closed");
        return JObject::null().into_raw();
    }
    let doc = unsafe { from_handle(handle) };
    let blocks = match doc.engine.extract_text_blocks(page_index as usize) {
        Ok(b) => b,
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("text-block extraction failed: {e}"));
            return JObject::null().into_raw();
        }
    };
    let mut items: Vec<String> = Vec::with_capacity(blocks.len() * 5);
    for block in &blocks {
        let (mut x_min, mut y_min) = (f64::INFINITY, f64::INFINITY);
        let (mut x_max, mut y_max) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for span in &block.spans {
            if span.x < x_min {
                x_min = span.x;
            }
            if span.y < y_min {
                y_min = span.y;
            }
            let right = span.x + span.width;
            let top = span.y + span.height;
            if right > x_max {
                x_max = right;
            }
            if top > y_max {
                y_max = top;
            }
        }
        if !x_min.is_finite() {
            x_min = 0.0;
            y_min = 0.0;
            x_max = 0.0;
            y_max = 0.0;
        }
        items.push(x_min.to_string());
        items.push(y_min.to_string());
        items.push((x_max - x_min).max(0.0).to_string());
        items.push((y_max - y_min).max(0.0).to_string());
        items.push(block.text());
    }
    match new_string_array(&mut env, &items) {
        Ok(arr) => arr.into_raw(),
        Err(e) => {
            throw_pdf_exception(&mut env, &format!("array creation error: {e}"));
            JObject::null().into_raw()
        }
    }
}

#[cfg(test)]
mod writeback_dispatch_tests {
    //! Pin the new writeback behavior of `nativeSetFormField`'s dispatch
    //! helper: correct /V encoding (ASCII literal else UTF-16BE+BOM),
    //! /V-as-Name + /AS sync for buttons, and read-only rejection —
    //! replacing the old raw-bytes /V write + bogus "NeedsAppearances" key.

    use super::apply_string_value;
    use lopdf::{dictionary, Document, Object, Stream};
    use pdf_forms::WritebackError;

    /// Minimal indirect-AcroForm document: text field, read-only text
    /// field, and a checkbox whose on-state (`On1`) lives on a kid widget.
    fn form_doc() -> Document {
        let mut doc = Document::with_version("1.4");
        let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let pages_id = doc.new_object_id();

        let text_field = doc.add_object(dictionary! {
            "FT" => "Tx",
            "T" => Object::string_literal("first_name"),
            "V" => Object::string_literal(""),
        });
        // /Ff bit 1 = ReadOnly.
        let readonly_field = doc.add_object(dictionary! {
            "FT" => "Tx",
            "Ff" => 1i64,
            "T" => Object::string_literal("locked"),
            "V" => Object::string_literal("frozen"),
        });
        let checkbox_kid = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "Rect" => vec![100.into(), 700.into(), 115.into(), 715.into()],
            "AP" => dictionary! {
                "N" => dictionary! {
                    "Off" => Object::Null,
                    "On1" => Object::Null,
                },
            },
        });
        let checkbox_field = doc.add_object(dictionary! {
            "FT" => "Btn",
            "T" => Object::string_literal("subscribe"),
            "V" => Object::Name(b"Off".to_vec()),
            "Kids" => vec![checkbox_kid.into()],
        });

        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! {},
            "Annots" => vec![checkbox_kid.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let acroform_id = doc.add_object(dictionary! {
            "Fields" => vec![
                text_field.into(),
                readonly_field.into(),
                checkbox_field.into(),
            ],
        });
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    fn field_v(doc: &Document, name: &str) -> Object {
        doc.objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .find(|d| {
                d.get(b"T")
                    .ok()
                    .and_then(|t| lopdf::decode_text_string(t).ok())
                    .as_deref()
                    == Some(name)
            })
            .and_then(|d| d.get(b"V").ok())
            .cloned()
            .unwrap_or_else(|| panic!("field '{name}' has no /V"))
    }

    #[test]
    fn ascii_text_stays_literal() {
        let mut doc = form_doc();
        apply_string_value(&mut doc, "first_name", "Jane").expect("apply ASCII");
        match field_v(&doc, "first_name") {
            Object::String(v, _) => assert_eq!(v, b"Jane"),
            other => panic!("expected /V string, got {other:?}"),
        }
    }

    #[test]
    fn non_ascii_text_writes_utf16be_bom() {
        let mut doc = form_doc();
        apply_string_value(&mut doc, "first_name", "Café").expect("apply non-ASCII");
        match field_v(&doc, "first_name") {
            Object::String(v, _) => assert!(
                v.starts_with(&[0xFE, 0xFF]),
                "non-ASCII /V must be UTF-16BE with BOM, got {v:02X?}"
            ),
            other => panic!("expected /V string, got {other:?}"),
        }
    }

    #[test]
    fn checkbox_dispatch_sets_name_value_and_widget_as() {
        let mut doc = form_doc();
        let outcome = apply_string_value(&mut doc, "subscribe", "true").expect("apply checkbox");
        assert_eq!(field_v(&doc, "subscribe"), Object::Name(b"On1".to_vec()));
        assert!(
            outcome.appearance_states_set >= 1,
            "kid widget /AS must be synced"
        );
        let widget_synced = doc
            .objects
            .values()
            .filter_map(|o| o.as_dict().ok())
            .filter(|d| d.has(b"AP") && !d.has(b"T"))
            .any(|d| matches!(d.get(b"AS"), Ok(Object::Name(n)) if n == b"On1"));
        assert!(widget_synced, "kid widget /AS must be the on-state On1");
    }

    #[test]
    fn checkbox_dispatch_bool_ish_off_strings() {
        let mut doc = form_doc();
        apply_string_value(&mut doc, "subscribe", "true").expect("check");
        apply_string_value(&mut doc, "subscribe", "false").expect("uncheck");
        assert_eq!(field_v(&doc, "subscribe"), Object::Name(b"Off".to_vec()));
    }

    #[test]
    fn readonly_field_is_rejected() {
        let mut doc = form_doc();
        let err = apply_string_value(&mut doc, "locked", "new value")
            .expect_err("read-only field must be rejected");
        assert!(
            matches!(err, WritebackError::ReadOnly(ref n) if n == "locked"),
            "expected ReadOnly error, got {err:?}"
        );
        match field_v(&doc, "locked") {
            Object::String(v, _) => assert_eq!(v, b"frozen", "value must be unchanged"),
            other => panic!("expected /V string, got {other:?}"),
        }
    }
}
