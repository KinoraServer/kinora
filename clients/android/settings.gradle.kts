pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "kinora-android"

// Plain Kotlin modules: no Android dependencies, unit-tested on the JVM.
include(":core:model")
include(":core:api")

// Android modules.
include(":core:playback")
include(":app-tv")
