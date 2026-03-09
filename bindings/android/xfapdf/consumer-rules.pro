# Keep JNI native methods for consumers of this library
-keepclasseswithmembernames class com.xfa.pdf.PdfDocument {
    native <methods>;
}
-keep class com.xfa.pdf.PdfException { *; }
