plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

// Release signing comes from environment variables in CI (see README.md).
// Without them, builds use the committed development key, so dev builds
// still install over each other on your TV.
val releaseKeystore: String? = System.getenv("KINORA_KEYSTORE_FILE")

android {
    namespace = "app.kinora.tv"
    compileSdk = 36

    defaultConfig {
        applicationId = "app.kinora.tv"
        minSdk = 26
        targetSdk = 36
        versionCode = (System.getenv("KINORA_VERSION_CODE") ?: "1").toInt()
        versionName = System.getenv("KINORA_VERSION_NAME") ?: "0.1.0-dev"
    }

    signingConfigs {
        getByName("debug") {
            storeFile = rootProject.file("signing/dev.keystore")
            storePassword = "kinora-dev"
            keyAlias = "kinora-dev"
            keyPassword = "kinora-dev"
        }
        create("release") {
            if (releaseKeystore != null) {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("KINORA_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("KINORA_KEY_ALIAS")
                keyPassword = System.getenv("KINORA_KEY_PASSWORD")
            } else {
                storeFile = rootProject.file("signing/dev.keystore")
                storePassword = "kinora-dev"
                keyAlias = "kinora-dev"
                keyPassword = "kinora-dev"
            }
        }
    }

    buildTypes {
        getByName("release") {
            // Shrinking is off for now to keep early builds simple to debug.
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    lint {
        // TV-specific checks (banner, leanback launcher, no touch requirement) stay on.
        abortOnError = true
        checkReleaseBuilds = false
        disable += setOf("GradleDependency", "NewerVersionAvailable", "AndroidGradlePluginVersion")
    }
}

kotlin {
    compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) }
}

dependencies {
    implementation(project(":core:playback"))

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    implementation(libs.tv.material)

    implementation(libs.activity.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.navigation.compose)
    implementation(libs.datastore.preferences)
    implementation(libs.core.ktx)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.media3.ui)
    implementation(libs.coil.compose)
    implementation(libs.coil.network.okhttp)

    testImplementation(libs.junit)
}
