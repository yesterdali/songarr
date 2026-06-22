allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

val newBuildDir: Directory =
    rootProject.layout.buildDirectory
        .dir("../../build")
        .get()
rootProject.layout.buildDirectory.value(newBuildDir)

subprojects {
    val newSubprojectBuildDir: Directory = newBuildDir.dir(project.name)
    project.layout.buildDirectory.value(newSubprojectBuildDir)
}

// Some plugins (e.g. media_store_plus) are still published against an older
// compileSdk than their androidx deps require. Force every Android library
// module up to a modern compileSdk so the AAR metadata check passes. Must be
// registered before `evaluationDependsOn` below forces evaluation.
subprojects {
    afterEvaluate {
        extensions.findByType<com.android.build.gradle.LibraryExtension>()?.let { ext ->
            val current = ext.compileSdkVersion
                ?.substringAfter("android-")
                ?.toIntOrNull()
            if (current == null || current < 34) {
                ext.compileSdk = 36
            }
        }
    }
}

subprojects {
    project.evaluationDependsOn(":app")
}

tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}
