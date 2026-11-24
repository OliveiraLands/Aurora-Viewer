import groovy.json.JsonSlurper
import java.io.File

plugins {
    id("com.android.application")
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

android {
    namespace = "org.auroraviewer.aurora"
    compileSdk = 35

    defaultConfig {
        applicationId = brandPackage
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

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

    buildTypes {
        release {
            isMinifyEnabled = false
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
}
