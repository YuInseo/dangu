plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// CI 에뮬레이터 확인용 빌드: 비밀 바탕화면의 FLAG_SECURE를 빼서 화면을 찍을 수 있게.
val e2e = System.getenv("MODES_E2E") == "1"

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
        buildConfigField("boolean", "E2E", e2e.toString())
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
    buildFeatures {
        compose = true
        buildConfig = true
    }
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

    testImplementation("junit:junit:4.13.2")
    // 단위 시험에서 진짜 org.json을 쓰려고(안드로이드 것은 시험에선 빈 껍데기다).
    testImplementation("org.json:json:20240303")
}
