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
    implementation("io.github.emmby:lavi:0.1.0")
}

application {
    mainClass.set("Example")
}

tasks.named<JavaExec>("run") {
    systemProperty("lavi.library.path", "${projectDir}/../../target/debug")
}
