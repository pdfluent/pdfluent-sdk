# Keep JNI native methods
-keepclasseswithmembernames class com.xfa.pdf.PdfDocument {
    native <methods>;
}

# Keep classes accessed from native code
-keep class com.xfa.pdf.PdfException { *; }
-keep class com.xfa.pdf.RenderedImage { *; }
