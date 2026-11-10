plugins {
    application
}

repositories {
    mavenCentral()
}

java {
    sourceCompatibility = JavaVersion.VERSION_1_8
    targetCompatibility = JavaVersion.VERSION_1_8
}

dependencies {
    implementation("io.github.emmby:lavi:0.3.0")
}

application {
    mainClass.set("Example")
}

// Note: The published `io.github.emmby:lavi` JAR on Maven Central bundles and
// auto-extracts the native library automatically. `lavi.library.path` below is
// only used when running against the local monorepo `target/debug` build.
tasks.named<JavaExec>("run") {
    systemProperty("lavi.library.path", "${projectDir}/../../target/debug")
}
