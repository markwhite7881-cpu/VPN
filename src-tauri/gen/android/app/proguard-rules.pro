# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# gomobile / libbox: the Go runtime calls into these Java classes via
# JNI and reflection - obfuscating or stripping them breaks the core.
-keep class go.** { *; }
-keep class io.nekohasekai.** { *; }
-keepclasseswithmembernames class * {
    native <methods>;
}

# Tauri plugin dispatch is reflection-based (@Command methods are
# looked up by name at runtime).
-keepclassmembers class * {
    @app.tauri.annotation.* <methods>;
}
-keep class ru.classquiz.singbox.vpn.** { *; }
