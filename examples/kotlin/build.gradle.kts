plugins {
    kotlin("jvm") version "2.1.10"
    application
}

repositories {
    mavenCentral()
}

java {
    sourceCompatibility = JavaVersion.VERSION_1_8
    targetCompatibility = JavaVersion.VERSION_1_8
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_1_8)
    }
}

dependencies {
    implementation("io.github.emmby:lavi:0.4.0")
}

application {
    mainClass.set("ExampleKt")
}

// Note: The published `io.github.emmby:lavi` JAR on Maven Central bundles and
// auto-extracts the native library automatically. `lavi.library.path` below is
// only used when running against the local monorepo `target/debug` build.
tasks.named<JavaExec>("run") {
    systemProperty("lavi.library.path", "${projectDir}/../../target/debug")
}
