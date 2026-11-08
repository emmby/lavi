rootProject.name = "lavi-example-java"

includeBuild("../../lavi-jvm") {
    dependencySubstitution {
        substitute(module("io.github.emmby:lavi")).using(project(":"))
    }
}
