import groovy.json.JsonSlurper
import java.io.File
import java.util.Properties

plugins {
    id("com.android.application")
}

val hasGoogleServices: Boolean = file("google-services.json").isFile
if (hasGoogleServices) {
    apply(plugin = "com.google.gms.google-services")
    apply(plugin = "com.google.firebase.crashlytics")
}

// Optional downstream branding overrides from AURORA_BRAND_CONFIG or <repo>/brand.local.json
val repoRoot: File = rootProject.projectDir.resolve("../..").canonicalFile
val brandConfigEnv: String? = System.getenv("AURORA_BRAND_CONFIG")?.takeIf { it.isNotBlank() }
val brandFile: File = if (brandConfigEnv != null) {
    val f = File(brandConfigEnv)
    if (f.isAbsolute) f else repoRoot.resolve(brandConfigEnv)
} else {
    repoRoot.resolve("brand.local.json")
}

@Suppress("UNCHECKED_CAST")
val brandMap: Map<String, Any?> = if (brandFile.isFile) {
    (JsonSlurper().parse(brandFile) as? Map<String, Any?>) ?: emptyMap()
} else {
    emptyMap()
}

val brandAppName: String = (brandMap["app_name"] as? String)?.takeIf { it.isNotBlank() } ?: "Aurora Viewer"
val brandPackage: String = (brandMap["android_package"] as? String)?.takeIf { it.isNotBlank() } ?: "org.auroraviewer.aurora"

@Suppress("UNCHECKED_CAST")
val brandIcons: Map<String, Any?> = (brandMap["icons"] as? Map<String, Any?>) ?: emptyMap()
val brandAndroidResDir: File? = (brandIcons["android_res_dir"] as? String)
    ?.takeIf { it.isNotBlank() }
    ?.let { raw ->
        val f = File(raw)
        if (f.isAbsolute) f else repoRoot.resolve(raw)
    }
    ?.takeIf { it.isDirectory }

val keyPropsFile: File = rootProject.file("key.properties")
val keyProps = Properties()
if (keyPropsFile.isFile) {
    keyPropsFile.inputStream().use { keyProps.load(it) }
}
val releaseStoreFile: String? = keyProps.getProperty("storeFile")?.takeIf { it.isNotBlank() }
    ?: System.getenv("CM_KEYSTORE_PATH")?.takeIf { it.isNotBlank() }
val releaseStorePassword: String? = keyProps.getProperty("storePassword")?.takeIf { it.isNotBlank() }
    ?: System.getenv("CM_KEYSTORE_PASSWORD")?.takeIf { it.isNotBlank() }
val releaseKeyAlias: String? = keyProps.getProperty("keyAlias")?.takeIf { it.isNotBlank() }
    ?: System.getenv("CM_KEY_ALIAS")?.takeIf { it.isNotBlank() }
val releaseKeyPassword: String? = keyProps.getProperty("keyPassword")?.takeIf { it.isNotBlank() }
    ?: System.getenv("CM_KEY_PASSWORD")?.takeIf { it.isNotBlank() }
val hasReleaseSigning: Boolean = releaseStoreFile != null
    && releaseStorePassword != null
    && releaseKeyAlias != null
    && releaseKeyPassword != null

val ciBuildNumber: Int? = System.getenv("BUILD_NUMBER")?.toIntOrNull()
    ?: System.getenv("PROJECT_BUILD_NUMBER")?.toIntOrNull()

android {
    namespace = "org.auroraviewer.aurora"
    compileSdk = 36

    defaultConfig {
        applicationId = brandPackage
        minSdk = 26
        targetSdk = 36
        versionCode = ciBuildNumber ?: 1
        versionName = if (ciBuildNumber != null) "0.1.$ciBuildNumber" else "0.1.0"

        resValue("string", "app_name", brandAppName)
        manifestPlaceholders["appIcon"] = if (brandAndroidResDir != null) {
            "@mipmap/ic_launcher_brand"
        } else {
            "@mipmap/ic_launcher"
        }

        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    signingConfigs {
        if (hasReleaseSigning) {
            create("release") {
                storeFile = file(releaseStoreFile!!)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
            ndk {
                debugSymbolLevel = "SYMBOL_TABLE"
            }
            if (hasReleaseSigning) {
                signingConfig = signingConfigs.getByName("release")
            }
            if (hasGoogleServices) {
                extensions.findByType(com.google.firebase.crashlytics.buildtools.gradle.CrashlyticsExtension::class.java)?.apply {
                    nativeSymbolUploadEnabled = true
                    unstrippedNativeLibsDir = file("src/main/jniLibs")
                }
            }
        }
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
            if (brandAndroidResDir != null) {
                res.srcDir(brandAndroidResDir)
            }
        }
    }
}

dependencies {
    implementation("org.rustls:rustls-platform-verifier:0.2.0")
    implementation("org.jetbrains.kotlin:kotlin-stdlib:2.1.0")
    if (hasGoogleServices) {
        implementation(platform("com.google.firebase:firebase-bom:33.10.0"))
        implementation("com.google.firebase:firebase-crashlytics")
        implementation("com.google.firebase:firebase-crashlytics-ndk")
    }
}
