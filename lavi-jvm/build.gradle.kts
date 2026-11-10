import java.security.MessageDigest

plugins {
    `java-library`
    `maven-publish`
    signing
    kotlin("jvm") version "2.1.10"
}

group = "io.github.emmby"
version = "0.3.0"

repositories {
    mavenCentral()
}

java {
    sourceCompatibility = JavaVersion.VERSION_1_8
    targetCompatibility = JavaVersion.VERSION_1_8
    withSourcesJar()
    withJavadocJar()
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_1_8)
    }
}

dependencies {
    testImplementation(platform("org.junit:junit-bom:5.11.4"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
    testImplementation(kotlin("test"))
}

tasks.withType<Javadoc>().configureEach {
    (options as StandardJavadocDocletOptions).addStringOption("Xdoclint:none", "-quiet")
}

tasks.test {
    useJUnitPlatform()
    systemProperty("lavi.library.path", "${rootProject.projectDir}/../target/debug")
}

publishing {
    publications {
        create<MavenPublication>("mavenJava") {
            groupId = "io.github.emmby"
            artifactId = "lavi"
            version = project.version.toString()
            from(components["java"])

            pom {
                name.set("Lavi")
                description.set("Portable XState v5 statechart runtime for Java, Kotlin, and Android.")
                url.set("https://github.com/emmby/lavi")

                licenses {
                    license {
                        name.set("MIT License")
                        url.set("https://opensource.org/licenses/MIT")
                    }
                }

                developers {
                    developer {
                        id.set("emmby")
                        name.set("Mike Burton")
                        url.set("https://github.com/emmby")
                    }
                }

                scm {
                    connection.set("scm:git:git://github.com/emmby/lavi.git")
                    developerConnection.set("scm:git:ssh://github.com:emmby/lavi.git")
                    url.set("https://github.com/emmby/lavi")
                }
            }
        }
    }

    repositories {
        maven {
            name = "staging"
            url = uri(layout.buildDirectory.dir("staging-deploy"))
        }
    }
}

signing {
    val gpgKey = System.getenv("MAVEN_GPG_PRIVATE_KEY")
    val gpgPassphrase = System.getenv("MAVEN_GPG_PASSPHRASE") ?: ""
    isRequired = !gpgKey.isNullOrBlank()
    if (!gpgKey.isNullOrBlank()) {
        useInMemoryPgpKeys(gpgKey, gpgPassphrase)
        sign(publishing.publications["mavenJava"])
    }
}

val generateMavenChecksums = tasks.register("generateMavenChecksums") {
    dependsOn("publishMavenJavaPublicationToStagingRepository")
    val stagingDir = layout.buildDirectory.dir("staging-deploy")
    outputs.upToDateWhen { false }

    doLast {
        val root = stagingDir.get().asFile
        if (!root.exists()) return@doLast

        val algorithms = listOf(
            "MD5" to "md5",
            "SHA-1" to "sha1",
            "SHA-256" to "sha256",
            "SHA-512" to "sha512"
        )

        root.walkTopDown().filter { it.isFile }.forEach { file ->
            val name = file.name
            if (name.startsWith("maven-metadata")) {
                file.delete()
                return@forEach
            }
            if (name.endsWith(".md5") ||
                name.endsWith(".sha1") ||
                name.endsWith(".sha256") ||
                name.endsWith(".sha512") ||
                name.endsWith(".asc")
            ) {
                return@forEach
            }
            val bytes = file.readBytes()
            for ((algo, ext) in algorithms) {
                val digest = MessageDigest.getInstance(algo).digest(bytes)
                val hex = digest.joinToString("") { "%02x".format(it) }
                File(file.parentFile, "${file.name}.$ext").writeText(hex)
            }
        }
    }
}

tasks.register<Zip>("zipMavenCentralBundle") {
    dependsOn(generateMavenChecksums)
    from(layout.buildDirectory.dir("staging-deploy"))
    outputs.upToDateWhen { false }
    archiveFileName.set("lavi-maven-central-bundle.zip")
    destinationDirectory.set(file("${rootProject.projectDir}/../dist/maven"))
}
