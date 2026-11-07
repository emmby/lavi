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
    implementation("io.lavi:lavi-jvm:0.1.0")
}

application {
    mainClass.set("ExampleKt")
}

tasks.named<JavaExec>("run") {
    systemProperty("lavi.library.path", "${projectDir}/../../target/debug")
}
