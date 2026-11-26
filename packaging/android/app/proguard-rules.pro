# Keep Activity & JNI callbacks invoked from Rust (`android.rs`).
-keep class org.auroraviewer.aurora.** { *; }

# Keep rustls-platform-verifier classes invoked from Rust JNI.
-keep class org.rustls.** { *; }

# Keep all native method names.
-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}
