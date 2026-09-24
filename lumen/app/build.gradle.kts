plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

// CI가 서명 키를 넘겨주면 그걸로 서명한다. 없으면 디버그 키.
val keystoreFile = System.getenv("LUMEN_KEYSTORE")?.takeIf { it.isNotBlank() }

android {
    namespace = "com.dangu.lumen"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.dangu.lumen"
        minSdk = 29
        targetSdk = 35
        versionCode = (System.getenv("LUMEN_VERSION_CODE") ?: "1").toInt()
        versionName = System.getenv("LUMEN_VERSION_NAME") ?: "0.1.0"
        val repo = System.getenv("GITHUB_REPOSITORY") ?: "YuInseo/dangu"
        buildConfigField("String", "UPDATE_REPO", "\"$repo\"")
    }

    signingConfigs {
        if (keystoreFile != null) {
            create("release") {
                storeFile = file(keystoreFile)
                storePassword = System.getenv("LUMEN_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("LUMEN_KEY_ALIAS")
                keyPassword = System.getenv("LUMEN_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = if (keystoreFile != null) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    // 릴리스 빌드마다 도는 lintVital을 끈다. CI 시간을 줄이려고.
    lint {
        checkReleaseBuilds = false
    }
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
    // 아이콘 몇 개만 쓴다 — extended(수천 개)는 빌드마다 컴파일·R8 시간을 크게 잡아먹는다.
    implementation("androidx.compose.material:material-icons-core")

    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("androidx.webkit:webkit:1.12.1")
    // 클래식 서랍의 서버 아이콘·DM 프로필 사진
    implementation("io.coil-kt:coil-compose:2.7.0")
}
