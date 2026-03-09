//! Java JNI bindings for the XFA PDF engine.
//!
//! Exposes `PdfDocument` functionality to Java via JNI:
//! open, page count, page geometry, render, text extraction, metadata.

use std::sync::Arc;

use jni::objects::{JByteArray, JClass, JObject, JString};
use jni::sys::{jdouble, jint, jlong, jobject};
use jni::JNIEnv;

use pdf_engine::{PdfDocument, RenderOptions, ThumbnailOptions};

/// Store a PdfDocument pointer as a Java `long` handle.
fn to_handle(doc: PdfDocument) -> jlong {
    let boxed = Box::new(Arc::new(doc));
    Box::into_raw(boxed) as jlong
}

/// Recover an Arc<PdfDocument> reference from a Java handle.
///
/// # Safety
/// The handle must have been created by `to_handle` and not yet freed.
unsafe fn from_handle(handle: jlong) -> &'static Arc<PdfDocument> {
    &*(handle as *const Arc<PdfDocument>)
}

/// Free a PdfDocument handle.
///
/// # Safety
/// The handle must have been created by `to_handle` and must only be freed once.
unsafe fn free_handle(handle: jlong) {
    let _ = Box::from_raw(handle as *mut Arc<PdfDocument>);
}

/// Throw a Java exception with the given message.
fn throw_pdf_exception(env: &mut JNIEnv, msg: &str) {
    let _ = env.throw_new("com/xfa/pdf/PdfException", msg);
}

// ---------------------------------------------------------------------------
// JNI exports — com.xfa.pdf.PdfDocument
// ---------------------------------------------------------------------------

/// `native long nativeOpen(byte[] data)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativeOpen(
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

    match PdfDocument::open(bytes) {
        Ok(doc) => to_handle(doc),
        Err(e) => {
            throw_pdf_exception(&mut env, &e.to_string());
            0
        }
    }
}

/// `native long nativeOpenWithPassword(byte[] data, String password)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativeOpenWithPassword(
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
            throw_pdf_exception(&mut env, &format!("failed to read password string: {e}"));
            return 0;
        }
    };

    match PdfDocument::open_with_password(bytes, &pw) {
        Ok(doc) => to_handle(doc),
        Err(e) => {
            throw_pdf_exception(&mut env, &e.to_string());
            0
        }
    }
}

/// `native void nativeClose(long handle)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativeClose(
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
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativePageCount(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    if handle == 0 {
        return 0;
    }
    let doc = unsafe { from_handle(handle) };
    doc.page_count() as jint
}

/// `native double nativePageWidth(long handle, int pageIndex)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativePageWidth(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    page_index: jint,
) -> jdouble {
    if handle == 0 {
        return 0.0;
    }
    let doc = unsafe { from_handle(handle) };
    match doc.page_geometry(page_index as usize) {
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
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativePageHeight(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    page_index: jint,
) -> jdouble {
    if handle == 0 {
        return 0.0;
    }
    let doc = unsafe { from_handle(handle) };
    match doc.page_geometry(page_index as usize) {
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
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativePageRotation(
    mut env: JNIEnv,
    _class: JClass,
    handle: jlong,
    page_index: jint,
) -> jint {
    if handle == 0 {
        return 0;
    }
    let doc = unsafe { from_handle(handle) };
    match doc.page_geometry(page_index as usize) {
        Ok(geom) => geom.rotation.degrees() as jint,
        Err(e) => {
            throw_pdf_exception(&mut env, &e.to_string());
            0
        }
    }
}

/// `native String nativeExtractText(long handle, int pageIndex)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativeExtractText<'a>(
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
    match doc.extract_text(page_index as usize) {
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
/// Returns RGBA pixel data. Width/height are encoded in the first 8 bytes
/// as two big-endian i32 values, followed by the raw RGBA pixels.
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativeRenderPage<'a>(
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
    match doc.render_page(page_index as usize, &options) {
        Ok(rendered) => {
            // Pack: [width:4 bytes BE][height:4 bytes BE][RGBA pixels...]
            let w_bytes = (rendered.width as i32).to_be_bytes();
            let h_bytes = (rendered.height as i32).to_be_bytes();
            let total_len = 8 + rendered.pixels.len();
            let mut buf = Vec::with_capacity(total_len);
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
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativeRenderThumbnail<'a>(
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
    match doc.thumbnail(page_index as usize, &options) {
        Ok(rendered) => {
            let w_bytes = (rendered.width as i32).to_be_bytes();
            let h_bytes = (rendered.height as i32).to_be_bytes();
            let total_len = 8 + rendered.pixels.len();
            let mut buf = Vec::with_capacity(total_len);
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
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativeGetMetadata<'a>(
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

    let info = doc.info();
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
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativeBookmarkCount(
    _env: JNIEnv,
    _class: JClass,
    handle: jlong,
) -> jint {
    if handle == 0 {
        return 0;
    }
    let doc = unsafe { from_handle(handle) };
    doc.bookmarks().len() as jint
}

/// `native int[] nativeSearchText(long handle, String query)`
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_xfa_pdf_PdfDocument_nativeSearchText<'a>(
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

    let pages = doc.search_text(&query_str);
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
