plugins {
    id("com.android.application")
}

android {
    namespace = "org.auroraviewer.aurora"
    compileSdk = 35

    defaultConfig {
        applicationId = "org.auroraviewer.aurora"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

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
        }
    }
}
