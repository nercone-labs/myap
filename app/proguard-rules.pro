# The Rust core calls no Java, but the JVM binds its native methods by the names of the class
# and the methods (`Java_dev_nercone_myap_core_NativeCore_*`).
-keep class dev.nercone.myap.core.NativeCore { *; }

# kotlinx.serialization: keep the generated serializers of the core models.
-keepclassmembers @kotlinx.serialization.Serializable class dev.nercone.myap.** {
    *** Companion;
    kotlinx.serialization.KSerializer serializer(...);
}
-keep,includedescriptorclasses class dev.nercone.myap.**$$serializer { *; }

# MapLibre calls its Java classes from its native library by name.
-keep class org.maplibre.android.** { *; }
-dontwarn org.maplibre.android.**
