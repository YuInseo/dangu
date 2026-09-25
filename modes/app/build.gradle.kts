plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

val keystoreFile = System.getenv("MODES_KEYSTORE")?.takeIf { it.isNotBlank() }

android {
    namespace = "com.dangu.modes"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.dangu.modes"
        minSdk = 29
        targetSdk = 35
        versionCode = (System.getenv("MODES_VERSION_CODE") ?: "1").toInt()
        versionName = System.getenv("MODES_VERSION_NAME") ?: "0.1.0"
    }

    signingConfigs {
        if (keystoreFile != null) {
            create("release") {
                storeFile = file(keystoreFile)
                storePassword = System.getenv("MODES_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("MODES_KEY_ALIAS")
                keyPassword = System.getenv("MODES_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = if (keystoreFile != null) signingConfigs.getByName("release") else signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    lint { checkReleaseBuilds = false }
    buildFeatures { compose = true }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-core")

    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.fragment:fragment-ktx:1.8.5")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    // 비밀 바탕화면 잠금(지문·얼굴·기기 PIN)
    implementation("androidx.biometric:biometric:1.1.0")
}
