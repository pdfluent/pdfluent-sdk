package com.xfa.pdf

/**
 * Native library loader for the PDF JNI bridge on Android.
 *
 * On Android, native libraries are bundled in the APK/AAR under jniLibs/
 * and loaded automatically via [System.loadLibrary].
 */
internal object NativeLoader {
    @Volatile
    private var loaded = false

    @JvmStatic
    fun load() {
        if (loaded) return
        synchronized(this) {
            if (loaded) return
            System.loadLibrary("pdf_java")
            loaded = true
        }
    }
}
